"""Read-only audit of the fixed canonical handoff screen; no ODE replays.

Exact Fraction midpoint cells verify final binary32 encoding independently of
the producer's candidate-neighbor minimization. Frozen metric gates retain the
original binary64 reference convention; convergence does not enlarge them.
"""
import argparse
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path

import numpy as np

from verify_optical_dop853 import geometry


SELECTED = 'artifacts/optical-cartesian-suffix-tight-selected-first'
ATMOSPHERE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first'
SELECTED_IDS = [3, 9, 10, 11, 12, 14, 15, 17, 18, 20]
METRICS = ('position_m', 'path_m', 'direction')
LABELS = ('A_raw_initial_vector', 'B_normalized_initial_orientation')


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def hashes(rows):
    for row in rows:
        assert digest(row['path']) == row['sha256'], row['path']


def raw(prefix):
    states = np.fromfile(prefix+'-states.bin', dtype='<u4').reshape(23, 24)
    output = np.fromfile(prefix+'-readback.bin', dtype='<u4')
    assert output.size == 464
    report = load(prefix+'.json')
    assert output[:4].tolist() == report['expected_header']
    return states, output[4:280].reshape(23, 12), report


def nearest_even_cell(word, exact):
    """Membership in the encoded value's exact midpoint Voronoi cell."""
    value = np.array([word], dtype='<u4').view('<f4')[0]
    assert np.isfinite(value)
    previous = np.nextafter(value, np.float32(-np.inf))
    following = np.nextafter(value, np.float32(np.inf))
    lower = (Q(float(previous))+Q(float(value)))/2
    upper = (Q(float(value))+Q(float(following)))/2
    assert lower <= exact <= upper
    if exact in (lower, upper):
        assert word & 1 == 0


def audit(prefix):
    report = load(prefix+'.json')
    sources = load(prefix+'-sources.json')
    stability = load(prefix+'-input-stability.json')
    assert stability['inputs_unchanged_through_run'] and sources['files'] == stability['after_run']
    hashes(sources['files'])
    outputs = load(prefix+'-outputs.json')
    hashes(outputs)
    for path, expected in report['source_and_input_hashes'].items():
        assert digest(path) == expected
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None
    assert not life['timed_out'] and not life['terminated_by_wrapper']
    source_review = load('artifacts/optical-handoff-feasibility-source-review-first.json')
    assert digest('scripts/optical_handoff_feasibility.py') == source_review['source_sha256']
    assert digest('scripts/verify_optical_handoff.ps1') == source_review['wrapper_sha256']
    assert report['raw_oracle_source_sha256'] == source_review['raw_oracle_source_sha256']
    assert not report['gpu_execution'] and not report['runtime_implementation'] and not report['performance_claim']
    selected = load('artifacts/optical-cartesian-suffix-selected-baseline-first.json')
    amendment = load('artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json')
    atmosphere = load('artifacts/optical-cartesian-matrix-atmosphere-100-gradient-baseline-freeze-first.json')
    assert digest('artifacts/optical-cartesian-suffix-selected-baseline-first.json') == amendment['baseline_sha256']
    for freeze in (selected, atmosphere):
        assert digest(freeze['contract']) == freeze['contract_sha256']
    selected_refs = {r['record_index']: r for r in selected['cases']}
    selected_limits = {r['record_index']: r['per_metric_error_ceiling_exact'] for r in amendment['records']
                       if r['record_index'] in SELECTED_IDS}
    atmosphere_ref = next(r for r in atmosphere['references'] if r['record_index'] == 1)
    planned = [('atmosphere-100-gradient/1', ATMOSPHERE, 1, atmosphere_ref,
                atmosphere_ref['per_metric_ceiling_exact'], True)]
    planned += [(f'selected/{i}', SELECTED, i, selected_refs[i], selected_limits[i], False) for i in SELECTED_IDS]
    assert report['planned_cases'] == [x[0] for x in planned] == source_review['case_ids']
    inputs = {p: raw(p) for p in (SELECTED, ATMOSPHERE)}
    assert np.flatnonzero(inputs[SELECTED][0][:, 22] == 1).tolist() == SELECTED_IDS
    selected_contract = load(selected['contract'])
    assert digest(SELECTED+'-primary.bin') == selected_contract['input_sha256']['artifacts/optical-normal-scene-reference-selected-primary.bin']
    assert digest(ATMOSPHERE+'-primary.bin') == load(atmosphere['contract'])['packet_checks']['five_uploads']['primary']
    assert 0 < len(report['records']) <= len(planned)
    assert report['unrun_cases'] == [x[0] for x in planned[len(report['records']):]]
    rounding_checks = 0
    metric_checks = 0
    rows = []
    for index, (row, item) in enumerate(zip(report['records'], planned)):
        name, source, i, reference, limits, absolute = item
        assert row['case'] == name and row['source_prefix'] == source and row['record_index'] == i
        states, terminal, _ = inputs[source]
        assert row['saved_state_words'] == states[i].tolist()
        assert states[i, 22] == 1 and terminal[i, 7] == 0
        assert states[i, 8:11].tolist() == reference['launch_words']
        assert all(Q(limits[m]) == Q(reference['original_error'][m])+Q(reference['reference_convergence'][m]) for m in METRICS)
        assert row['frozen_limits_exact'] == limits
        saved = states[i].copy().view('<f4').astype(float)
        origin = np.asarray(reference['inputs']['origin_ecef_m'], dtype=float)
        _, up = geometry(origin)
        east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
        basis = np.column_stack((east, up, -np.cross(up, east)))
        handoff = origin+basis@saved[:3]
        direction = basis@saved[4:7]
        assert np.array_equal(row['basis'], basis) and np.array_equal(row['original_eye_ecef'], origin)
        assert np.array_equal(row['handoff_ecef'], handoff) and np.array_equal(row['raw_world_direction'], direction)
        assert row['saved_position_path'] == saved[:4].tolist() and row['saved_direction_entry'] == saved[4:8].tolist()
        height, _ = geometry(handoff)
        heights = np.array([reference['inputs'].get('profile_base_height_m', 0)+k['height_m'] for k in reference['inputs']['layers']])
        owner = int(states[i, 20].view('<i4'))
        expected_owner = int(np.searchsorted(heights, height, side='right'))-1
        near = np.flatnonzero(abs(heights-height) < 2e-8).tolist()
        assert row['canonical_height'] == height and row['oracle_near_band_knots'] == near
        assert row['stored_owner'] == owner and row['canonical_owner'] == expected_owner
        lower = -10000. if owner == -1 else float(heights[owner]) if 0 <= owner < len(heights)-1 else None
        upper = float(heights[max(0, owner+1)]) if -1 <= owner < len(heights)-1 else None
        assert row['canonical_owner_bounds'] == [lower, upper]
        owner_ok = not near and owner == expected_owner and lower is not None and upper is not None and lower < height < upper
        direction_norm = np.linalg.norm(direction)
        assert np.isfinite(direction_norm) and direction_norm > 0
        correction = float(np.linalg.norm(direction/direction_norm-direction))
        assert correction == row['initial_normalization_correction_norm']
        hypothesis_rows = {}
        if not owner_ok:
            assert not row['passed'] and row['hypotheses'] == {}
        else:
            assert set(row['hypotheses']) == set(LABELS)
            for label in LABELS:
                result = row['hypotheses'][label]
                if result.get('unresolved'):
                    assert not result['passed']
                    hypothesis_rows[label] = {'passed': False, 'unresolved': True}
                    continue
                values = [result['coarse'], result['tight']]
                encodings = result['encoded_words_coarse_tight']
                gates = []
                top_residuals = []
                for k, (value, encoded) in enumerate(zip(values, encodings)):
                    assert len(encoded) == 7
                    assert value['relative_tolerance'] == (2e-12 if k == 0 else 5e-14)
                    assert value['maximum_step_m'] == (2000. if k == 0 else 500.)
                    assert value['absolute_tolerances'] == (([2e-7]*3+[2e-14]*3+[1e-9]) if k == 0 else ([2e-9]*3+[5e-16]*3+[1e-11]))
                    assert value['accepted_steps'] > 0 and value['function_calls'] > 0 and value['layer_contacts'] > 0
                    assert np.isfinite(value['endpoint_ecef_m']).all() and np.isfinite(value['exit_direction']).all()
                    assert np.isfinite(value['geometric_path_m']) and value['geometric_path_m'] > 0
                    local_p = basis.T@(np.asarray(value['endpoint_ecef_m'])-origin)
                    local_u = basis.T@np.asarray(value['exit_direction'])
                    exact_values = list(map(Q, local_p))+[Q(float(saved[3]))+Q(value['geometric_path_m'])]+list(map(Q, local_u))
                    for word, exact in zip(encoded, exact_values):
                        nearest_even_cell(word, exact)
                        rounding_checks += 1
                    f = np.asarray(encoded, dtype='<u4').view('<f4').astype(float)
                    truth = reference['tight_reference']
                    errors = dict(zip(METRICS, (float(np.linalg.norm(origin+basis@f[:3]-truth['endpoint_ecef_m'])),
                        abs(f[3]-truth['geometric_path_m']), float(np.linalg.norm(basis@f[4:7]-truth['exit_direction'])))))
                    checks = {m: Q(errors[m]) <= Q(limits[m]) for m in METRICS}
                    old_limits = {m: Q(errors[m]) < Q(v) for m, v in zip(METRICS, (5., 5., 3e-6))} if absolute else None
                    expected = {'errors': errors, 'same_ray_checks': checks, 'absolute_checks': old_limits,
                                'passed': all(checks.values()) and (not absolute or all(old_limits.values()))}
                    assert result['evaluations'][k] == expected
                    gates.append(expected['passed'])
                    metric_checks += 3
                    top_residuals.append(float(geometry(np.asarray(value['endpoint_ecef_m']))[0]-heights[-1]))
                coarse, tight = values
                convergence = {'endpoint_m': float(np.linalg.norm(np.asarray(coarse['endpoint_ecef_m'])-tight['endpoint_ecef_m'])),
                    'direction': float(np.linalg.norm(np.asarray(coarse['exit_direction'])-tight['exit_direction'])),
                    'path_m': abs(coarse['geometric_path_m']-tight['geometric_path_m']),
                    'optical_path_m': abs(coarse['optical_path_m']-tight['optical_path_m'])}
                assert convergence == result['observed_convergence']
                stable = encodings[0] == encodings[1] and gates[0] == gates[1]
                domain = all(saved[3]-saved[7]+v['geometric_path_m'] <= 2_000_000 for v in values)
                assert result['encoding_and_gate_stable'] == stable and result['original_remaining_path_domain'] == domain
                assert result['passed'] == (stable and domain and all(gates))
                hypothesis_rows[label] = {'passed': result['passed'], 'errors': result['evaluations'][1]['errors'],
                    'frozen_checks': result['evaluations'][1]['same_ray_checks'], 'coarse_tight_encoding_exact': stable,
                    'observed_convergence': convergence, 'descriptive_terminal_top_residuals_m': top_residuals}
            assert row['passed'] == all(x['passed'] for x in row['hypotheses'].values())
        if not row['passed']:
            assert index == len(report['records'])-1
        rows.append({'case': name, 'passed': row['passed'], 'strict_owner_verified': bool(owner_ok),
            'normalization_correction_norm': correction, 'hypotheses': hypothesis_rows})
    assert report['inputs_unchanged'] and report['passed'] == (not report['unrun_cases'] and all(x['passed'] for x in rows))
    return {'schema': 'canonical-handoff-independent-review-v1', 'input': prefix, 'auditor_sha256': digest(__file__),
        'source_review_sha256': digest('artifacts/optical-handoff-feasibility-source-review-first.json'),
        'source_and_input_hashes_verified': True, 'source_manifest_sha256': digest(prefix+'-sources.json'),
        'outputs_manifest_sha256': digest(prefix+'-outputs.json'), 'report_sha256': digest(prefix+'.json'),
        'lifecycle_sha256': digest(prefix+'-lifecycle.json'), 'case_count': len(rows),
        'hypothesis_count': sum(len(x['hypotheses']) for x in rows),
        'independent_exact_midpoint_rounding_checks': rounding_checks, 'frozen_metric_checks': metric_checks,
        'repeated_oracle_integrations': 0, 'all_reported_gates_reproduced': True,
        'passed_fixed_canonical_screen': report['passed'], 'unrun_cases': report['unrun_cases'], 'records': rows,
        'scope': 'Finite convergence and final encoding screen for two canonical initialization hypotheses. A is an off-unit parameterized extension; B changes the saved vector by normalization. Original frozen ceilings are unchanged. No rounded uploaded quadric equivalence, global ODE bound, dense scene/source continuation, mixed-precision implementation, GPU execution, or performance acceptance follows.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    result = audit(args.prefix)
    with Path(args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps({k: result[k] for k in ('case_count', 'hypothesis_count', 'independent_exact_midpoint_rounding_checks',
        'frozen_metric_checks', 'repeated_oracle_integrations', 'passed_fixed_canonical_screen')}, indent=2))
