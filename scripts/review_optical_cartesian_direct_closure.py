"""Independent bounded closure of the retained direct-output experiment.

No GPU or ODE execution. Reconstruct every header and full payload association,
evaluate the previously reviewed algebraic reference, and derive timing directly
from exact raw ticks without invoking the prospective auditor's audit/timing.
"""
import hashlib
import json
import math
import re
from collections import Counter
from fractions import Fraction as Q
from pathlib import Path
import struct

import mpmath as mp
import numpy as np

import optical_cartesian_trial_reference as reference

ROOT = Path(__file__).resolve().parents[1]
PREFIX = 'artifacts/optical-cartesian-direct-gpu-first'
OFFLINE = 'artifacts/optical-cartesian-direct-validation-second'
SELECTED = 'artifacts/optical-cartesian-suffix-tight-selected-first'
ATMOSPHERE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first'
IDS = [3, 9, 10, 11, 12, 14, 15, 17, 18, 20]


def path(name):
    return (ROOT/name).resolve()


def load(name):
    return json.loads(path(name).read_text(encoding='utf-8-sig'))


def sha(name):
    with path(name).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def words(name):
    raw = path(name).read_bytes()
    return list(struct.unpack('<'+'I'*(len(raw)//4), raw))


def f32(word):
    return struct.unpack('<f', struct.pack('<I', word))[0]


def run():
    report = load(PREFIX+'.json')
    review = load(PREFIX+'-independent-review.json')
    sources = load(PREFIX+'-sources.json')
    stability = load(PREFIX+'-input-stability.json')
    lifecycle = load(PREFIX+'-lifecycle.json')
    outputs = load(PREFIX+'-outputs.json')
    before = {str(path(x['path'])): x['sha256'] for x in sources['files']}
    after = {str(path(x['path'])): x['sha256'] for x in stability['after_run']}
    assert len(before) == len(sources['files']) == len(after)+1
    assert len(after) == len(stability['after_run'])
    assert all(before[p] == h for p, h in after.items())
    assert load(PREFIX+'-compile-inputs.json')['inputs'] == stability['after_run']
    assert sources['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    assert sources['cartesian_direct'] and not sources['cartesian_promoted']
    assert sources['executable_retained'] and not sources['validation_only']
    for item in sources['files']:
        assert sha(item['path']) == item['sha256']
    output_hashes = {str(path(x['path'])): x['sha256'] for x in outputs}
    assert len(output_hashes) == len(outputs)
    assert lifecycle['exit_code'] == 0 and lifecycle['failure'] is None
    assert not lifecycle['timed_out'] and not lifecycle['terminated_by_wrapper']
    assert lifecycle['executable_retained'] and lifecycle['executable_unchanged_through_run']
    assert sha(lifecycle['executable']) == lifecycle['executable_sha256']
    assert lifecycle['arguments'] == ['--name', Path(PREFIX).name, '--cartesian-direct']
    captures = {str(path(row['capture']['path']))
                for entry in report['entries'] for row in entry['iterations']}
    for item in outputs:
        if str(path(item['path'])) not in captures:
            assert sha(item['path']) == item['sha256']
    for name, key in [('scripts/review_optical_cartesian_direct.py', 'auditor_sha256'),
                      ('scripts/optical_cartesian_trial_reference.py', 'reference_sha256'),
                      ('docs/science/cartesian-direct-trial-policy.md', 'policy_sha256'),
                      (PREFIX+'-sources.json', 'source_manifest_sha256'),
                      (PREFIX+'-outputs.json', 'output_manifest_sha256'),
                      (PREFIX+'.json', 'report_sha256')]:
        assert sha(name) == review[key]
    for suffix in ('.wgsl', '-inputs.bin', '-uniform.bin'):
        assert path(PREFIX+suffix).read_bytes() == path(OFFLINE+suffix).read_bytes()
    assert load(OFFLINE+'-validation.json')['fixture'] == report['fixture']
    assert words(PREFIX+'-uniform.bin') == [11, 8, 2816, 0x43544431]
    incoming = words(PREFIX+'-inputs.bin')
    assert incoming == words('artifacts/optical-cartesian-trial-validation-second-inputs.bin')
    inputs = [incoming[i*36:(i+1)*36] for i in range(11)]
    provenance_inputs = []
    for index, (prefix, record) in enumerate([(ATMOSPHERE, 1)]+[(SELECTED, i) for i in IDS]):
        primary, states, logs = [words(prefix+s) for s in ('-primary.bin', '-states.bin', '-auxiliary.bin')]
        old_hashes = {str(path(x['path'])): x['sha256'] for x in load(prefix+'-outputs.json')}
        for suffix in ('-primary.bin', '-states.bin', '-auxiliary.bin'):
            assert sha(prefix+suffix) == old_hashes[str(path(prefix+suffix))]
        state = states[record*24:(record+1)*24]
        header = logs[record*16388:record*16388+4]
        first = logs[record*16388+4:record*16388+8]
        owner = struct.unpack('<i', struct.pack('<I', state[20]))[0]
        branch = [primary[41], primary[49], 0] if owner == -1 else primary[48+4*owner:51+4*owner]
        expected = primary[:16]+state[:7]+[first[0]]+branch+[state[20]]+state[16:20]+[index, record, first[2], first[3]]
        assert inputs[index] == expected
        assert 0 < header[0] <= 4096 and header[1:] == [0, record, primary[46]]
        assert first[1] == state[3] and first[2] > 0 and first[3] in (0, 1)
        assert state[11] == state[14] == state[15] == state[23] == 0
        assert state[21:23] == [record, 1]
        assert -1 <= owner < int(f32(primary[36]))-1
        with mp.workdps(100):
            h = reference.reference(inputs[index])[89]
            lower = f32(primary[42] if owner < 0 else primary[48+4*owner])
            upper = f32(primary[48+4*(owner+1)])
            assert mp.mpf(lower) < h < mp.mpf(upper)
        provenance_inputs.append({'input': index, 'source': prefix, 'record': record, 'owner': owner})
    assert report['cartesian_direct'] and not report['cartesian_promoted'] and not report['fast_bounds']
    assert report['count'] == 11 and report['cost_invocations'] == 2816
    assert report['strictly_increasing_unique_timestamp_pairs']
    assert {'SHADER_F64', 'SHADER_INT64', 'TIMESTAMP_QUERY'} <= set(re.findall(r'\b[A-Z][A-Z_0-9]+\b', report['required_features']))
    assert [e['entry'] for e in report['entries']] == ['direct_reference', 'direct_promoted']
    ordinal = np.arange(22528, dtype=np.uint32)
    invocation, cycle = ordinal//8, ordinal % 8
    identity = (invocation+17*cycle) % 11
    assert Counter(identity.tolist()) == dict.fromkeys(range(11), 2048)
    representatives, capture_evidence, numerical = {}, [], []
    for precision, entry in zip((32, 64), report['entries']):
        assert entry['dispatch'] == [88, 1, 1] and entry['output_bytes'] == 34603008
        assert len(entry['iterations']) == 8
        for iteration, row in enumerate(entry['iterations']):
            assert row['iteration'] == iteration and row['warmup'] == (iteration < 3)
            capture = row['capture']; raw = path(capture['path']).read_bytes()
            assert len(raw) == capture['bytes'] == 34603008
            assert capture['complete_output_retained'] and capture['output_cleared_before_dispatch']
            assert capture['output_captured_after_dispatch']
            digest = hashlib.sha256(raw).hexdigest()
            assert digest == output_hashes[str(path(capture['path']))]
            data = np.frombuffer(raw, dtype='<u4').reshape(22528, 384)
            expected = np.column_stack((np.full(22528, 0x43544431, dtype=np.uint32),
                invocation, cycle, identity, np.array([x[33] for x in inputs], dtype=np.uint32)[identity],
                np.array([x[27] for x in inputs], dtype=np.uint32)[identity],
                np.full(22528, precision, dtype=np.uint32), np.full(22528, 127, dtype=np.uint32)))
            assert np.array_equal(data[:, :8], expected)
            if iteration == 0:
                representatives[precision] = data[np.arange(11)*8, 8:].copy()
            # Compare every stored word, including signed-zero bits and padding;
            # this equality is not inferred from file or payload hashes.
            assert np.array_equal(data[:, 8:], representatives[precision][identity])
            values = data[:, 8:].copy().view('<f8')
            assert np.isfinite(values).all()
            if precision == 32:
                assert np.array_equal(values, values.astype('<f4').astype('<f8'))
            capture_evidence.append({'path': capture['path'], 'sha256': digest,
                                     'precision': precision, 'iteration': iteration,
                                     'records': 22528, 'all_header_and_payload_words_checked': True})
        for index in range(11):
            raw_payload = representatives[precision][index].tobytes()
            values = list(struct.unpack('<188d', raw_payload))
            with mp.workdps(100):
                target = reference.reference(inputs[index]); length = target[77]
                assert values[77] == float(length) and values[78:80] == [0., 0.]
                assert values[6] < 1e29 and values[185] > 0
                for stage in range(7):
                    normal = values[86+14*stage:89+14*stage]
                    normal_norm = mp.sqrt(sum(mp.mpf(x)**2 for x in normal))
                    assert mp.mpf('0.5') < normal_norm < 2
                    assert mp.mpf('0.5') <= 1+mp.mpf(values[90+14*stage]) <= mp.mpf('3e38')
                comparisons = []
                for field, start, width, scale in reference.groups(length):
                    difference = mp.sqrt(sum((mp.mpf(values[j])-target[j])**2 for j in range(start,start+width)))*scale
                    comparisons.append({'field': field, 'difference_metres': mp.nstr(difference, 40),
                                        'within_bound': bool(difference <= mp.mpf(1)/128000)})
                assert len(comparisons) == 75
                if precision == 64: assert all(x['within_bound'] for x in comparisons)
            recorded = [x for x in review['payload_comparisons']
                        if x['precision'] == precision and x['input_index'] == index]
            assert len(recorded) == 1
            recorded = recorded[0]
            assert recorded['observations'] == 16384
            assert recorded['payload_sha256'] == hashlib.sha256(raw_payload).hexdigest()
            assert recorded['comparison']['complete_finite_valid_trial']
            assert recorded['comparison']['comparisons'] == comparisons
            assert recorded['comparison']['passed_arithmetic_gate'] == all(x['within_bound'] for x in comparisons)
            numerical.append({'precision': precision, 'input': index, 'observations': 16384,
                'payload_sha256': hashlib.sha256(raw_payload).hexdigest(), 'groups': 75,
                'groups_outside_B': sum(not x['within_bound'] for x in comparisons)})
    assert len(capture_evidence) == 16 and len(captures) == 16
    assert review['all_records_verified'] and review['verified_records'] == 360448
    assert review['complete_finite_trials'] and review['promoted_arithmetic_gate_passed']
    assert review['numerical_gate_passed'] and review['first_numerical_failure'] is None
    assert review['inconclusive'] is None and review['distinct_payloads'] == 22
    # Timing is deliberately reached only after the complete raw/numeric gate.
    samples, tick_rows, previous = {}, [], -1
    for precision, entry in zip((32, 64), report['entries']):
        durations = []
        for iteration, row in enumerate(entry['iterations']):
            begin, end = row['raw_timestamps']
            assert type(begin) is int and type(end) is int and end > begin > previous
            previous = end
            period = Q(row['timestamp_period_ns']); assert period > 0
            milliseconds = (end-begin)*period/1000000
            seconds = float(milliseconds/1000)
            assert abs(row['gpu_seconds']-seconds) <= 4*math.ulp(seconds)
            tick_rows.append({'precision': precision, 'iteration': iteration,
                'warmup': row['warmup'], 'ticks': [begin, end],
                'period_ns_exact': str(period), 'milliseconds_exact': str(milliseconds)})
            if iteration >= 3: durations.append(milliseconds)
        samples[precision] = durations
    a, b = samples[32], samples[64]
    median_a, median_b = sorted(a)[2], sorted(b)[2]
    median_pass = median_b <= 2*median_a
    range_pass = max(b) < 2*min(a)
    cost = review['cost']
    assert cost['retained_milliseconds_exact'] == {
        'cost_reference': list(map(str, a)), 'cost_promoted': list(map(str, b))}
    assert cost['reference_median_ms'] == float(median_a)
    assert cost['promoted_median_ms'] == float(median_b)
    assert cost['median_ratio_exact'] == str(median_b/median_a)
    assert cost['median_at_most_twice_reference'] == median_pass
    assert cost['promoted_max_less_than_twice_reference_min'] == range_pass
    assert cost['passed_effort_gate'] == (median_pass and range_pass) == review['passed']
    assert median_pass and not range_pass and not review['passed']
    return {'schema': 'cartesian-direct-independent-closure-v1',
        'review_completed': True, 'experiment_passed': False,
        'decision': 'REJECT: fixed strict range gate fails; no further run, sweep or controller approval.',
        'provenance': {'source_files_checked': len(before), 'outputs_checked': len(outputs),
            'compile_before_after_identity': True, 'lifecycle_and_retained_executable_pass': True,
            'offline_shader_uniform_inputs_exact': True,
            'review_sha256': sha(PREFIX+'-independent-review.json'),
            'source_manifest_sha256': sha(PREFIX+'-sources.json'),
            'output_manifest_sha256': sha(PREFIX+'-outputs.json'),
            'raw_report_sha256': sha(PREFIX+'.json'), 'closure_source_sha256': sha(__file__),
            'policy_sha256': sha('docs/science/cartesian-direct-trial-policy.md')},
        'input_reconstruction': provenance_inputs, 'captures': capture_evidence,
        'raw_bytes_checked': 16*34603008, 'records_checked': 360448,
        'scalar_observations_checked_finite': 360448*188,
        'distinct_payloads_compared_by_full_bytes': 22,
        'numerical_reference': 'Same frozen 100-digit algebraic target; comparison norms recomputed independently without calling prospective comparison/audit/timing functions.',
        'numerical': numerical, 'maximum_reported_group_errors': review['maximum_reported_group_errors'],
        'all_numeric_comparisons_exactly_match_frozen_auditor': True,
        'promoted_groups_within_B': 825, 'B_metres_exact': '1/128000',
        'timing': {'all16_raw_pairs': tick_rows, 'reference_median_ms_exact': str(median_a),
            'promoted_median_ms_exact': str(median_b), 'median_ratio_exact': str(median_b/median_a),
            'median_gate': median_pass, 'range_gate': range_pass,
            'promoted_max_ms_exact': str(max(b)), 'twice_reference_min_ms_exact': str(2*min(a)),
            'range_excess_ms_exact': str(max(b)-2*min(a)), 'effort_gate': False},
        'gpu_executions': 0, 'ode_executions': 0,
        'limits': ['Timed full-store workload only; clear, readback and host validation excluded.',
            'No intrinsic precision throughput, unobserved core/controller cost or FPS inference.',
            'Finite standalone trials only, not full-ray/source/contact or global ODE validity.',
            'Prior checksum workload remains rejected and its timing remains uninterpreted.']}


if __name__ == '__main__':
    result = run()
    destination = PREFIX+'-closure-review.json'
    with path(destination).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({k: result[k] for k in ('review_completed', 'experiment_passed',
        'decision', 'raw_bytes_checked', 'records_checked')}, indent=2))
