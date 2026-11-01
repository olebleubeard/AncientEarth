"""Independent equatorial reference to the first downward h0 contact.

Uses the WGS84 equatorial circle and physical Cartesian ray equations, not
the GPU quadric inverse, midpoint stages, suffix stages or old fixed-path stop.
Directed source events switch the one-sided gradient. A separate nonterminal
radial-velocity event records an outward-to-inward turn. DOP853 convergence and
Bouguer drift are finite numerical evidence, not an exact trajectory tube.
"""
import argparse
import json
import math
from pathlib import Path

import numpy as np
from scipy.integrate import solve_ivp

A = 6378137.
ROOT = Path(__file__).resolve().parents[1]
NAME = 'outward turn and return to initial height'


def reference(case, tighter):
    origin = np.asarray(case['origin_ecef_m'], dtype=float)
    launch = np.asarray(case['initial_direction'], dtype=float)
    assert origin.shape == launch.shape == (3,)
    assert origin[2] == launch[2] == 0 and np.isfinite(origin).all() and np.isfinite(launch).all()
    launch_norm = float(np.linalg.norm(launch))
    assert math.isfinite(launch_norm) and launch_norm > 0
    launch /= launch_norm
    origin = origin[:2]
    state = np.r_[np.zeros(2), launch[:2], 0.]
    base = case.get('profile_base_height_m', 0.)
    assert math.isfinite(base)
    heights = np.asarray([base+x['height_m'] for x in case['layers']])
    values = np.asarray([x['refractivity'] for x in case['layers']])
    assert len(heights) >= 2 and np.isfinite(heights).all()
    assert heights[0] == 0 and np.all(np.diff(heights) > 0)
    assert np.all(np.isfinite(values)) and np.all(values > -1)

    def physical(value):
        p = origin+value[:2]
        radius = np.linalg.norm(p)
        normal = p/radius
        return radius-A, normal, float(value[2:4]@normal)

    initial_h, _, initial_v = physical(state)
    assert 0 < initial_h < heights[-1]
    owner = int(np.searchsorted(heights, initial_h, side='right'))-1
    # This fixture's exact equatorial source-node initialization is explicit.
    exact_nodes = np.flatnonzero(heights == initial_h)
    if len(exact_nodes):
        owner = int(exact_nodes[0])-(initial_v < 0)
    assert owner >= 0
    initial_n = 1.+np.interp(initial_h, heights, values)
    invariant = initial_n*(origin[0]*launch[1]-origin[1]*launch[0])
    max_invariant_error, max_norm_error = 0., 0.
    path, calls, steps = 0., 0, 0
    contacts, turns, verticals = [], [], [initial_v]
    terminal = None
    maximum_step = 500. if tighter else 2000.
    atol = ([2e-9]*2+[5e-16]*2+[1e-11]) if tighter else ([2e-7]*2+[2e-14]*2+[1e-9])
    for _ in range(32):
        assert 0 <= owner < len(heights)-1
        lower, upper = heights[owner:owner+2]
        n0 = values[owner]
        slope = (values[owner+1]-n0)/(upper-lower)

        def rhs(_, value):
            h, normal, radial = physical(value)
            excess = n0+slope*(h-lower)
            return np.r_[value[2:4], (normal-value[2:4]*radial)*(slope/(1.+excess)), excess]

        def rise(_, value):
            return physical(value)[0]-upper

        def fall(_, value):
            return physical(value)[0]-lower

        def turn(_, value):
            return physical(value)[2]

        rise.direction, fall.direction, turn.direction = 1, -1, -1
        rise.terminal, fall.terminal, turn.terminal = True, True, False
        solution = solve_ivp(rhs, (path, 2_000_000.), state, method='DOP853',
                             rtol=5e-14 if tighter else 2e-12, atol=atol,
                             max_step=maximum_step, events=(rise, fall, turn))
        assert solution.success and solution.t[-1] > path
        calls += solution.nfev
        steps += len(solution.t)-1
        for value in solution.y.T:
            h, _, radial = physical(value)
            verticals.append(radial)
            p = origin+value[:2]
            conserved = (1.+n0+slope*(h-lower))*(p[0]*value[3]-p[1]*value[2])
            max_invariant_error = max(max_invariant_error, abs(conserved-invariant))
            max_norm_error = max(max_norm_error, abs(np.linalg.norm(value[2:4])-1.))
        for t, value in zip(solution.t_events[2], solution.y_events[2]):
            h, _, radial = physical(value)
            turns.append({'path_m': float(t), 'height_m': h, 'radial_velocity': radial, 'source_owner': owner})
        path, state = float(solution.t[-1]), solution.y[:, -1]
        h, normal, radial = physical(state)
        assert solution.status == 1, 'Reference exhausted its finite2Mm domain'
        if len(solution.t_events[1]):
            kind = 'first-downward-h0' if lower == 0 else 'downward-source'
            contacts.append({'kind': kind, 'path_m': path, 'height_m': h,
                             'radial_velocity': radial, 'source_owner_before': owner})
            if lower == 0:
                terminal = kind
                break
            owner -= 1
        else:
            assert len(solution.t_events[0])
            kind = 'profile-top' if owner+1 == len(heights)-1 else 'upward-source'
            contacts.append({'kind': kind, 'path_m': path, 'height_m': h,
                             'radial_velocity': radial, 'source_owner_before': owner})
            if kind == 'profile-top':
                terminal = kind
                break
            owner += 1
    assert terminal is not None
    direction = state[2:4]/np.linalg.norm(state[2:4])
    return {'terminal_kind': terminal, 'endpoint_ecef_m': [*(origin+state[:2]), 0.],
            'exit_direction': [*direction, 0.], 'geometric_path_m': path,
            'optical_path_m': path+float(state[4]), 'source_contacts': contacts, 'radial_turns': turns,
            'observed_outward_and_inward': min(verticals) < 0 < max(verticals),
            'minimum_radial_velocity': min(verticals), 'maximum_radial_velocity': max(verticals),
            'bouguer_initial_m': invariant, 'maximum_bouguer_drift_m': max_invariant_error,
            'maximum_direction_norm_drift': max_norm_error, 'function_calls': calls,
            'accepted_steps': steps, 'maximum_step_m': maximum_step,
            'relative_tolerance': 5e-14 if tighter else 2e-12, 'absolute_tolerances': atol}


def tests():
    profile = [{'height_m': 0., 'refractivity': 0.}, {'height_m': 120000., 'refractivity': 0.}]
    for vertical in (-1., -.5):
        launch = np.array([vertical, math.sqrt(1-vertical*vertical), 0.])
        case = {'origin_ecef_m': [A+2000., 0., 0.], 'initial_direction': launch.tolist(), 'layers': profile}
        value = reference(case, True)
        r = A+2000.
        b, c = r*vertical, 2000.*(2*A+2000.)
        exact_path = c/(-b+math.sqrt(b*b-c))
        assert value['terminal_kind'] == 'first-downward-h0' and not value['radial_turns']
        assert abs(value['geometric_path_m']-exact_path) < 5e-8
        assert np.linalg.norm(np.asarray(value['endpoint_ecef_m'])-(np.asarray(case['origin_ecef_m'])+launch*exact_path)) < 5e-8
    retained = json.loads((ROOT/'artifacts/optical-bouguer-reference.json').read_text())
    case = next(c for c in retained['cases'] if c['name'] == NAME)
    result = reference(case, True)
    assert result['terminal_kind'] == 'first-downward-h0'
    assert result['observed_outward_and_inward'] and len(result['radial_turns']) == 1
    returning = next(c for c in result['source_contacts'] if c['kind'] == 'downward-source' and c['source_owner_before'] == 1)
    assert abs(returning['path_m']-case['target_path_m']) < .0005
    assert result['radial_turns'][0]['path_m'] < returning['path_m'] < result['geometric_path_m']
    assert result['maximum_bouguer_drift_m']/abs(result['bouguer_initial_m']) < 1e-12
    print(json.dumps({'tests': '2vacuum first-root checks and retained Bouguer return/source-event check passed',
                      'nominal_returning_reference': result}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    assert args.self_test, 'Use reviewed reference(case,tighter) with actual baseline launch inputs.'
    tests()
