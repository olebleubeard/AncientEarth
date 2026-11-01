"""Exact-rational checks of prepared coverage packets; not GPU execution."""
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path


def squared(values):
    return sum((v * v for v in values), Q())


def sufficient(patch, query):
    p, v, c = ([Q(x) for x in source] for source in
               [query['point'], query['direction'], patch['center']])
    pe, de, ve = (Q(query[key]) for key in ['point_error_m', 'direction_error', 'vertex_error_m'])
    e = abs(v[0]) + abs(v[1] - 1) + abs(v[2]) + de
    if e >= 1:
        return False, None
    delta = 2 * e / (1 - e)
    erosion = delta * (Q(patch['boundary_radius']) + sum(abs(a - b) for a, b in zip(p, c))) + pe + ve
    margins = [Q(half) - abs(p[index] - c[index]) - erosion
               for half, index in zip(patch['half_extent_xz'], [0, 2])]
    if min(margins) <= 0 or v[1] - de <= 0:
        return False, float(min(margins))
    lo, hi = (Q(x) for x in patch['vertical_bounds_y'])
    quotients = [n / d for n in [lo - ve - p[1] - pe, hi + ve - p[1] + pe]
                 for d in [v[1] - de, v[1] + de]]
    lower, upper = (Q(x) for x in query['parameter_range_m'])
    return lower <= min(quotients) and max(quotients) <= upper, float(min(margins))


def review(path):
    data = json.loads(path.read_text())
    rows = []
    for patch in data['patches']:
        original = [Q(x) for x in patch['original_center_xyz_up']]
        packed = [Q(patch['center'][i]) for i in [0, 2, 1]]
        delta = [a - b for a, b in zip(packed, original)]
        for i in range(2):
            assert Q(patch['half_extent_xz'][i]) + abs(delta[i]) <= Q(patch['original_half_extent'][i])
        radius_increase = Q(patch['boundary_radius']) - Q(patch['original_boundary_radius'])
        assert radius_increase >= 0 and radius_increase ** 2 >= squared(delta)
        assert Q(patch['vertical_bounds_y'][0]) <= Q(patch['original_vertical_bounds_z'][0])
        assert Q(patch['vertical_bounds_y'][1]) >= Q(patch['original_vertical_bounds_z'][1])
        assert patch['degree'] != 0
        for query in patch['queries']:
            for name, bound_name, allowance in [
                    ('point', 'point_error_m', query['f64_transform_allowance_m']),
                    ('direction', 'direction_error', 64 * 2 ** -52)]:
                error = [Q(a) - Q(b) for a, b in zip(query[name], query['exact_' + name + '_before_f32'])]
                remaining = Q(query[bound_name]) - Q(allowance)
                assert remaining >= 0 and remaining ** 2 >= squared(error)
            assert Q(query['vertex_error_m']) >= Q(patch['source_position_error_m']) + Q(query['f64_transform_allowance_m'])
            certified, margin = sufficient(patch, query)
            if certified:
                assert query['canonical_normal_line_hit_m'] is not None
                hit_t = Q(101000) - Q(query['canonical_normal_line_hit_m'])
                assert Q(query['parameter_range_m'][0]) <= hit_t <= Q(query['parameter_range_m'][1])
            if query['name'] == 'external_vertex':
                assert not certified
            rows.append({'patch': patch['patch'], 'name': query['name'],
                         'exact_rational_sufficient_predicate': certified,
                         'minimum_eroded_margin_m': margin})
    return {'scope': 'Prepared center/rectangle/radius/height containment and explicit input error bounds checked with exact rational arithmetic. Sufficient predicate evaluated on represented inputs in exact arithmetic. This is not execution or certification of the WGSL interval implementation or a universal runtime transform-error bound.',
            'input_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'patches': len(data['patches']), 'queries': len(rows),
            'certified': sum(row['exact_rational_sufficient_predicate'] for row in rows),
            'external_vertices_deferred': sum(row['name'] == 'external_vertex' for row in rows),
            'cases': rows}


if __name__ == '__main__':
    path = Path('artifacts/optical-coverage-core-packet-reviewed.json')
    result = review(path)
    output = path.with_name(path.stem + '-independent-review.json')
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'cases'}, indent=2))
