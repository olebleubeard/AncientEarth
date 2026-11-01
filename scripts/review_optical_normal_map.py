"""Audit canonical map GPU intervals against independently differentiated MP math.

Read actual uploaded f32 words and exact canonical f64 metadata bits. The
reference uses direct full Cartesian positions at 90 decimal digits, with an
expanded power polynomial and numerical differentiation by mpmath. No shader
stable-map or derivative identity is used as the reference.
"""
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

import mpmath as mp


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def decoded_f64(bits):
    return struct.unpack('<d', struct.pack('<Q', bits))[0]


def contains(bounds, truth):
    return mp.mpf(bounds[0]) <= truth <= mp.mpf(bounds[1])


def direct_map(axis, eccentricity, anchor, eye, start, coefficients, theta):
    coordinates = []
    for component in range(3):
        c2, c3, c4, c5 = [mp.mpf(c[component]) for c in coefficients]
        coordinates.append(mp.mpf(start[component]) + (c2+c3)*theta
                           + (c4-c3+c5)*theta**2 + (-c4-2*c5)*theta**3 + c5*theta**4)
    raw = (coordinates[1], mp.mpf(1), -coordinates[2])
    magnitude = mp.sqrt(sum(x*x for x in raw))
    normal = [x/magnitude for x in raw]

    def absolute(n, h):
        projection = sum(x*y for x, y in zip(n, axis))
        radius = 6378137/mp.sqrt(1-eccentricity*projection**2)
        return [radius*(x-eccentricity*projection*k)+h*x for x, k in zip(n, axis)]

    position = absolute(normal, anchor+coordinates[0])
    origin = absolute((mp.mpf(0), mp.mpf(1), mp.mpf(0)), eye)
    return [x-y for x, y in zip(position, origin)]


def audit(prefix):
    report_path = Path(str(prefix)+'.json')
    report = read_json(report_path)
    fixture = report['fixture']
    assert report['canonical_map'] is True and report['record_words'] == 24
    count = report['count']
    assert count == len(fixture['cases']) == 24
    assert fixture['input_vectors_per_case'] == 10
    assert fixture['output_words_per_case'] == 24
    uploaded = Path(str(prefix)+'-rays.bin').read_bytes()
    raw = Path(str(prefix)+'-readback.bin').read_bytes()
    uniform = Path(str(prefix)+'-uniform.bin').read_bytes()
    assert len(uploaded) == count*160
    assert len(raw) == report['output_bytes'] == count*96
    assert struct.pack('<2f', *fixture['encoded_eye_height']) == uniform[8:16]
    source = read_json(Path(str(prefix)+'-sources.json'))
    assert source['compiler_inputs_unchanged'] is True and source['canonical_map'] is True
    stability = read_json(Path(str(prefix)+'-input-stability.json'))
    assert stability['inputs_unchanged_through_run'] is True
    for item in read_json(Path(str(prefix)+'-outputs.json')):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256']

    cases = []
    failures = []
    maxima = {}

    def track(key, number, index):
        if key not in maxima or number > maxima[key]['value']:
            maxima[key] = {'value': float(number), 'case': index}

    with mp.workdps(90):
        truth_bits = fixture['canonical_f64_bits']
        axis_raw = [mp.mpf(decoded_f64(x)) for x in truth_bits['axis_before_normalization']]
        norm = mp.sqrt(sum(x*x for x in axis_raw))
        axis = [x/norm for x in axis_raw]
        eccentricity = mp.mpf(decoded_f64(truth_bits['eccentricity']))
        eye = mp.mpf(decoded_f64(truth_bits['eye_height_m']))
        eye_pair = struct.unpack_from('<2f', uniform, 8)
        eye_error = eye-sum(map(mp.mpf, eye_pair))
        assert contains(fixture['eye_residual'], eye_error)
        assert contains(fixture['eccentricity_interval'], eccentricity)
        for i in range(3):
            assert contains((fixture['axis_lower'][i], fixture['axis_upper'][i]), axis[i])

        for index, declared in enumerate(fixture['cases']):
            input_bytes = uploaded[index*160:(index+1)*160]
            encoded = struct.unpack('<40f', input_bytes)
            expected = [*declared['encoded_anchor'], 0., 0.,
                        *fixture['axis_lower'], 0., *fixture['axis_upper'], 0.,
                        *fixture['eccentricity_interval'], *fixture['eye_residual'],
                        *declared['anchor_residual'], declared['theta'], 0.,
                        *declared['start_q']]
            expected += [x for c in declared['coefficients'] for x in c]
            assert struct.pack('<40f', *expected) == input_bytes
            start = encoded[20:23]
            coefficients = [encoded[i:i+3] for i in [24, 28, 32, 36]]
            theta = mp.mpf(encoded[18])
            anchor = mp.mpf(decoded_f64(declared['anchor_canonical_bits']))
            anchor_error = anchor-mp.mpf(encoded[0])-mp.mpf(encoded[1])
            assert contains(encoded[16:18], anchor_error)
            words = struct.unpack_from('<24I', raw, index*96)
            output = struct.unpack_from('<24f', raw, index*96)
            assert list(words) == report['gpu_raw_records'][index]
            assert all(words[i] == 0 for i in [7, 11, 15, 19, 22, 23])
            assert all(math.isfinite(x) for i, x in enumerate(output) if i != 3)
            record = {'index': index, 'valid': words[3], 'raw_words': list(words),
                      'anchor_canonical_m': float(anchor), 'theta': float(theta)}
            if words[3] != 1:
                failures.append({'index': index, 'reason': 'map returned invalid'})
                cases.append(record)
                continue

            position = direct_map(axis, eccentricity, anchor, eye, start, coefficients, theta)
            velocity = [mp.diff(lambda t: direct_map(axis, eccentricity, anchor, eye,
                                                     start, coefficients, t)[i], theta)
                        for i in range(3)]
            p_bounds = [(output[i], output[4+i]) for i in range(3)]
            v_bounds = [(output[8+i], output[12+i]) for i in range(3)]
            p_ok = all(contains(b, x) for b, x in zip(p_bounds, position))
            v_ok = all(contains(b, x) for b, x in zip(v_bounds, velocity))
            ordinary = list(map(mp.mpf, output[16:19]))
            error = mp.sqrt(sum((p-c)**2 for p, c in zip(position, ordinary)))
            radius_ok = output[20] == 0. and mp.mpf(output[21]) >= error
            if not p_ok or not v_ok or not radius_ok:
                failures.append({'index': index, 'position_enclosed': p_ok,
                                 'velocity_enclosed': v_ok, 'radius_enclosed': radius_ok})
            track('ordinary_position_error_m', error, index)
            track('computed_point_radius_upper_m', output[21], index)
            track('maximum_position_component_width_m', max(b-a for a, b in p_bounds), index)
            track('maximum_velocity_component_width_m_per_theta', max(b-a for a, b in v_bounds), index)
            record.update(position_enclosed=p_ok, velocity_enclosed=v_ok,
                          radius_enclosed=radius_ok,
                          canonical_position_m=[mp.nstr(x, 35) for x in position],
                          canonical_velocity_m_per_theta=[mp.nstr(x, 35) for x in velocity],
                          position_intervals_m=p_bounds, velocity_intervals_m_per_theta=v_bounds,
                          ordinary_position_error_m=float(error),
                          computed_point_radius_upper_m=output[21])
            cases.append(record)

    return {'scope': '24 actual GPU point/derivative interval queries. Exact canonical f64 bits and uploaded f32 words are independently checked against a direct 90-digit Cartesian map and differentiation. This validates these finite enclosures, not integration/global ray error, first contacts, scene transforms, performance or runtime adoption.',
            'input_report_sha256': hashlib.sha256(report_path.read_bytes()).hexdigest(),
            'raw_sha256': hashlib.sha256(raw).hexdigest(),
            'all_output_hashes_verified': True, 'inputs_unchanged_through_run': True,
            'count': count, 'all_queries_enclosed': not failures,
            'failed_queries': failures, 'maxima': maxima, 'cases': cases}


if __name__ == '__main__':
    prefix = Path(sys.argv[1] if len(sys.argv) > 1 else 'artifacts/optical-normal-map-first')
    result = audit(prefix)
    Path(str(prefix)+'-independent-review.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'cases'}, indent=2))
    raise SystemExit(0 if result['all_queries_enclosed'] else 1)
