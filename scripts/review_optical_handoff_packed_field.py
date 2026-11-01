"""Independent retained-output checks for the promoted uploaded-field screen.

No ODE integrations are replayed. Decode the packet with struct, reconstruct
the stated scalar geometry, and verify rounding with exact midpoint cells.
This checks a continuous f64 hypothesis, not the GPU's arithmetic/controller.
"""
import argparse
from fractions import Fraction as Q
import json
import math
from pathlib import Path
import struct

import numpy as np

from review_optical_handoff_feasibility import (
    ATMOSPHERE, SELECTED, SELECTED_IDS, METRICS, LABELS,
    digest, hashes, load, raw, nearest_even_cell,
)
from verify_optical_dop853 import geometry as canonical_geometry


SOURCE_REVIEW = 'artifacts/optical-handoff-packed-field-source-review-first.json'


def uploaded_geometry(position, matrix, origin):
    """Independent scalar transcription of the explicitly reviewed f64 order."""
    def dot(x, y):
        first = x[0]*y[0] + x[1]*y[1]
        return first + x[2]*y[2]

    def normalize(x):
        length = math.sqrt(dot(x, x))
        assert math.isfinite(length) and length > 0
        return [v/length for v in x]

    p = [float(x) for x in position]
    assert all(math.isfinite(x) for x in p)
    product = [dot(row, p) for row in matrix]
    gradient = [origin[i] + product[i] for i in range(3)]
    implicit = origin[3] + dot(p, [2*origin[i] + product[i] for i in range(3)])
    assert all(math.isfinite(x) for x in (*gradient, implicit))
    assert math.sqrt(dot(gradient, gradient)) >= .75
    normal = normalize(gradient)
    for _ in range(5):
        old_bits = struct.pack('<3d', *normal)
        qn = [dot(row, normal) for row in matrix]
        projection = dot(normal, gradient)
        discriminant = projection*projection - implicit*dot(normal, qn)
        assert math.isfinite(discriminant)
        denominator = projection + math.sqrt(max(0., discriminant))
        assert math.isfinite(denominator) and denominator > 0
        height = implicit/denominator
        normal = normalize([gradient[i] - height*qn[i] for i in range(3)])
        if struct.pack('<3d', *normal) == old_bits:
            break
    assert math.isfinite(height)
    return height, normal


def packet(prefix):
    data = Path(prefix+'-primary.bin').read_bytes()
    assert len(data) == 2240
    words = struct.unpack('<560I', data)
    values = struct.unpack('<560f', data)
    assert all(math.isfinite(x) for x in values[:44])
    count = int(values[36])
    assert values[36] == count and 2 <= count <= 128 and values[39] >= .5
    matrix = [list(values[4*k:4*k+3]) for k in range(3)]
    origin = list(values[12:16])
    layers = [list(values[48+4*k:51+4*k]) for k in range(count)]
    assert all(math.isfinite(x) for layer in layers for x in layer)
    assert all(layers[k][0] < layers[k+1][0] for k in range(count-1))
    assert words[41] == words[48] and words[37] == words[48+4*(count-1)]
    lower, top = values[42], values[37]
    assert lower < layers[0][0] and 1+layers[0][1] > 0
    for k in range(count-1):
        n0 = layers[k][1]
        n1 = n0 + layers[k][2]*(layers[k+1][0]-layers[k][0])
        assert math.isfinite(n1) and min(1+n0, 1+n1) > 0
    return words, matrix, origin, layers, lower, top


def audit(prefix):
    report = load(prefix+'.json')
    source_review = load(SOURCE_REVIEW)
    assert source_review['passed'] and source_review['ode_integrations_executed'] == 0
    assert report['schema'] == 'uploaded-field-handoff-feasibility-v1' and report['packed_field']
    assert report['raw_oracle_source_sha256'] is None
    assert not any(report[k] for k in ('gpu_execution', 'runtime_implementation', 'performance_claim'))
    sources = load(prefix+'-sources.json')
    stability = load(prefix+'-input-stability.json')
    assert sources['packed_field'] and stability['inputs_unchanged_through_run']
    assert sources['files'] == stability['after_run']
    hashes(sources['files'])
    outputs = load(prefix+'-outputs.json')
    hashes(outputs)
    output_paths = {Path(x['path']).resolve() for x in outputs}
    for suffix in ('.json', '.partial.json', '-sources.json', '-lifecycle.json',
                   '-input-stability.json', '.stdout.log', '.stderr.log'):
        assert Path(prefix+suffix).resolve() in output_paths
    assert report['source_and_input_hashes'] == source_review['source_and_input_sha256']
    for path, expected in report['source_and_input_hashes'].items():
        assert digest(path) == expected
    manifest = {Path(x['path']).resolve(): x['sha256'] for x in sources['files']}
    assert all(manifest[Path(p).resolve()] == h for p, h in report['source_and_input_hashes'].items())
    lifecycle = load(prefix+'-lifecycle.json')
    assert lifecycle['exit_code'] == 0 and lifecycle['failure'] is None
    assert not lifecycle['timed_out'] and not lifecycle['terminated_by_wrapper']
    assert lifecycle['arguments'] == ['-B', '-u', 'scripts/optical_handoff_feasibility.py', prefix, '--packed-field']
    assert Path(lifecycle['executable']).resolve() == Path(sources['runtime']['executable']).resolve()
    assert 1 <= lifecycle['timeout_seconds'] <= 600
    partial = load(prefix+'.partial.json')
    assert partial == {k: v for k, v in report.items() if k not in ('inputs_unchanged', 'passed', 'decision')}

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
    approved = source_review['actual_packet_and_saved_owner_checks']
    assert report['planned_cases'] == [x[0] for x in planned] == [x['case'] for x in approved]
    inputs = {p: raw(p) for p in (SELECTED, ATMOSPHERE)}
    for p in inputs:
        old_manifest = load(p+'-outputs.json')
        for suffix in ('-primary.bin', '-states.bin', '-readback.bin'):
            entries = [x for x in old_manifest if Path(x['path']).resolve() == Path(p+suffix).resolve()]
            assert len(entries) == 1 and digest(p+suffix) == entries[0]['sha256']
    assert np.flatnonzero(inputs[SELECTED][0][:, 22] == 1).tolist() == SELECTED_IDS
    assert digest(SELECTED+'-primary.bin') == load(selected['contract'])['input_sha256']['artifacts/optical-normal-scene-reference-selected-primary.bin']
    assert digest(ATMOSPHERE+'-primary.bin') == load(atmosphere['contract'])['packet_checks']['five_uploads']['primary']
    assert 0 < len(report['records']) <= len(planned)
    assert report['unrun_cases'] == [x[0] for x in planned[len(report['records']):]]
    rounding_checks, metric_checks, rows = 0, 0, []
    for index, (row, item) in enumerate(zip(report['records'], planned)):
        name, source, i, reference, limits, absolute = item
        assert (row['case'], row['source_prefix'], row['record_index']) == (name, source, i)
        states, terminals, _ = inputs[source]
        assert row['saved_state_words'] == states[i].tolist()
        assert states[i, 22] == 1 and terminals[i, 7] == 0 and states[i, 14] == states[i, 15] == 0
        assert states[i, 8:11].tolist() == reference['launch_words']
        saved = states[i].copy().view('<f4').astype(float)
        assert np.isfinite(saved[:8]).all()
        assert row['saved_position_path'] == saved[:4].tolist() and row['saved_direction_entry'] == saved[4:8].tolist()
        assert row['frozen_limits_exact'] == limits
        assert all(Q(limits[m]) == Q(reference['original_error'][m])+Q(reference['reference_convergence'][m]) for m in METRICS)
        words, matrix, offset, layers, lower_domain, top = packet(source)
        field = row['packed_field']
        assert field['primary_words'] == [list(words[k:k+4]) for k in range(0, 560, 4)]
        assert field['q_rows_f64'] == matrix and field['origin_f64'] == offset
        assert field['layers_height_N_slope_f64'] == layers
        assert field['lower_domain'] == lower_domain and field['top'] == top
        symmetric = all(words[4*k+j] == words[4*j+k] for k in range(3) for j in range(3))
        assert field['q_is_bitwise_symmetric'] == symmetric
        owner = int(states[i, 20].view('<i4'))
        assert -1 <= owner < len(layers)-1 and row['stored_owner'] == owner
        low = lower_domain if owner == -1 else layers[owner][0]
        high = layers[owner+1][0]
        height, _ = uploaded_geometry(saved[:3], matrix, offset)
        assert row['packed_height'] == height and row['packed_owner_bounds'] == [low, high]
        assert approved[index]['packed_height'] == height and approved[index]['saved_owner'] == owner
        owner_ok = low < height < high
        origin = np.asarray(reference['inputs']['origin_ecef_m'], dtype=float)
        _, up = canonical_geometry(origin)
        east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
        basis = np.column_stack((east, up, -np.cross(up, east)))
        assert np.array_equal(row['original_eye_ecef'], origin) and np.array_equal(row['basis'], basis)
        assert row['raw_local_direction'] == saved[4:7].tolist()
        length = np.linalg.norm(saved[4:7]); assert math.isfinite(length) and length > 0
        correction = float(np.linalg.norm(saved[4:7]/length-saved[4:7]))
        assert correction == row['initial_normalization_correction_norm']
        hypotheses = {}
        if not owner_ok:
            assert not row['passed'] and row['hypotheses'] == {}
        else:
            assert set(row['hypotheses']) == set(LABELS)
            for label in LABELS:
                result = row['hypotheses'][label]
                normalized = label == LABELS[1]
                assert result['oracle_adapter_source_sha256'] == approved[index]['adapter_sha256'][str(normalized)]
                if result.get('unresolved'):
                    assert not result['passed'] and result['error_type']
                    hypotheses[label] = {'passed': False, 'unresolved': True}
                    continue
                values = [result['coarse'], result['tight']]
                encodings = result['encoded_words_coarse_tight']; assert len(encodings) == 2
                gates, residuals = [], []
                for k, (value, encoded) in enumerate(zip(values, encodings)):
                    assert value['terminal_kind'] == 'atmosphere_exit'
                    assert value['relative_tolerance'] == (2e-12 if k == 0 else 5e-14)
                    assert value['maximum_step_m'] == (2000. if k == 0 else 500.)
                    assert value['absolute_tolerances'] == (([2e-7]*3+[2e-14]*3+[1e-9]) if k == 0 else ([2e-9]*3+[5e-16]*3+[1e-11]))
                    for counter in ('accepted_steps', 'function_calls', 'layer_contacts'):
                        assert type(value[counter]) is int and value[counter] > 0
                    assert value['layer_contacts'] <= 1000
                    assert len(value['endpoint_local']) == len(value['u_local']) == 3
                    assert np.isfinite(value['endpoint_local']).all() and np.isfinite(value['u_local']).all()
                    assert math.isfinite(value['geometric_path_m']) and value['geometric_path_m'] > 0
                    assert math.isfinite(value['optical_path_m']) and math.isfinite(value['direction_norm_drift'])
                    assert value['direction_norm_drift'] >= 0
                    assert len(encoded) == 7
                    exact = list(map(Q, value['endpoint_local'])) + [Q(float(saved[3]))+Q(value['geometric_path_m'])] + list(map(Q, value['u_local']))
                    for word, x in zip(encoded, exact):
                        nearest_even_cell(word, x); rounding_checks += 1
                    encoded_f = np.asarray(encoded, dtype='<u4').view('<f4').astype(float)
                    truth = reference['tight_reference']
                    errors = dict(zip(METRICS, (
                        float(np.linalg.norm(origin+basis@encoded_f[:3]-truth['endpoint_ecef_m'])),
                        abs(encoded_f[3]-truth['geometric_path_m']),
                        float(np.linalg.norm(basis@encoded_f[4:7]-truth['exit_direction'])))))
                    checks = {m: Q(errors[m]) <= Q(limits[m]) for m in METRICS}
                    absolute_checks = {m: Q(errors[m]) < Q(v) for m, v in zip(METRICS, (5., 5., 3e-6))} if absolute else None
                    expected = {'errors': errors, 'same_ray_checks': checks, 'absolute_checks': absolute_checks,
                                'passed': all(checks.values()) and (not absolute or all(absolute_checks.values()))}
                    assert result['evaluations'][k] == expected
                    gates.append(expected['passed']); metric_checks += 3
                    residuals.append(uploaded_geometry(value['endpoint_local'], matrix, offset)[0]-top)
                coarse, tight = values
                convergence = {'endpoint_m': float(np.linalg.norm(np.asarray(coarse['endpoint_local'])-tight['endpoint_local'])),
                    'direction': float(np.linalg.norm(np.asarray(coarse['u_local'])-tight['u_local'])),
                    'path_m': abs(coarse['geometric_path_m']-tight['geometric_path_m']),
                    'optical_path_m': abs(coarse['optical_path_m']-tight['optical_path_m'])}
                assert result['observed_convergence'] == convergence
                stable = encodings[0] == encodings[1] and gates[0] == gates[1]
                domain = all(saved[3]-saved[7]+value['geometric_path_m'] <= 2_000_000 for value in values)
                assert result['encoding_and_gate_stable'] == stable and result['original_remaining_path_domain'] == domain
                assert result['passed'] == (stable and domain and all(gates))
                hypotheses[label] = {'passed': result['passed'], 'errors': result['evaluations'][1]['errors'],
                    'frozen_checks': result['evaluations'][1]['same_ray_checks'], 'absolute_checks': result['evaluations'][1]['absolute_checks'],
                    'coarse_tight_encoding_exact': stable, 'observed_convergence': convergence,
                    'descriptive_packed_top_residuals_m': residuals}
            assert row['passed'] == all(x['passed'] for x in row['hypotheses'].values())
        if not row['passed']:
            assert index == len(report['records'])-1
        jumps = [layers[k][1]+layers[k][2]*(layers[k+1][0]-layers[k][0])-layers[k+1][1] for k in range(len(layers)-1)]
        rows.append({'case': name, 'passed': row['passed'], 'strict_saved_owner_verified': bool(owner_ok),
            'q_bitwise_symmetric': symmetric, 'one_sided_N_differences': jumps,
            'normalization_correction_norm': correction, 'hypotheses': hypotheses})
    assert report['inputs_unchanged'] and report['passed'] == (not report['unrun_cases'] and all(x['passed'] for x in rows))
    return {'schema': 'uploaded-field-handoff-independent-review-v1', 'input': prefix,
        'auditor_sha256': digest(__file__), 'source_review_sha256': digest(SOURCE_REVIEW),
        'source_manifest_sha256': digest(prefix+'-sources.json'), 'outputs_manifest_sha256': digest(prefix+'-outputs.json'),
        'report_sha256': digest(prefix+'.json'), 'partial_sha256': digest(prefix+'.partial.json'),
        'lifecycle_sha256': digest(prefix+'-lifecycle.json'), 'all_source_runtime_and_output_hashes_verified': True,
        'case_count': len(rows), 'hypothesis_count': sum(len(x['hypotheses']) for x in rows),
        'independent_exact_midpoint_rounding_checks': rounding_checks, 'frozen_metric_checks': metric_checks,
        'repeated_oracle_integrations': 0, 'all_reported_gates_reproduced': True,
        'passed_fixed_uploaded_field_screen': report['passed'], 'unrun_cases': report['unrun_cases'], 'records': rows,
        'scope': ['Finite retained-output checks of the explicitly ordered f64 uploaded-field hypothesis.',
            'Q skew, packed slopes, one-sided N differences and strict saved owner are retained.',
            'A retains the raw off-unit vector and its non-arc-length parameter; B normalizes initially. Both normalize the final direction.',
            'Local outputs are rounded directly; saved path plus represented suffix path is checked as an exact rational sum before f32 rounding.',
            'Canonical full-eye truth and all prospective ceilings are unchanged; observed convergence adds no allowance.',
            'Descriptive terminal-height residuals add no new acceptance tolerance. No ODE integration was repeated.',
            'No rigorous global ODE enclosure, model-gradient equivalence, WGSL arithmetic, adaptive-controller/source-root equivalence, scene chronology, mixed-precision implementation, GPU or speed acceptance.']}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    result = audit(args.prefix)
    with Path(args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({k: result[k] for k in ('case_count', 'hypothesis_count',
        'independent_exact_midpoint_rounding_checks', 'frozen_metric_checks',
        'repeated_oracle_integrations', 'passed_fixed_uploaded_field_screen')}, indent=2))
