"""Independent finite, high-precision replay of a shared discrete RK map.

This audits a representation certificate, not the exact ODE/global error or
scene visibility. Native launch f32 evaluation is deliberately outside scope.
"""
import argparse
from collections import Counter
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import struct

import mpmath as mp


def encoded(value):
    return mp.mpf(struct.unpack('>d', bytes.fromhex(value))[0])


def norm(values):
    return mp.sqrt(mp.fsum(v * v for v in values))


def dot(a, b):
    return mp.fsum(x * y for x, y in zip(a, b, strict=True))


def polynomial(coefficients, x, y, powers):
    return mp.fsum(mp.mpf(c) * x ** a * y ** b
                   for c, (a, b) in zip(coefficients, powers, strict=True))


def review(path, selected=None):
    mp.mp.dps = 90
    path = Path(path)
    data = json.loads(path.read_text())
    assert data['scene_admitted_cells'] == 0 and data['curves_retained']
    bits = data['arithmetic_input_bits']
    view = list(map(encoded, bits['view']))
    initial_h = encoded(bits['initial_height'])
    knots = [tuple(map(encoded, p)) for p in bits['absolute_knots']]
    a = [list(map(encoded, row)) for row in bits['A']]
    c, d = [list(map(encoded, bits[k])) for k in ('C', 'D')]
    eccentricity = encoded(bits['E2'])
    complement = encoded(bits['one_minus_E2'])
    tangent = encoded(bits['tan_half_vertical'])
    horizontal_tangent = encoded(bits['aspect_tan_half_vertical'])
    # The declared ideal parameter is normalized encoded polar axis. The
    # author's Taylor interval includes its evaluation/truncation error.
    source = json.loads(Path(data['source']).read_text())
    raw_axis = source['fixture']['clear_ray_fixture']['encoded_uniform'][0][:2]
    # These are binary32 values, exactly representable after JSON parsing.
    axis = [mp.mpf(0), mp.mpf(raw_axis[0]), -mp.mpf(raw_axis[1])]
    axis = [v / norm(axis) for v in axis]
    for actual, declared in zip(axis, bits['axis_polynomials'], strict=True):
        center = encoded(declared['coefficients'][0])
        remainder = encoded(declared['remainder'])
        assert all(encoded(v) == 0 for v in declared['coefficients'][1:])
        assert abs(actual - center) <= remainder
    powers = data['coefficient_order']
    blocks = [(0, 2), (2, 5), (5, 6), (6, 7)]

    def rhs(h, state, anchor, refractivity0, gradient):
        qa, qb = state[:2]
        scale = 1 / mp.sqrt(1 + qa * qa + qb * qb)
        normal = [qa * scale, scale, -qb * scale]
        direction = state[2:5]
        vertical = dot(direction, normal)
        z = dot(normal, axis)
        discriminant = 1 - eccentricity * z * z
        prime = mp.mpf(6378137) / mp.sqrt(discriminant)
        meridian = prime * complement / discriminant
        tangent_u = [u - vertical * n for u, n in zip(direction, normal)]
        axis_tangent = [k - z * n for k, n in zip(axis, normal)]
        correction = (prime * eccentricity * dot(axis_tangent, direction)
                      / (discriminant * (meridian + h) * (prime + h)))
        dn = [t / (prime + h) + correction * q
              for t, q in zip(tangent_u, axis_tangent)]
        refractivity = refractivity0 + gradient * (h - anchor)
        bend = gradient / (1 + refractivity)
        return [(dn[0] - qa * dn[1]) / (normal[1] * vertical),
                (-dn[2] - qb * dn[1]) / (normal[1] * vertical),
                *[bend * (n / vertical - u) for n, u in zip(normal, direction)],
                1 / vertical, refractivity / vertical]

    def step(h, endpoint, state, owner):
        if owner == 0:
            anchor, n0 = knots[0]
            gradient = mp.mpf(0)
        else:
            anchor, n0 = knots[owner - 1]
            upper, n1 = knots[owner]
            gradient = (n1 - n0) / (upper - anchor)
        height_step = endpoint - h
        stages = []
        for i in range(7):
            trial = [state[j] + height_step * mp.fsum(a[i][k] * stages[k][j]
                                                    for k in range(i)) for j in range(7)]
            stages.append(rhs(h + c[i] * height_step, trial, anchor, n0, gradient))
        delta = [height_step * mp.fsum(a[6][k] * stages[k][j] for k in range(6)) for j in range(7)]
        r3 = [height_step * stages[0][j] - delta[j] for j in range(7)]
        r4 = [2 * delta[j] - height_step * (stages[0][j] + stages[6][j]) for j in range(7)]
        r5 = [height_step * mp.fsum(d[k] * stages[k][j] for k in range(7)) for j in range(7)]
        return [delta, r3, r4, r5]

    def dense(state, r, theta):
        return [state[j] + theta * r[0][j] + theta * (1 - theta)
                * (r[1][j] + theta * (r[2][j] + (1 - theta) * r[3][j])) for j in range(7)]

    budget_count = 0
    admitted = 0
    for case in data['cases']:
        complete = case['transport'] == 'enclosed common-schedule exit'
        admitted += complete
        assert not case.get('scene_admitted', False)
        curves = case.get('curves', case.get('partial_curves', []))
        previous = None
        for curve in curves:
            assert curve['height_end_m'] > curve['height_start_m']
            assert curve['vertical_lower'] > .01 and curve['raw_direction_norm_lower'] > .25
            assert 0 < curve['path_upper_m'] <= 100000
            for key in ('incoming_group_error', 'dense_group_error', 'normalized_endpoint_group_error'):
                assert len(curve[key]) == 4 and all(mp.isfinite(v) and v >= 0 for v in curve[key])
            exact_sum = Fraction(curve['ode_metric_m']) + Fraction(curve['representation_metric_m'])
            assert exact_sum <= Fraction(curve['combined_metric_m']) <= Fraction(curve['tolerance_m'])
            if previous is not None:
                assert previous['height_end_m'] == curve['height_start_m']
                assert previous['normalized_endpoint_group_error'] == curve['incoming_group_error']
                assert curve['active_upper_node'] == previous['active_upper_node'] + int(previous['source_boundary_end'])
            owner = curve['active_upper_node']
            if curve['source_boundary_end']:
                assert mp.mpf(curve['height_end_m']) == knots[owner][0]
            else:
                assert mp.mpf(curve['height_end_m']) < knots[owner][0]
            previous = curve
            budget_count += 1
        if complete:
            assert len(curves) == case['accepted'] and mp.mpf(curves[-1]['height_end_m']) == knots[-1][0]
    assert admitted == data['transport_enclosed_cells']
    maximum_ratio = [mp.mpf(0)] * 4
    maximum_error = [mp.mpf(0)] * 4
    replayed_curves = 0
    evaluated_states = 0
    outputs = []
    selected = selected if selected is not None else [i for i, case in enumerate(data['cases'])
                                                     if case.get('curves') or case.get('partial_curves')]
    for case_index in selected:
        case = data['cases'][case_index]
        curves = case.get('curves', case.get('partial_curves', []))
        assert curves
        x0, y0, width, height = case['cell']
        for angular_x, angular_y in [(-1, -1), (0, 0), (1, 1)]:
            pixel_x = mp.mpf(x0) + mp.mpf(width) / 2 + mp.mpf(width - 1) * angular_x / 2
            pixel_y = mp.mpf(y0) + mp.mpf(height) / 2 + mp.mpf(height - 1) * angular_y / 2
            sx = (2 * pixel_x / view[16] - 1) * horizontal_tangent
            sy = (1 - 2 * pixel_y / view[17]) * tangent
            launch = [view[j] * sx + view[4 + j] * sy + view[8 + j] for j in range(3)]
            launch = [u / norm(launch) for u in launch]
            state = [mp.mpf(0), mp.mpf(0), *launch, mp.mpf(0), mp.mpf(0)]
            for curve in curves:
                h, endpoint = map(mp.mpf, [curve['height_start_m'], curve['height_end_m']])
                stored_start = [polynomial(p, angular_x, angular_y, powers) for p in curve['start_coefficients']]
                stored_r = [[polynomial(p, angular_x, angular_y, powers) for p in row]
                            for row in curve['dense_coefficients']]
                for group, (lo, hi) in enumerate(blocks):
                    error = norm([state[j] - stored_start[j] for j in range(lo, hi)])
                    assert error <= mp.mpf(curve['incoming_group_error'][group]), (case_index, 'start', group, error)
                actual_r = step(h, endpoint, state, curve['active_upper_node'])
                for theta in [mp.mpf(1) / 4, mp.mpf(1) / 2, mp.mpf(3) / 4, mp.mpf(1)]:
                    actual = dense(state, actual_r, theta)
                    represented = dense(stored_start, stored_r, theta)
                    for group, (lo, hi) in enumerate(blocks):
                        error = norm([actual[j] - represented[j] for j in range(lo, hi)])
                        bound = mp.mpf(curve['dense_group_error'][group])
                        assert error <= bound, (case_index, curve['height_start_m'], group, theta, error, bound)
                        maximum_error[group] = max(maximum_error[group], error)
                        maximum_ratio[group] = max(maximum_ratio[group], error / bound if bound else mp.mpf(0))
                    evaluated_states += 1
                state = dense(state, actual_r, mp.mpf(1))
                unit = norm(state[2:5])
                state[2:5] = [u / unit for u in state[2:5]]
                replayed_curves += 1
            if case['transport'] == 'enclosed common-schedule exit':
                stored_direction = [polynomial(p['coefficients'], angular_x, angular_y, powers)
                                    for p in case['exit_direction_polynomials']]
                assert norm([state[j + 2] - stored_direction[j] for j in range(3)]) <= mp.mpf(case['exit_group_error'][1])
            outputs.append({'case_index': case_index, 'cell': case['cell'], 'angular_point': [angular_x, angular_y],
                            'complete_exit': case['transport'] == 'enclosed common-schedule exit',
                            'accepted_curves_replayed': len(curves), 'last_height_m': curves[-1]['height_end_m']})
    return {'schema': 'ancientearth.shared-transport.independent-review.v1',
            'scope': 'Finite high-precision replay of the same stored-coefficient discrete RK maps at three launch-cell points and four interior fractions per accepted step. All retained local budgets/ownership/continuity are audited. This is not a global exact-ODE accuracy proof, complete angular-cell sampling, GPU certificate, performance result or scene/sky admission.',
            'input': str(path), 'input_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'precision_decimal_digits': mp.mp.dps, 'cells_tested': data['cells_tested'],
            'transport_enclosed_cells': admitted, 'scene_admitted_cells': 0,
            'reason_counts': dict(Counter(c.get('reason', 'complete') for c in data['cases'])),
            'accepted_curve_budgets_audited': budget_count, 'direct_curve_replays': replayed_curves,
            'dense_states_checked': evaluated_states, 'maximum_group_error': list(map(float, maximum_error)),
            'maximum_fraction_of_declared_group_bound': list(map(float, maximum_ratio)), 'replays': outputs}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('input')
    parser.add_argument('--output', required=True)
    parser.add_argument('--cases', type=int, nargs='+')
    args = parser.parse_args()
    assert not Path(args.output).exists(), 'retain previous evidence'
    result = review(args.input, args.cases)
    Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'replays'}, indent=2))
