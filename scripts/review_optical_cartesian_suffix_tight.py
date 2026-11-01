"""One-factor selected suffix gate, fixed before this candidate's readback.

Retains the same original-ray references and prospective metric rules. Source
and exact handoff identity, unchanged routes, original terminals and complete
accepted-step logs are independent prerequisites. No factor search or timing.
"""
import argparse
from collections import Counter
from fractions import Fraction as Q
import json
import math
from pathlib import Path

import numpy as np

from review_optical_cartesian_suffix_policy import evaluate_selected_record, METRICS
from review_optical_cartesian_suffix_source import digest, read, replace_once
from review_optical_cartesian_suffix_trace import number, positive_addition_enclosure
from verify_optical_dop853 import geometry

ROOT = Path(__file__).resolve().parents[1]
CONTROL = 'artifacts/optical-cartesian-suffix-trace-selected-first'
FIRST = 'artifacts/optical-cartesian-suffix-selected-first'
POLICY = 'artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json'
EXPECTED_SUFFIX = [3, 9, 10, 11, 12, 14, 15, 17, 18, 20]


def load(path):
    return json.loads(read(ROOT / path))


def module_identity(prefix):
    rows = []
    for name in ('prefix', 'suffix', 'fallback'):
        new = read(ROOT / (prefix+'-'+name+'.wgsl'))
        old = read(ROOT / (CONTROL+'-'+name+'.wgsl'))
        restored = new
        if name == 'suffix':
            restored = replace_once(new, 'let tolerance=0.25*max(0.002,4e-7*length(state.p));',
                                    'let tolerance=max(0.002,4e-7*length(state.p));')
        assert restored == old, name
        rows.append({'module': name, 'new_sha256': digest(ROOT / (prefix+'-'+name+'.wgsl')),
                     'control_sha256': digest(ROOT / (CONTROL+'-'+name+'.wgsl')),
                     'whole_source_equal_after_only_prescribed_replacement': True})
    return rows


def audit_logs(active, handoff_word, final_word, attempt_count):
    # The prospectively pinned selected experiment requires an explicitly
    # logged final source-exit step, as all ten control rays had.
    assert len(active) > 0 and int(active[-1, 3]) == 1
    cumulative, rounding = Q(number(handoff_word)), Q(0)
    previous_attempt, rows = 0, []
    for j, raw in enumerate(active):
        step_word, before_word, attempt, kind = map(int, raw)
        step, before = number(step_word), number(before_word)
        assert math.isfinite(step) and math.isfinite(before) and step >= 0 and before >= 0
        assert previous_attempt < attempt <= attempt_count
        assert kind == 0 or (kind == 1 and j == len(active)-1)
        previous_attempt = attempt
        if j == 0:
            assert before_word == handoff_word
        after_word = int(active[j+1, 1]) if j+1 < len(active) else final_word
        after = Q(number(after_word))
        lower, upper = positive_addition_enclosure(before, step)
        assert lower <= after <= upper
        error = after-Q(before)-Q(step)
        cumulative += Q(step)
        rounding += error
        assert after-cumulative == rounding
        rows.append({'words': raw.tolist(), 'next_path_word': after_word,
                     'addition_error_exact': str(error), 'exact_cumulative_path': str(cumulative)})
    if not len(active):
        assert handoff_word == final_word
    assert Q(number(final_word))-cumulative == rounding
    return {'records': rows, 'exact_sum': str(cumulative), 'exact_sum_m': float(cumulative),
            'stored_minus_exact_sum_m': float(rounding), 'zero_steps': sum(number(r[0]) == 0 for r in active)}


def review(prefix):
    report, control = load(prefix+'.json'), load(CONTROL+'.json')
    assert report['cartesian_suffix_tight'] and report['cartesian_suffix_trace'] and report['selected']
    assert not report['timed'] and not report['iterations'] and report['record_count'] == 23
    assert report['state_words_per_record'] == 24 and report['diagnostic_bytes_per_record'] == 32
    assert report['steps'] == control['steps']
    assert report['auxiliary_layout'] == control['auxiliary_layout'] == {
        'bytes': 1507696, 'log_words_per_record': 16388, 'max_steps': 4096}
    source, compiled, stability = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert source['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    assert source['files'][:-1] == compiled['inputs'] == stability['after_run']
    assert source['files'][-1]['path'].endswith('.exe')
    outputs = load(prefix+'-outputs.json')
    for item in source['files'] + outputs + load(CONTROL+'-outputs.json') + load(FIRST+'-outputs.json'):
        assert digest(ROOT / item['path']) == item['sha256'], item['path']
    lifecycle = load(prefix+'-lifecycle.json')
    assert lifecycle['exit_code'] == 0 and not lifecycle['timed_out'] and not lifecycle['terminated_by_wrapper']
    old_source = {x['path'].replace('\\', '/'): x['sha256'] for x in load(CONTROL+'-sources.json')['files']}
    snapshots = {'docs/science/cartesian-suffix-design.md': '-design.md',
                 'scripts/optical_wavefront_probe.rs': '-host.rs',
                 'scripts/verify_optical_wavefront.ps1': '-wrapper.ps1'}
    for original, snapshot in snapshots.items():
        assert digest(ROOT / (CONTROL+snapshot)) == old_source[original]
    modules = module_identity(prefix)
    inputs = []
    for tag in ('primary', 'field', 'frames', 'geometry', 'certificates'):
        path = ROOT / (prefix+'-'+tag+'.bin')
        assert path.read_bytes() == (ROOT / (CONTROL+'-'+tag+'.bin')).read_bytes()
        inputs.append({'name': tag, 'bytes': path.stat().st_size, 'sha256': digest(path)})
    policy = load(POLICY)
    baseline, contract = load(policy['baseline_report']), load(policy['initial_contract'])
    assert digest(ROOT / policy['baseline_report']) == policy['baseline_sha256']
    assert digest(ROOT / policy['initial_contract']) == policy['initial_contract_sha256']
    assert digest(ROOT / policy['policy_source']) == policy['policy_source_sha256']
    references = {r['record_index']: r for r in baseline['cases']}
    original_packet = contract['baseline']
    raw = np.fromfile(ROOT / (prefix+'-readback.bin'), dtype='<u4')
    original_raw = np.fromfile(ROOT / (original_packet+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT / (prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    old_state = np.fromfile(ROOT / (CONTROL+'-states.bin'), dtype='<u4').reshape(23, 24)
    old_raw = np.fromfile(ROOT / (CONTROL+'-readback.bin'), dtype='<u4')
    progress = np.fromfile(ROOT / (prefix+'-progress.bin'), dtype='<u4')
    auxiliary = np.fromfile(ROOT / (prefix+'-auxiliary.bin'), dtype='<u4').reshape(23, 4097, 4)
    assert raw.nbytes == report['output_bytes'] == 1856 and state.nbytes == report['state_bytes'] == 2208
    assert auxiliary.nbytes == 1507696 and progress.nbytes == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    old_terminal, old_detail = old_raw[4:280].reshape(23, 12), old_raw[280:].reshape(23, 8)
    original_terminal, original_detail = original_raw[4:280].reshape(23, 12), original_raw[280:].reshape(23, 8)
    assert np.array_equal(detail[:, 4:7], original_detail[:, 4:7])
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.array_equal(state[:, 21], np.arange(23))
    assert np.all(state[:, 12:15] <= 4096) and np.all(state[:, 12].astype(np.uint64)+state[:, 13] <= 4096)
    work = state[:, 12:15].astype(np.uint64).sum(axis=1)
    assert progress.tolist() == report['progress_words'] == [0, 23, int(work.max()), 0]
    identity_columns = list(range(13)) + list(range(16, 24))
    identities = np.all(state[:, identity_columns] == old_state[:, identity_columns], axis=1)
    route_identity = state[:, 22] == old_state[:, 22]
    origin = np.asarray(baseline['cases'][0]['inputs']['origin_ecef_m'], dtype=float)
    _, up = geometry(origin)
    east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
    basis = np.column_stack((east, up, -np.cross(up, east)))
    names = {0: 'unhanded-original', 1: 'accepted-suffix', 2: 'original-fallback', 3: 'masked'}
    rows, log_count = [], 0
    for i in range(23):
        sample = report['samples'][i]
        assert sample['record_index'] == i and sample['pixel'] == policy['records'][i]['pixel']
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist() and sample['state_words'] == state[i].tolist()
        route = int(state[i, 22]); assert route in names
        count, overflow, record, token = map(int, auxiliary[i, 0])
        assert count <= 4096 and overflow == 0 and record == i and token == report['header'][2]
        f = terminal[i].copy().view('<f4').astype(float)
        assert np.isfinite(f[:7]).all() and np.isfinite(f[8:11]).all()
        errors, logs = None, None
        reasons = []
        if not identities[i]: reasons.append('handoff-or-prefix-identity-differs')
        if not route_identity[i]: reasons.append('route-differs')
        if i not in EXPECTED_SUFFIX:
            if not np.array_equal(terminal[i], original_terminal[i]): reasons.append('original-terminal-differs')
            if not np.array_equal(state[i], old_state[i]) or not np.array_equal(detail[i], old_detail[i]): reasons.append('original-route-state-or-detail-differs')
            if count != 0: reasons.append('unexpected-original-route-log')
        if route == 1:
            assert terminal[i, 7] == 0 and state[i, 13] > 0 and state[i, 14] == state[i, 15] == 0
            ref = references[i]
            assert ref['launch_words'] == detail[i, 4:7].tolist()
            tight = ref['tight_reference']
            errors = {'position_m': float(np.linalg.norm(origin+basis@f[:3]-tight['endpoint_ecef_m'])),
                      'path_m': abs(float(f[3])-tight['geometric_path_m']),
                      'direction': float(np.linalg.norm(basis@f[4:7]-tight['exit_direction']))}
            logs = audit_logs(auxiliary[i, 1:1+count], int(state[i, 3]), int(terminal[i, 3]), int(state[i, 13]))
            logs['exact_sum_reference_error_m'] = float(abs(Q(logs['exact_sum'])-Q(tight['geometric_path_m'])))
            log_count += count
        evaluation = evaluate_selected_record(policy, i, names[route], terminal[i].tolist(), detail[i, 4:7].tolist(), errors)
        reasons += evaluation['reasons']
        rows.append({'record_index': i, 'pixel': sample['pixel'], 'route': names[route],
                     'same_handoff_and_prefix': bool(identities[i]), 'same_route': bool(route_identity[i]),
                     'attempts': state[i, 12:15].tolist(), 'terminal_words': terminal[i].tolist(),
                     'candidate_error': errors, 'original_error': references[i]['original_error'] if i in references else None,
                     'reference_convergence': references[i]['reference_convergence'] if i in references else None,
                     'accepted_step_count': count, 'accepted_step_audit': logs,
                     'passed': not reasons, 'failure_reasons': reasons})
    suffixes = np.flatnonzero(state[:, 22] == 1).tolist()
    failed = [r['record_index'] for r in rows if not r['passed']]
    passed = not failed and suffixes == EXPECTED_SUFFIX
    return {'scope': __doc__.strip(), 'prefix': prefix, 'factor': 0.25,
            'input_hash_count': len(source['files']), 'output_hash_count': len(outputs),
            'module_identity': modules, 'five_inputs_exact': inputs,
            'policy_sha256': digest(ROOT / POLICY), 'baseline_sha256': digest(ROOT / policy['baseline_report']),
            'all_handoffs_and_prefixes_exact': bool(np.all(identities)),
            'same_all_routes': bool(np.all(route_identity)), 'suffix_record_indices': suffixes,
            'route_counts': dict(Counter(names[int(x)] for x in state[:, 22])),
            'progress': progress.tolist(), 'accepted_step_records_verified': log_count,
            'failed_records': failed, 'all_fixed_selected_gates_passed': passed,
            'decision': 'selected-tight-control-gate-pass-only' if passed else 'reject-and-close-one-factor-refinement-branch',
            'full_frame_timing_or_production_authorized': False,
            'limitations': ['Finite tighter-control response, including changed f32 schedules and roundoff; no isolated truncation-order or global error proof.',
                           'Even a passing selected result requires the frozen original atmosphere and synthetic branch matrices before timing.',
                           'A failed metric or route closes this one-factor branch; no factor sweep or adjusted acceptance threshold.'],
            'records': rows, 'reviewer_source_sha256': digest(__file__)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    output = ROOT / args.output
    assert not output.exists(), 'Preserve prior evidence'
    result = review(args.prefix)
    output.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps({k: result[k] for k in ['decision', 'all_handoffs_and_prefixes_exact', 'same_all_routes', 'accepted_step_records_verified', 'failed_records']}))
