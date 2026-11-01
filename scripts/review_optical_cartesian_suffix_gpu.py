"""Audit the selected suffix against its unchanged pre-candidate policy.

Uses the frozen SAME original-ray reference, without fitting new thresholds
or reintegrating a candidate-specific physical model. A passing measured
local matrix would still require the separate synthetic/atmosphere gates.
"""
import argparse
from collections import Counter
from fractions import Fraction as Q
import json
from pathlib import Path

import numpy as np

from review_optical_cartesian_suffix_policy import evaluate_selected_record, METRICS
from review_optical_cartesian_suffix_source import digest, read, review as review_source
from verify_optical_dop853 import geometry

ROOT = Path(__file__).resolve().parents[1]
POLICY = 'artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json'
VALIDATED = 'artifacts/optical-cartesian-suffix-validation-third'


def load(path):
    return json.loads(read(ROOT / path))


def review(prefix):
    report = load(prefix + '.json')
    policy = load(POLICY)
    baseline = load(policy['baseline_report'])
    contract = load(policy['initial_contract'])
    assert digest(ROOT / policy['baseline_report']) == policy['baseline_sha256']
    assert digest(ROOT / policy['initial_contract']) == policy['initial_contract_sha256']
    assert digest(ROOT / policy['policy_source']) == policy['policy_source_sha256']
    for path, expected in contract['input_sha256'].items():
        assert digest(ROOT / path) == expected, path
    assert policy['candidate_readback_used'] is False
    provenance = review_source(prefix)
    for name in ('prefix', 'suffix', 'fallback'):
        assert (ROOT / (prefix + '-' + name + '.wgsl')).read_bytes() == (
            ROOT / (VALIDATED + '-' + name + '.wgsl')).read_bytes()
    assert report['cartesian_suffix'] and report['selected'] and not report['reference']
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['output_bytes'] == 1856 and report['terminal_bytes'] == 1120
    assert report['diagnostic_bytes_per_record'] == 32
    expected_steps = ['cs_prefix', 'cs_suffix', 'cs_fallback', 'cs_validate', 'cs_finalize']
    assert [s['entry'] for s in report['steps']] == expected_steps
    assert [s['module_index'] for s in report['steps']] == [0, 1, 2, 2, 2]
    assert all(s['dispatch'] == [1, 1, 1] and s['repetitions'] == 1 for s in report['steps'])
    assert not report['timed'] and not report['iterations']
    raw = np.fromfile(ROOT / (prefix + '-readback.bin'), dtype='<u4')
    states = np.fromfile(ROOT / (prefix + '-states.bin'), dtype='<u4')
    progress = np.fromfile(ROOT / (prefix + '-progress.bin'), dtype='<u4')
    assert raw.nbytes == 1856 and states.nbytes == report['state_bytes'] == 2208
    assert progress.nbytes == report['progress_bytes'] == 16
    states = states.reshape(23, 24)
    terminal = raw[4:280].reshape(23, 12)
    detail = raw[280:].reshape(23, 8)
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    assert progress.tolist() == report['progress_words']
    original_raw = np.fromfile(ROOT / (contract['baseline'] + '-readback.bin'), dtype='<u4')
    original_terminal = original_raw[4:280].reshape(23, 12)
    original_detail = original_raw[280:].reshape(23, 8)
    assert np.array_equal(detail[:, 4:7], original_detail[:, 4:7])
    assert np.array_equal(states[:, 8:11], detail[:, 4:7])
    assert np.all(detail[:, 7] == 0) and np.all(states[:, 23] == 0)
    assert np.all(states[:, 11] == 0) and np.array_equal(states[:, 21], np.arange(23))
    assert np.array_equal(states[:, 12:15], detail[:, :3])
    assert np.array_equal(states[:, 22] | (states[:, 15] << 8), detail[:, 3])
    assert np.all(states[:, 12:15] <= 4096)
    assert np.all(states[:, 12].astype(np.uint64) + states[:, 13] <= 4096)
    assert np.all(states[:, 22] <= 3)
    work = states[:, 12:15].astype(np.uint64).sum(axis=1)
    assert progress.tolist() == [0, 23, int(work.max()), 0]
    floats = terminal.copy().view('<f4')
    assert np.isfinite(floats[:, :7]).all() and np.isfinite(floats[:, 8:11]).all()
    primary = np.fromfile(ROOT / (prefix + '-primary.bin'), dtype='<f4')
    assert primary.nbytes == 2240
    layer_count = int(primary[36])
    assert 2 <= layer_count <= 128 and primary[36] == layer_count
    knots = primary[48:].reshape(128, 4)[:layer_count, 0].astype(float)
    assert np.isfinite(knots).all() and np.all(np.diff(knots) > 0)
    references = {r['record_index']: r for r in baseline['cases']}
    origin = np.asarray(baseline['cases'][0]['inputs']['origin_ecef_m'], dtype=float)
    _, up = geometry(origin)
    east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
    basis = np.column_stack((east, up, -np.cross(up, east)))
    names = {0: 'unhanded-original', 1: 'accepted-suffix', 2: 'original-fallback', 3: 'masked'}
    rows = []
    for i in range(23):
        sample = report['samples'][i]
        assert sample['record_index'] == i and sample['pixel'] == policy['records'][i]['pixel']
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist()
        assert sample['state_words'] == states[i].tolist()
        route = int(states[i, 22])
        counts = states[i, 12:15].tolist()
        reason = int(states[i, 15])
        row = {'record_index': i, 'pixel': sample['pixel'], 'route': names[route],
               'attempts': dict(zip(('prefix', 'suffix', 'fallback'), counts)),
               'failure_reason': reason, 'status': int(terminal[i, 7]),
               'original_status': int(original_terminal[i, 7]),
               'launch_words': detail[i, 4:7].tolist(),
               'terminal_words': terminal[i].tolist(), 'state_words': states[i].tolist(),
               'differing_original_terminal_word_indices': np.flatnonzero(terminal[i] != original_terminal[i]).tolist()}
        if route in (1, 2):
            assert counts[0] > 0 and counts[1] > 0
            handoff = states[i].copy().view('<f4').astype(float)
            assert np.isfinite(handoff[:8]).all() and np.isfinite(handoff[16:20]).all()
            assert 0 <= handoff[7] <= handoff[3]
            owner = int(states[i, 20].copy().view('<i4'))
            h = float(handoff[19])
            assert -1 <= owner < layer_count-1
            assert h < knots[0] if owner == -1 else knots[owner] < h < knots[owner+1]
            row['handoff'] = {'p': handoff[:3].tolist(), 'u': handoff[4:7].tolist(),
                              'path': float(handoff[3]), 'entry': float(handoff[7]),
                              'stored_geometry': handoff[16:20].tolist(), 'stored_source_owner': owner,
                              'stored_owner_matches_strict_packed_field_interval': True}
        else:
            assert not counts[1] and not counts[2] and reason == 0
            assert np.all(states[i, :8] == 0) and np.all(states[i, 16:20] == 0)
            assert int(states[i, 20]) == 0xfffffffe
        errors = None
        if route == 1:
            assert terminal[i, 7] == 0 and counts[2] == 0 and reason == 0
            reference = references[i]
            assert reference['launch_words'] == detail[i, 4:7].tolist()
            assert reference['inputs']['origin_ecef_m'] == origin.tolist()
            tight = reference['tight_reference']
            f = floats[i].astype(float)
            errors = {'position_m': float(np.linalg.norm(origin+basis@f[:3]-tight['endpoint_ecef_m'])),
                      'path_m': abs(float(f[3])-tight['geometric_path_m']),
                      'direction': float(np.linalg.norm(basis@f[4:7]-tight['exit_direction']))}
            row.update(candidate_error=errors, original_error=reference['original_error'],
                       reference_convergence=reference['reference_convergence'],
                       exact_candidate_minus_allowed_error={k: str(Q(errors[k])-Q(reference['original_error'][k])-
                                                                  Q(reference['reference_convergence'][k])) for k in METRICS})
        elif route == 2:
            assert reason in (2, 3, 4, 5)
        row['policy_result'] = evaluate_selected_record(policy, i, names[route],
            terminal[i].tolist(), detail[i, 4:7].tolist(), errors)
        rows.append(row)
    routes = Counter(r['route'] for r in rows)
    failures = [r['record_index'] for r in rows if not r['policy_result']['passed']]
    sky = [r for r in rows if r['route'] == 'accepted-suffix']
    result = {
        'scope': __doc__.strip(), 'prefix': prefix, 'source_and_input_review': provenance,
        'baseline_report_sha256': digest(ROOT / policy['baseline_report']),
        'prospective_policy_sha256': digest(ROOT / POLICY),
        'candidate_modules_equal_offline_reviewed_modules': True,
        'raw_readback_sha256': digest(ROOT / (prefix + '-readback.bin')),
        'raw_states_sha256': digest(ROOT / (prefix + '-states.bin')),
        'raw_progress_sha256': digest(ROOT / (prefix + '-progress.bin')),
        'record_count': 23, 'progress': progress.tolist(), 'route_counts': dict(routes),
        'status_counts': {str(k): v for k, v in Counter(terminal[:, 7].tolist()).items()},
        'all_actual_launch_words_equal_original': True,
        'unhanded_or_fallback_records_exact': sum(r['route'] in ('unhanded-original', 'original-fallback')
                                                and not r['differing_original_terminal_word_indices'] for r in rows),
        'accepted_suffix_error_maxima': {k: max(r['candidate_error'][k] for r in sky) for k in METRICS} if sky else {},
        'positive_suffix_completions': len(sky), 'failed_policy_records': failures,
        'selected_policy_passed': not failures and bool(sky),
        'decision': 'reject-selected-per-ray-nonregression-gate' if failures else ('reject-no-suffix-completions' if not sky else 'selected-gate-only-pass'),
        'full_frame_timing_authorized_by_this_report': False,
        'limitations': [
            'Uses pre-candidate canonical actual-launch DOP853 references and their measured convergence; not a rigorous global ODE enclosure.',
            'Canonical-versus-packed field/coordinate and normalized-reference-launch differences remain in measured errors.',
            'Persistent state is the handoff state, not the suffix terminal; source-owner check concerns stored geometry and packed source intervals only.',
            'No fallback route was exercised if its count is zero; exact fallback source alone does not establish runtime parity for that branch.',
            'Untimed selected diagnostic. No full-frame, native, scene-wide, synthetic-matrix or production acceptance.'],
        'records': rows, 'reviewer_source_sha256': digest(__file__)}
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    output = ROOT / args.output
    assert not output.exists(), 'Preserve prior evidence'
    result = review(args.prefix)
    output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps({k: result[k] for k in ['decision', 'route_counts', 'progress', 'accepted_suffix_error_maxima', 'failed_policy_records']}))
