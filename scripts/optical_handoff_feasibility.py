"""CPU rejection screen for canonical continuation from frozen GPU handoffs.

This is not a compensated implementation. A retains the raw off-unit initial
vector in the oracle equations; its path parameter is not physical arc length.
B retains the existing oracle's initial normalization, changing that vector.
Both retain its final direction normalization. Neither substitutes for the
rounded uploaded quadric/field, scene chronology, or a GPU performance result.
"""
import argparse
from fractions import Fraction as Q
import hashlib
import inspect
import json
from pathlib import Path
import sys

import numpy as np
import scipy

import verify_optical_dop853 as oracle

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / 'scripts/optical_handoff_feasibility_inputs.txt'
SELECTED = 'artifacts/optical-cartesian-suffix-tight-selected-first'
ATMOSPHERE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first'
SELECTED_REFERENCE = 'artifacts/optical-cartesian-suffix-selected-baseline-first.json'
SELECTED_POLICY = 'artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json'
ATMOSPHERE_REFERENCE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-baseline-freeze-first.json'
EXPECTED_SELECTED = [3, 9, 10, 11, 12, 14, 15, 17, 18, 20]
METRICS = ('position_m', 'path_m', 'direction')
ORACLE_SHA256 = 'cc8252c11f9dc4b1a7c7ed36413849dc798bf1665866a09614ac5d3d19efe0bb'


def load(path):
    return json.loads((ROOT / path).read_text(encoding='utf-8-sig'))


def digest(path):
    with (ROOT / path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def fingerprints():
    paths = [x.strip() for x in INVENTORY.read_text().splitlines()
             if x.strip() and not x.lstrip().startswith('#')]
    assert len(paths) == len(set(paths))
    return {x: digest(x) for x in paths}


def raw_oracle():
    assert digest('scripts/verify_optical_dop853.py') == ORACLE_SHA256
    source = inspect.getsource(oracle.integrate)
    line = '    direction /= np.linalg.norm(direction)\n'
    assert source.count(line) == 1
    raw = source.replace(line, '', 1)
    assert 'final_direction = state[3:6] / np.linalg.norm(state[3:6])' in raw
    namespace = dict(oracle.__dict__)
    exec(compile(raw, '<existing-oracle-without-initial-normalization>', 'exec'), namespace)
    return namespace['integrate'], hashlib.sha256(raw.encode()).hexdigest()


def round_f32(value):
    """Nearest-even binary32 to the exact Fraction; inspect both neighbors."""
    value = Q(value)
    assert abs(value) < 1_000_000_000
    center = np.float32(float(value))
    candidates = [center, np.nextafter(center, np.float32(-np.inf)),
                  np.nextafter(center, np.float32(np.inf))]
    return min(candidates, key=lambda v: (abs(Q(float(v))-value), int(v.view(np.uint32)) & 1))


def basis_at(origin):
    _, up = oracle.geometry(origin)
    east = np.array([-origin[1], origin[0], 0.])
    east /= np.linalg.norm(east)
    return np.column_stack((east, up, -np.cross(up, east)))


def records(prefix):
    report = load(prefix+'.json')
    assert report['record_count'] == 23 and report['selected'] and not report['timed']
    outputs = load(prefix+'-outputs.json')
    for suffix in ('-states.bin', '-readback.bin', '-primary.bin'):
        path = prefix+suffix
        expected = [x for x in outputs if Path(x['path']).resolve() == (ROOT/path).resolve()]
        assert len(expected) == 1 and digest(path) == expected[0]['sha256']
    states = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    assert raw.size == 464 and raw[:4].tolist() == report['expected_header']
    terminal, details = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    for i, sample in enumerate(report['samples']):
        assert sample['record_index'] == i and sample['state_words'] == states[i].tolist()
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == details[i].tolist()
    assert np.array_equal(states[:, 8:11], details[:, 4:7])
    assert np.array_equal(states[:, 21], np.arange(23))
    return report, states, terminal


def cases():
    selected_report, selected_states, selected_terminal = records(SELECTED)
    atmosphere_report, atmosphere_states, atmosphere_terminal = records(ATMOSPHERE)
    assert np.flatnonzero(selected_states[:, 22] == 1).tolist() == EXPECTED_SELECTED
    original = load(SELECTED_REFERENCE)
    policy = load(SELECTED_POLICY)
    assert digest(SELECTED_REFERENCE) == policy['baseline_sha256']
    references = {r['record_index']: r for r in original['cases']}
    policy_rows = {r['record_index']: r for r in policy['records']}
    assert digest(original['contract']) == original['contract_sha256']
    original_contract = load(original['contract'])
    primary_hash = original_contract['input_sha256']['artifacts/optical-normal-scene-reference-selected-primary.bin']
    assert digest(SELECTED+'-primary.bin') == primary_hash
    atmosphere = load(ATMOSPHERE_REFERENCE)
    assert digest(atmosphere['contract']) == atmosphere['contract_sha256']
    atmosphere_contract = load(atmosphere['contract'])
    assert digest(ATMOSPHERE+'-primary.bin') == atmosphere_contract['packet_checks']['five_uploads']['primary']
    atmosphere_ref = {r['record_index']: r for r in atmosphere['references']}[1]
    result = [('atmosphere-100-gradient/1', ATMOSPHERE, 1, atmosphere_ref,
               atmosphere_states[1], atmosphere_terminal[1], atmosphere_ref['per_metric_ceiling_exact'], True)]
    for i in EXPECTED_SELECTED:
        ref = references[i]
        row = policy_rows[i]
        assert ref['launch_words'] == row['launch_words']
        limits = row['per_metric_error_ceiling_exact']
        assert all(Q(limits[m]) == Q(ref['original_error'][m])+Q(ref['reference_convergence'][m]) for m in METRICS)
        result.append((f'selected/{i}', SELECTED, i, ref, selected_states[i],
                       selected_terminal[i], limits, False))
    return result


def encode(result, origin, basis, saved_path):
    local_p = basis.T @ (np.asarray(result['endpoint_ecef_m'])-origin)
    local_u = basis.T @ np.asarray(result['exit_direction'])
    values = [round_f32(x) for x in local_p]
    # This is exact addition of the represented saved-f32 and oracle-f64
    # numbers followed by one f32 rounding, not a simulated GPU accumulator.
    values += [round_f32(Q(saved_path)+Q(result['geometric_path_m']))]
    values += [round_f32(x) for x in local_u]
    encoded = np.asarray(values, dtype='<f4')
    return encoded.view('<u4').tolist()


def evaluate(words, reference, origin, basis, limits, absolute):
    f = np.asarray(words, dtype='<u4').view('<f4').astype(float)
    truth = reference['tight_reference']
    errors = dict(zip(METRICS, (
        float(np.linalg.norm(origin+basis@f[:3]-truth['endpoint_ecef_m'])),
        abs(f[3]-truth['geometric_path_m']),
        float(np.linalg.norm(basis@f[4:7]-truth['exit_direction'])))))
    checks = {m: Q(errors[m]) <= Q(limits[m]) for m in METRICS}
    absolute_checks = {m: Q(errors[m]) < Q(v) for m, v in
                       zip(METRICS, (5., 5., 3e-6))} if absolute else None
    return {'errors': errors, 'same_ray_checks': checks, 'absolute_checks': absolute_checks,
            'passed': all(checks.values()) and (not absolute or all(absolute_checks.values()))}


def run_case(item, raw_integrate):
    name, prefix, i, ref, words, terminal, limits, absolute = item
    assert words[22] == 1 and terminal[7] == 0 and words[14] == words[15] == 0
    assert words[8:11].tolist() == ref['launch_words']
    state = words.copy().view('<f4').astype(float)
    assert np.isfinite(state[:8]).all()
    canonical = dict(ref['inputs'])
    canonical.pop('target_path_m', None)
    origin = np.asarray(canonical['origin_ecef_m'], dtype=float)
    basis = basis_at(origin)
    handoff_origin = origin+basis@state[:3]
    handoff_u = basis@state[4:7]
    direction_norm = float(np.linalg.norm(handoff_u))
    assert np.isfinite(direction_norm) and direction_norm > 0
    canonical['origin_ecef_m'] = handoff_origin.tolist()
    canonical['initial_direction'] = handoff_u.tolist()
    h, _ = oracle.geometry(handoff_origin)
    heights = np.array([canonical.get('profile_base_height_m', 0.)+x['height_m'] for x in canonical['layers']])
    owner = int(words[20].view('<i4'))
    canonical_owner = int(np.searchsorted(heights, h, side='right'))-1
    # The unchanged oracle has a directed override inside this exact band.
    # Refuse such starts rather than silently select a different owner.
    near_knots = np.flatnonzero(abs(heights-h) < 2e-8).tolist()
    lower = -10000. if owner == -1 else float(heights[owner]) if 0 <= owner < len(heights)-1 else None
    upper = float(heights[max(0, owner+1)]) if -1 <= owner < len(heights)-1 else None
    row = {'case': name, 'source_prefix': prefix, 'record_index': i,
           'saved_state_words': words.tolist(), 'saved_position_path': state[:4].tolist(),
           'saved_direction_entry': state[4:8].tolist(), 'basis': basis.tolist(),
           'original_eye_ecef': origin.tolist(), 'handoff_ecef': handoff_origin.tolist(),
           'raw_world_direction': handoff_u.tolist(),
           'initial_normalization_correction_norm': float(np.linalg.norm(handoff_u/direction_norm-handoff_u)),
           'stored_owner': owner, 'canonical_owner': canonical_owner, 'canonical_height': float(h),
           'oracle_near_band_knots': near_knots,
           'canonical_owner_bounds': [lower, upper], 'frozen_limits_exact': limits,
           'hypotheses': {}, 'passed': False}
    if not (not near_knots and canonical_owner == owner and lower is not None and upper is not None and lower < h < upper):
        row['stop_reason'] = 'Unsupported canonical starting owner; no silent layer reselection.'
        return row
    for label, integrate in [('A_raw_initial_vector', raw_integrate),
                             ('B_normalized_initial_orientation', oracle.integrate)]:
        try:
            coarse = integrate(canonical, False)
            tight = integrate(canonical, True)
        except (AssertionError, ValueError, FloatingPointError) as error:
            row['hypotheses'][label] = {'passed': False, 'unresolved': True,
                                        'error_type': type(error).__name__, 'error': str(error)}
            continue
        encoded = [encode(x, origin, basis, float(state[3])) for x in (coarse, tight)]
        checks = [evaluate(x, ref, origin, basis, limits, absolute) for x in encoded]
        domain = all(state[3]-state[7]+x['geometric_path_m'] <= 2_000_000. for x in (coarse, tight))
        stable = encoded[0] == encoded[1] and checks[0]['passed'] == checks[1]['passed']
        row['hypotheses'][label] = {'coarse': coarse, 'tight': tight,
            'observed_convergence': oracle.differences(coarse, tight),
            'encoded_words_coarse_tight': encoded, 'evaluations': checks,
            'encoding_and_gate_stable': stable, 'original_remaining_path_domain': bool(domain),
            'passed': stable and domain and checks[0]['passed'] and checks[1]['passed']}
    row['passed'] = all(x['passed'] for x in row['hypotheses'].values())
    if not row['passed']:
        row['stop_reason'] = 'At least one fixed hypothesis failed or was inconclusive; no further case or implementation.'
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    parser.add_argument('--packed-field', action='store_true',
                        help='Fixed CPU promotion of the exact uploaded field; same frozen cases and gates.')
    args = parser.parse_args()
    output, partial = Path(str(args.prefix)+'.json'), Path(str(args.prefix)+'.partial.json')
    assert not output.exists() and not partial.exists()
    before = fingerprints()
    raw_integrate, raw_hash = raw_oracle()
    inputs = cases()
    result = {'schema': 'canonical-handoff-feasibility-v1', 'scope': __doc__.strip(),
        'source_and_input_hashes': before, 'raw_oracle_source_sha256': raw_hash,
        'python': sys.version, 'numpy': np.__version__, 'scipy': scipy.__version__,
        'planned_cases': [x[0] for x in inputs], 'records': [],
        'policy': 'Both hypotheses must pass every frozen metric, strict owner, exit/domain and stable coarse/tight f32 encoding gate. Stop on the first failure/inconclusive case. No new convergence allowance or threshold. All-pass is finite canonical feasibility only.',
        'coordinate_scope': 'Same original full-eye basis construction as the frozen reference audits; f64 transforms retained, not asserted exact orthogonal arithmetic. Round local coordinates, never absolute ECEF. Actual rounded uploaded quadric is not substituted.',
        'endpoint_scope': 'Oracle final direction normalization retained for A and B. Total path is exact addition of represented saved-f32 and suffix-f64 numbers, then one f32 rounding. This is not an implemented compensated GPU update.',
        'gpu_execution': False, 'runtime_implementation': False, 'performance_claim': False}
    if args.packed_field:
        import optical_handoff_packed_field as packed
        result.update(schema='uploaded-field-handoff-feasibility-v1',
            scope=packed.__doc__.strip(), packed_field=True,
            raw_oracle_source_sha256=None,
            policy='Both initial-vector hypotheses must pass the unchanged frozen metrics, strict saved packed owner, packed-top exit, remaining path domain and identical coarse/tight f32 encoding. Stop on first failed or inconclusive case. No new allowance.',
            coordinate_scope='Integration uses the exact saved original-eye-local f32 position and direction, promoted to f64. Encoded local outputs map through the existing frozen original-eye basis only for comparison with canonical truth. Uploaded Q skew, origin and independent branch slopes are retained; geometry is ordered scalar f64, not WGSL arithmetic emulation.',
            source_scope='Initial owner is the recorded signed word; no initial near-knot override. Directed packed-height events alone move owner. Each interval retains its uploaded N and slope, including one-sided packed endpoint differences. Recomputed promoted geometry replaces the saved GPU geometry cache explicitly.',
            normalization_scope='A keeps the raw local direction; B normalizes that local direction once initially. Neither renormalizes per stage or per accepted step. Both retain oracle final normalization. A path is the equations\' off-unit parameter, not asserted physical arc length.')
    for item in inputs:
        row = packed.run_case(item, sys.modules[__name__]) if args.packed_field else run_case(item, raw_integrate)
        result['records'].append(row)
        result['unrun_cases'] = [x[0] for x in inputs[len(result['records']):]]
        partial.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n')
        print(json.dumps({'case': row['case'], 'passed': row['passed'],
                          'stop_reason': row.get('stop_reason')}), flush=True)
        if not row['passed']:
            break
    assert fingerprints() == before
    result['inputs_unchanged'] = True
    result['passed'] = not result['unrun_cases'] and all(r['passed'] for r in result['records'])
    result['decision'] = ('Finite canonical hypotheses pass; no packed-field/GPU/scene/speed conclusion.' if result['passed'] else
                          'Stop this conservative canonical feasibility screen. This does not disprove every mixed-precision implementation.')
    if args.packed_field:
        result['decision'] = ('Promoted uploaded-field hypotheses pass this CPU necessary screen; no GPU arithmetic, controller, contact, mixed-precision implementation or speed conclusion.' if result['passed'] else
                              'Stop this promoted uploaded-field screen at the fixed gate; no mixed-precision implementation or further case execution.')
    with output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')


if __name__ == '__main__':
    main()
