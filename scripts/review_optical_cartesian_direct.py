"""Audit every numeric record from the distinct timed direct-output workload."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import time

import mpmath as mp

import optical_cartesian_trial_reference as reference
from review_optical_cartesian_trial import ROOT, SELECTED, ATMOSPHERE, IDS, load, digest, words, f32, timing

RECORD_WORDS = 384
RECORD_BYTES = 4*RECORD_WORDS
RECORDS = 11*256*8
OUTPUT_BYTES = RECORD_BYTES*RECORDS
AUDIT_SECONDS = 300


def provenance(prefix):
    source = load(prefix+'-sources.json')
    stable = load(prefix+'-input-stability.json')
    assert source['cartesian_direct'] and not source['cartesian_promoted']
    assert not source['fast_bounds'] and not source['validation_only']
    assert source['compiler_inputs_unchanged'] and source['executable_retained']
    assert stable['inputs_unchanged_through_run']
    before = {str((ROOT/r['path']).resolve()): r['sha256'] for r in source['files']}
    after = {str((ROOT/r['path']).resolve()): r['sha256'] for r in stable['after_run']}
    assert len(before) == len(after)+1 and all(before[p] == h for p, h in after.items())
    assert load(prefix+'-compile-inputs.json')['inputs'] == stable['after_run']
    for row in source['files']:
        assert digest(row['path']) == row['sha256']
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None and not life['timed_out']
    assert not life['terminated_by_wrapper'] and life['executable_retained']
    assert life['executable_unchanged_through_run']
    assert digest(life['executable']) == life['executable_sha256']
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-direct']
    manifests = load(prefix+'-outputs.json')
    outputs = {}
    for row in manifests:
        p = str((ROOT/row['path']).resolve())
        assert p not in outputs
        outputs[p] = row['sha256']
    for suffix in ('.json', '.wgsl', '-inputs.bin', '-uniform.bin', '-sources.json',
                   '-compile-inputs.json', '-compiler.log', '-executable.exe',
                   '-lifecycle.json', '-input-stability.json', '.stdout.log', '.stderr.log'):
        assert digest(prefix+suffix) == outputs[str((ROOT/(prefix+suffix)).resolve())]
    return outputs


def input_gate(prefix, fixture):
    inputs = words(prefix+'-inputs.bin'); assert len(inputs) == 11*36
    # The distinct observer does not change any of the previous frozen inputs.
    assert inputs == words('artifacts/optical-cartesian-trial-validation-second-inputs.bin')
    planned = [(ATMOSPHERE, 1)]+[(SELECTED, i) for i in IDS]
    raw = {}
    for p in (ATMOSPHERE, SELECTED):
        old_manifest = {str((ROOT/x['path']).resolve()): x['sha256'] for x in load(p+'-outputs.json')}
        for suffix in ('.json', '-primary.bin', '-states.bin', '-auxiliary.bin'):
            assert digest(p+suffix) == old_manifest[str((ROOT/(p+suffix)).resolve())]
        raw[p] = words(p+'-primary.bin'), words(p+'-states.bin'), words(p+'-auxiliary.bin')
    rows = []
    for index, (p, identity) in enumerate(planned):
        primary, states, logs = raw[p]
        saved = states[24*identity:24*(identity+1)]
        logbase = 16388*identity; first = logs[logbase+4:logbase+8]
        owner = struct.unpack('<i', struct.pack('<I', saved[20]))[0]
        count = f32(primary[36]); assert count.is_integer() and 2 <= count <= 128
        assert -1 <= owner < int(count)-1
        assert saved[11] == saved[14] == saved[15] == saved[23] == 0
        assert saved[21:23] == [identity, 1]
        assert 0 < logs[logbase] <= 4096 and logs[logbase+1:logbase+4] == [0, identity, primary[46]]
        assert first[1] == saved[3] and first[2] > 0 and first[3] in (0, 1)
        branch = [primary[41], primary[49], 0] if owner < 0 else primary[48+4*owner:51+4*owner]
        expected = primary[:16]+saved[:4]+saved[4:7]+[first[0]]+branch+[saved[20]]+saved[16:20]+[index, identity, first[2], first[3]]
        packed = inputs[36*index:36*(index+1)]; assert packed == expected
        m = fixture['mapping'][index]
        assert m['input_words'] == packed and m['primary_words'] == primary and m['saved_state_words'] == saved
        assert m['source_prefix'] == p and m['record_index'] == identity and m['input_index'] == index
        assert m['source_owner'] == owner and m['first_log_words'] == first and m['log_count'] == logs[logbase]
        target = reference.reference(packed)
        with mp.workdps(reference.DIGITS):
            lower = f32(primary[42] if owner < 0 else primary[48+4*owner])
            upper = f32(primary[48+4*(owner+1)])
            assert mp.mpf(lower) < target[89] < mp.mpf(upper)
        rows.append({'input_words': packed, 'record_index': identity, 'owner_bits': saved[20]})
    return rows


def comparison(packed, payload, precision):
    values = list(struct.unpack('<188d', payload))
    if not all(math.isfinite(v) for v in values):
        return {'complete_finite_valid_trial': False, 'passed_arithmetic_gate': False,
                'nonfinite_scalar_ids': [i for i, v in enumerate(values) if not math.isfinite(v)]}
    if precision == 32:
        assert all(struct.unpack('<f', struct.pack('<f', v))[0] == v for v in values)
    result = reference.compare(packed, values)
    del result['target']
    return result


def audit(prefix, validation_prefix):
    started = time.monotonic()
    output_hashes = provenance(prefix)
    report = load(prefix+'.json'); fixture = report['fixture']
    assert report['cartesian_direct'] and not report['cartesian_promoted'] and not report['fast_bounds']
    assert report['count'] == 11 and report['cost_invocations'] == 2816
    assert report['strictly_increasing_unique_timestamp_pairs']
    assert all(x in report['required_features'] for x in ('SHADER_F64', 'SHADER_INT64', 'TIMESTAMP_QUERY'))
    assert fixture['inner'] == 8 and fixture['repetitions'] == 256
    assert fixture['direct_record_words'] == RECORD_WORDS and fixture['direct_record_count'] == RECORDS
    assert fixture['direct_output_bytes'] == OUTPUT_BYTES
    magic = fixture['direct_record_magic']; assert magic == 0x43544431
    assert fixture['direct_scalar_count'] == 188
    assert fixture['direct_header'] == ['magic', 'invocation', 'cycle', 'input_index',
                                       'source_record', 'owner_bits', 'precision_bits', 'stage_written_mask']
    offline = load(validation_prefix+'-validation.json')
    assert offline['cartesian_direct'] and offline['fixture'] == fixture
    for suffix in ('.wgsl', '-uniform.bin', '-inputs.bin'):
        assert (ROOT/(prefix+suffix)).read_bytes() == (ROOT/(validation_prefix+suffix)).read_bytes()
    assert words(prefix+'-uniform.bin') == [11, 8, 2816, magic]
    inputs = input_gate(prefix, fixture)
    entries = {r['entry']: r for r in report['entries']}
    assert list(entries) == ['direct_reference', 'direct_promoted']
    distinct, locations, captures, seen_paths = {}, [], [], set()
    complete = promoted_ok = True
    observed = 0; first_failure = None; inconclusive = None
    for entry, precision in [('direct_reference', 32), ('direct_promoted', 64)]:
        data = entries[entry]
        assert data['dispatch'] == [88, 1, 1] and data['output_bytes'] == OUTPUT_BYTES
        assert len(data['iterations']) == 8
        for iteration, row in enumerate(data['iterations']):
            assert row['iteration'] == iteration and row['warmup'] == (iteration < 3)
            capture = row['capture']; path = (ROOT/capture['path']).resolve()
            assert str(path) not in seen_paths; seen_paths.add(str(path))
            assert capture['bytes'] == OUTPUT_BYTES and path.stat().st_size == OUTPUT_BYTES
            assert capture['complete_output_retained'] and capture['output_cleared_before_dispatch']
            assert capture['output_captured_after_dispatch']
            assert digest(path) == output_hashes[str(path)]
            with path.open('rb') as stream:
                for ordinal in range(RECORDS):
                    if time.monotonic()-started > AUDIT_SECONDS:
                        inconclusive = 'The fixed 300-second audit budget expired; remaining raw records are unverified.'
                        break
                    raw = stream.read(RECORD_BYTES); assert len(raw) == RECORD_BYTES
                    header = struct.unpack('<8I', raw[:32])
                    invocation, cycle = divmod(ordinal, 8)
                    identity = (invocation+17*cycle) % 11; source = inputs[identity]
                    expected = (magic, invocation, cycle, identity, source['record_index'],
                                source['owner_bits'], precision, 127)
                    assert header == expected, (capture['path'], ordinal, header, expected)
                    payload = raw[32:]
                    key = (precision, identity, payload)  # Full byte equality, not a digest.
                    location = {'entry': entry, 'iteration': iteration, 'record': ordinal,
                                'byte_offset': RECORD_BYTES*ordinal, 'path': capture['path']}
                    if key not in distinct:
                        outcome = comparison(source['input_words'], payload, precision)
                        item = {'precision': precision, 'input_index': identity, 'first_location': location,
                                'payload_sha256': hashlib.sha256(payload).hexdigest(),
                                'observations': 0, 'comparison': outcome}
                        distinct[key] = len(locations); locations.append(item)
                    item = locations[distinct[key]]; item['observations'] += 1
                    outcome = item['comparison']
                    good = outcome['complete_finite_valid_trial'] and (precision == 32 or outcome['passed_arithmetic_gate'])
                    if not good and first_failure is None: first_failure = location
                    complete = complete and outcome['complete_finite_valid_trial']
                    if precision == 64: promoted_ok = promoted_ok and outcome['passed_arithmetic_gate']
                    observed += 1
                if inconclusive is None: assert stream.read(1) == b''
            captures.append({'entry': entry, 'iteration': iteration, 'path': capture['path'],
                             'sha256': output_hashes[str(path)], 'bytes': OUTPUT_BYTES})
            if inconclusive: break
        if inconclusive: break
    exhausted = observed == 16*RECORDS and inconclusive is None
    numerical = exhausted and complete and promoted_ok
    # Reuse only the reviewed exact tick/rational effort calculation, after all
    # current observed numeric records pass. It reads no old experiment output.
    cost = timing({'cost_reference': entries['direct_reference'],
                   'cost_promoted': entries['direct_promoted']}) if numerical else None
    if cost:
        cost['scope'] = 'All numeric stores are timed. Output clearing, readback, validation and file writing are excluded. Store overhead and liveness may compress this ratio; no unobserved core/controller cost or native-FPS inference.'
    maximum = {}
    with mp.workdps(reference.DIGITS):
        for precision in (32, 64):
            pairs = [(mp.mpf(c['difference_metres']), item['input_index'], c['field'])
                     for item in locations if item['precision'] == precision
                     for c in item['comparison'].get('comparisons', [])]
            if pairs:
                value, identity, field = max(pairs)
                maximum[str(precision)] = {'metres': str(value), 'input_index': identity, 'field': field}
    passed = bool(numerical and cost and cost['passed_effort_gate'])
    return {'schema': 'cartesian-direct-output-review-v1', 'prefix': prefix,
            'validation_prefix': validation_prefix, 'auditor_sha256': digest(__file__),
            'reference_sha256': digest('scripts/optical_cartesian_trial_reference.py'),
            'policy_sha256': digest('docs/science/cartesian-direct-trial-policy.md'),
            'source_manifest_sha256': digest(prefix+'-sources.json'),
            'output_manifest_sha256': digest(prefix+'-outputs.json'), 'report_sha256': digest(prefix+'.json'),
            'python': sys.version, 'mpmath_version': mp.__version__, 'precision_digits': reference.DIGITS,
            'expected_records': 16*RECORDS, 'verified_records': observed, 'all_records_verified': exhausted,
            'complete_finite_trials': complete, 'promoted_arithmetic_gate_passed': promoted_ok,
            'numerical_gate_passed': numerical,
            'first_numerical_failure': first_failure, 'inconclusive': inconclusive,
            'distinct_payloads': len(locations), 'maximum_reported_group_errors': maximum,
            'captures': captures, 'payload_comparisons': locations, 'cost': cost, 'passed': passed,
            'decision': ('This fully observed fixed workload permits considering a separately reviewed controller.' if passed else
                         'No controller approval or performance inference; this workload failed or is inconclusive.'),
            'scope': 'All actual captured numeric payloads checked. Exact full-byte equality alone shares a reference comparison. Earlier checksum workload remains rejected. No ODE rerun, complete-ray/contact proof, core-only cost or native FPS conclusion.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix'); parser.add_argument('--validation-prefix', required=True)
    parser.add_argument('--output', required=True); args = parser.parse_args()
    result = audit(args.prefix, args.validation_prefix)
    with (ROOT/args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({k: result[k] for k in ('verified_records', 'distinct_payloads',
        'complete_finite_trials', 'promoted_arithmetic_gate_passed', 'passed', 'decision')}, indent=2))
