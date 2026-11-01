"""Prospective raw arithmetic/checksum/cost gates for one promoted trial run."""
import argparse
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

import mpmath as mp

import optical_cartesian_trial_reference as reference

ROOT = Path(__file__).resolve().parents[1]
SELECTED = 'artifacts/optical-cartesian-suffix-tight-selected-first'
ATMOSPHERE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first'
IDS = [3, 9, 10, 11, 12, 14, 15, 17, 18, 20]


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def digest(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def words(path):
    raw = (ROOT/path).read_bytes()
    assert len(raw) % 4 == 0
    return list(struct.unpack('<'+'I'*(len(raw)//4), raw))


def f32(word):
    return struct.unpack('<f', struct.pack('<I', word))[0]


def source_rows(prefix):
    source = load(prefix+'-sources.json')
    stability = load(prefix+'-input-stability.json')
    assert source['cartesian_promoted'] and not source['fast_bounds'] and not source['validation_only']
    assert source['compiler_inputs_unchanged'] and source['executable_retained']
    assert stability['inputs_unchanged_through_run']
    before = {str((ROOT/x['path']).resolve()): x['sha256'] for x in source['files']}
    after = {str((ROOT/x['path']).resolve()): x['sha256'] for x in stability['after_run']}
    assert all(before[p] == h for p, h in after.items()) and len(before) == len(after)+1
    for row in source['files']:
        assert digest(row['path']) == row['sha256']
    compiled = load(prefix+'-compile-inputs.json')
    assert compiled['inputs'] == stability['after_run']
    lifecycle = load(prefix+'-lifecycle.json')
    assert lifecycle['exit_code'] == 0 and lifecycle['failure'] is None
    assert not lifecycle['timed_out'] and not lifecycle['terminated_by_wrapper']
    assert lifecycle['executable_retained'] and lifecycle['executable_unchanged_through_run']
    assert digest(lifecycle['executable']) == lifecycle['executable_sha256']
    assert lifecycle['arguments'] == ['--name', Path(prefix).name, '--cartesian-promoted']
    outputs = load(prefix+'-outputs.json')
    for row in outputs:
        assert digest(row['path']) == row['sha256']
    present = {str((ROOT/x['path']).resolve()) for x in outputs}
    for suffix in ('.json', '.wgsl', '-inputs.bin', '-uniform.bin', '-detail.bin',
                   '-cost-reference.bin', '-cost-promoted.bin', '-sources.json',
                   '-compile-inputs.json', '-compiler.log', '-executable.exe',
                   '-lifecycle.json', '-input-stability.json', '.stdout.log', '.stderr.log'):
        assert str((ROOT/(prefix+suffix)).resolve()) in present
    return source


def checksum(core_word_pairs, first):
    h = 0
    for k in range(8):
        index = (first+17*k) % 11
        for word in core_word_pairs[index]:
            h = (((h ^ word)*16777619)+2166136261) & 0xffffffff
    return [h]*4


def timing(entries):
    """Only call after every arithmetic/input/context prerequisite passes."""
    samples, previous = {}, -1
    for name in ('cost_reference', 'cost_promoted'):
        rows = entries[name]['iterations']; assert len(rows) == 8
        values = []
        for index, row in enumerate(rows):
            assert row['iteration'] == index and row['warmup'] == (index < 3)
            start, end = row['raw_timestamps']
            assert isinstance(start, int) and isinstance(end, int) and end > start > previous
            previous = end
            period = row['timestamp_period_ns']
            assert math.isfinite(period) and period > 0
            seconds = (end-start)*period*1e-9
            assert abs(row['gpu_seconds']-seconds) <= 4*math.ulp(seconds)
            if index >= 3:
                values.append(Q(end-start)*Q(period)/1_000_000)
        samples[name] = values
    a, b = samples['cost_reference'], samples['cost_promoted']
    median_a, median_b = sorted(a)[2], sorted(b)[2]
    median_gate = median_b <= 2*median_a
    separation_gate = max(b) < 2*min(a)
    return {'retained_milliseconds_exact': {k: list(map(str, v)) for k, v in samples.items()},
            'retained_milliseconds': {k: list(map(float, v)) for k, v in samples.items()},
            'reference_median_ms': float(median_a), 'promoted_median_ms': float(median_b),
            'median_ratio_exact': str(median_b/median_a),
            'median_at_most_twice_reference': median_gate,
            'promoted_max_less_than_twice_reference_min': separation_gate,
            'passed_effort_gate': median_gate and separation_gate,
            'scope': 'One trial+dense+full-bit-consumption workload effort rule. Common observation overhead can compress the ratio. Not intrinsic FP64 throughput, a whole-ray bound, or an FPS forecast.'}


def audit(prefix, validation_prefix):
    report = load(prefix+'.json')
    sources = source_rows(prefix)
    assert report['cartesian_promoted'] and not report['fast_bounds']
    assert report['count'] == 11 and report['detail_record_words'] == 756
    assert report['cost_invocations'] == 11*256
    assert report['strictly_increasing_unique_timestamp_pairs']
    assert all(x in report['required_features'] for x in ('SHADER_F64', 'SHADER_INT64', 'TIMESTAMP_QUERY'))
    offline = load(validation_prefix+'-validation.json')
    assert offline['cartesian_promoted'] and offline['count'] == 11
    assert offline['fixture'] == report['fixture']
    for suffix in ('.wgsl', '-uniform.bin', '-inputs.bin'):
        assert (ROOT/(prefix+suffix)).read_bytes() == (ROOT/(validation_prefix+suffix)).read_bytes()
    fixture = report['fixture']
    assert fixture['inner'] == 8 and fixture['repetitions'] == 256
    assert fixture['core_scalars'] == 80 and fixture['detail_scalars_per_precision'] == 188
    assert words(prefix+'-uniform.bin') == [11, 8, 2816, 0x43545031]
    inputs = words(prefix+'-inputs.bin'); assert len(inputs) == 11*36
    detail = words(prefix+'-detail.bin'); assert len(detail) == 11*756
    entries = {x['entry']: x for x in report['entries']}
    assert list(entries) == ['trial_detail', 'cost_reference', 'cost_promoted']
    assert entries['trial_detail']['iterations'] == []
    assert entries['trial_detail']['dispatch'] == [1, 1, 1]
    assert entries['trial_detail']['output_bytes'] == 11*756*4
    for name in ('cost_reference', 'cost_promoted'):
        assert entries[name]['dispatch'] == [88, 1, 1] and entries[name]['output_bytes'] == 2816*16
    planned = [(ATMOSPHERE, 1)]+[(SELECTED, i) for i in IDS]
    raw_sources = {}
    for p in (ATMOSPHERE, SELECTED):
        manifest = load(p+'-outputs.json')
        for suffix in ('.json', '-primary.bin', '-states.bin', '-auxiliary.bin'):
            found = [x for x in manifest if (ROOT/x['path']).resolve() == (ROOT/(p+suffix)).resolve()]
            assert len(found) == 1 and digest(p+suffix) == found[0]['sha256']
        raw_sources[p] = (words(p+'-primary.bin'), words(p+'-states.bin'), words(p+'-auxiliary.bin'))
    rows, core_pairs = [], [[], []]
    for index, (p, identity) in enumerate(planned):
        primary, states, log = raw_sources[p]
        saved = states[24*identity:24*(identity+1)]
        log_base = 16388*identity; first = log[log_base+4:log_base+8]
        owner = struct.unpack('<i', struct.pack('<I', saved[20]))[0]
        count = f32(primary[36]); assert count.is_integer() and 2 <= count <= 128
        assert -1 <= owner < int(count)-1
        assert saved[11] == saved[14] == saved[15] == saved[23] == 0
        assert saved[21:23] == [identity, 1]
        assert 0 < log[log_base] <= 4096 and log[log_base+1:log_base+4] == [0, identity, primary[46]]
        assert first[1] == saved[3] and first[2] > 0 and first[3] in (0, 1)
        length = f32(first[0]); assert 1e-6 <= length <= 100000
        branch = [primary[41], primary[49], 0] if owner < 0 else primary[48+4*owner:51+4*owner]
        expected = primary[:16]+saved[:4]+saved[4:7]+[first[0]]+branch+[saved[20]]+saved[16:20]+[index, identity, first[2], first[3]]
        packed = inputs[36*index:36*(index+1)]; assert packed == expected
        mapping = fixture['mapping'][index]
        assert mapping['input_words'] == packed and mapping['saved_state_words'] == saved and mapping['primary_words'] == primary
        assert mapping['source_prefix'] == p and mapping['record_index'] == identity and mapping['input_index'] == index
        assert mapping['source_owner'] == owner and mapping['first_log_words'] == first and mapping['log_count'] == log[log_base]
        record = detail[756*index:756*(index+1)]
        assert record[:4] == [index, saved[20], first[2], first[3]]
        precision = []
        for mode in range(2):
            raw_values = record[4+376*mode:380+376*mode]
            values = list(struct.unpack('<188d', struct.pack('<376I', *raw_values)))
            if mode == 0:
                assert all(math.isfinite(v) and struct.unpack('<f', struct.pack('<f', v))[0] == v for v in values)
            comparison = reference.compare(packed, values)
            precision.append(comparison)
            core_pairs[mode].append(raw_values[:160])
        lower = f32(primary[42] if owner < 0 else primary[48+4*owner])
        upper = f32(primary[48+4*(owner+1)])
        with mp.workdps(reference.DIGITS):
            assert mp.mpf(lower) < mp.mpf(precision[1]['target'][89]) < mp.mpf(upper)
        rows.append({'input_index': index, 'source_prefix': p, 'record_index': identity,
                     'new_standalone_length_m': length, 'original_log_attempt': first[2],
                     'reference_f32': precision[0], 'promoted_f64': precision[1]})
    valid = all(r[k]['complete_finite_valid_trial'] for r in rows for k in ('reference_f32', 'promoted_f64'))
    arithmetic = valid and all(r['promoted_f64']['passed_arithmetic_gate'] for r in rows)
    contexts = {}
    for mode, name in enumerate(('reference', 'promoted')):
        raw = words(prefix+'-cost-'+name+'.bin'); assert len(raw) == 2816*4
        by_case = [checksum(core_pairs[mode], j) for j in range(11)]
        mismatches = [i for i in range(2816) if raw[4*i:4*(i+1)] != by_case[i % 11]]
        contexts[name] = {'all_consumed_fields': 80, 'halves_per_field': 2,
                          'mismatch_count': len(mismatches), 'first_mismatch_ids': mismatches[:16]}
    context_pass = not any(x['mismatch_count'] for x in contexts.values())
    cost = timing(entries) if arithmetic and context_pass else None
    passed = bool(cost and cost['passed_effort_gate'])
    return {'schema': 'cartesian-promoted-trial-review-v1', 'prefix': prefix,
            'validation_prefix': validation_prefix, 'auditor_sha256': digest(__file__),
            'reference_sha256': digest('scripts/optical_cartesian_trial_reference.py'),
            'policy_sha256': digest('docs/science/cartesian-promoted-trial-policy.md'),
            'python': sys.version, 'mpmath_version': mp.__version__, 'precision_digits': reference.DIGITS,
            'report_sha256': digest(prefix+'.json'), 'source_manifest_sha256': digest(prefix+'-sources.json'),
            'output_manifest_sha256': digest(prefix+'-outputs.json'),
            'source_and_input_checks_passed': True, 'all_seven_stage_trials_valid': valid,
            'candidate_arithmetic_gate_passed': arithmetic, 'checksum_source_consistency_passed': context_pass,
            'checksums': contexts, 'cost': cost, 'passed': passed,
            'decision': ('This fixed arithmetic and instrumented-cost screen permits considering a separately reviewed controller port.' if passed else
                         'Reject this full-f64 trial scope at the fixed gate; no sweep or controller implementation.'),
            'records': rows,
            'scope': 'No ODE rerun, source/contact/controller proof, intrinsic hardware throughput, whole-ray speed bound or native FPS conclusion. Checksums are not collision-free. Numerical and engineering gates were fixed before GPU output.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--validation-prefix', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    result = audit(args.prefix, args.validation_prefix)
    with (ROOT/args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({k: result[k] for k in ('all_seven_stage_trials_valid',
        'candidate_arithmetic_gate_passed', 'checksum_source_consistency_passed', 'passed', 'decision')}, indent=2))
