"""Bounded, CPU-only first gate for changing the Cartesian reference point.

This evaluates the reviewed scalar f32 enclosure, not a GPU or a complete
contact solver. A fit pass is necessary, never sufficient, for scene use.
The existing numerical tube and quarter-remaining allocation are unchanged.
"""
from fractions import Fraction as Q
import json
from pathlib import Path
import struct
import sys

import numpy as np

from optical_patch_anchor_bounds import make_anchor
from optical_patch_feasibility_inputs import load_actual_cases, input_inventory
from test_optical_cartesian_producer import (fourth_upper, dyadic, midpoint,
    control_radius, up_add, up_mul, up_div)
from test_optical_normal_map_interval import (map_enclosure, exact, div, mul,
    vsub, vexact, vadd, scale)
from test_optical_normal_map_anchored import anchor_enclosure

ROOT = Path(__file__).resolve().parents[1]
PATCH = 'artifacts/optical-immutable-patch-second'
SOURCE = 'artifacts/optical-positive-contact-packet-first'
MAX_DEPTH = 4


def decode64(word):
    return struct.unpack('<d', struct.pack('<Q', int(word)))[0]


def encode64(value):
    return struct.unpack('<Q', struct.pack('<d', float(value)))[0]


def halfwidth(point):
    """Exact L1 halfwidth of the encoded interval endpoints, not an estimator."""
    return sum((Q(hi)-Q(lo))/2 for lo, hi in point)


def fit(case, bounds, interval):
    """Same Hermite certificate as test_optical_cartesian_producer.certify.

    Only endpoint evaluation is supplied by the chosen reference-point map.
    Translation changes neither physical derivatives nor the M4 majorant.
    """
    width, inverse = dyadic(interval)
    fourth, _ = fourth_upper(case, interval)
    square = up_mul(width, width)
    remainder = up_mul(up_mul(fourth, up_mul(square, square)), .0625)
    derivative_remainder = up_div(up_mul(up_mul(up_mul(fourth, square), width), 10.), 3.)
    p0, d0 = bounds((interval[0], interval[0]))
    p3, d2 = bounds((interval[1], interval[1]))
    origin = tuple(map(midpoint, p0))
    p0 = vsub(p0, vexact(origin))
    p3 = vsub(p3, vexact(origin))
    third = div(exact(width), exact(3.))
    controls = [p0, vadd(p0, scale(d0, third)), vsub(p3, scale(d2, third)), p3]
    derivatives = [d0, vsub(vsub(scale(vsub(p3, p0), mul(exact(3.), exact(inverse))), d0), d2), d2]
    radius = up_add(max(map(control_radius, controls)), up_mul(3., remainder))
    lower = max(halfwidth(row) for row in controls)+3*Q(remainder)
    assert Q(radius) >= lower
    return {'radius_m': radius, 'radius_rational_lower_m': str(lower),
            'fit_component_m': remainder, 'derivative_remainder_component_m': derivative_remainder,
            'fourth_factorial_bound_m': fourth,
            'controls': controls, 'derivative_controls': derivatives, 'origin': origin}


def evaluate(record, bounds):
    limit = record['quarter_remaining_limit']
    result = {'quarter_remaining_limit_m': limit, 'endpoint_intervals': [],
              'fit_admitted': False, 'contact_admitted': False}
    floors = []
    for theta in (0., record['theta_end']):
        point, velocity = bounds((theta, theta))
        floor = halfwidth(point)
        floors.append(floor)
        result['endpoint_intervals'].append({'theta': theta, 'position_m': point,
            'derivative_m_per_theta': velocity, 'l1_halfwidth_exact_m': str(floor)})
    result['endpoint_floor_exceeds_allocation'] = max(floors) > Q(limit)
    result['largest_endpoint_halfwidth_m'] = float(max(floors))
    if result['endpoint_floor_exceeds_allocation']:
        result['stop'] = 'Existing endpoint interval already exceeds unchanged fit allocation; subdivision cannot remove it.'
        return result
    if record['population'] != 'accepted-height-capture':
        result['stop'] = 'Final pending retry is a floor diagnostic; no earlier trial or dyadic fit is inferred.'
        return result
    trials = []
    for depth in range(MAX_DEPTH+1):
        pieces = 1 << depth
        maximum = 0.
        passed = True
        for index in range(pieces):
            certificate = fit(record['case'], bounds, (index/pieces, (index+1)/pieces))
            maximum = max(maximum, certificate['radius_m'])
            if certificate['radius_m'] > limit:
                passed = False
                break  # one failed piece rejects this complete partition
        trials.append({'pieces': pieces, 'evaluated_pieces': index+1,
                       'largest_evaluated_radius_m': maximum, 'all_pieces_fit': passed})
        if passed:
            result['fit_admitted'] = True
            result['pieces'] = pieces
            break
    result['partitions'] = trials
    result['stop'] = ('Fit-only gate passed; contact/first-event/returned-state gates remain untested.'
                      if result['fit_admitted'] else 'No complete partition through 16 pieces fits; no GPU work authorized by this result.')
    return result


def assert_static_face(packet, packed_faces, vertices, face_id):
    """Exact selected-face check against the approved immutable packet."""
    if face_id >= packet['faces']:
        return {'scope': 'Dynamic marker/sign source face; immutable terrain packet has no corresponding record.'}
    cursor = 0
    for patch in packet['patches']:
        if cursor <= face_id < cursor+patch['faces']:
            break
        cursor += patch['faces']
    origin = [Q(decode64(x)) for x in patch['origin_bits']]
    rows = [[Q(decode64(x)) for x in row] for row in patch['rows_bits']]
    words = packed_faces[face_id]
    a, b, c = [[Q(float(x)) for x in v] for v in vertices]
    checked = 0
    for axis, row in enumerate(rows):
        targets = [sum(q*(o-x) for q, o, x in zip(row, origin, a)),
                   sum(q*(y-x) for q, x, y in zip(row, a, b)),
                   sum(q*(y-x) for q, x, y in zip(row, a, c))]
        intervals = [(Q(float(words[0, axis]))+Q(float(words[1, axis])), Q(float(words[0, axis]))+Q(float(words[2, axis]))),
                     (Q(float(words[3, axis])), Q(float(words[4, axis]))),
                     (Q(float(words[5, axis])), Q(float(words[6, axis])))]
        for target, (lo, hi) in zip(targets, intervals):
            assert lo <= target <= hi
            checked += 1
    return {'patch': patch['patch'], 'exact_scalar_enclosures_checked': checked}


def main(prefix):
    output = ROOT / (prefix+'.json')
    assert output.parent == ROOT/'artifacts' and not output.exists()
    packet = json.loads((ROOT/(PATCH+'.json')).read_text(encoding='utf-8'))
    seeds = json.loads((ROOT/(PATCH+'-eye-inputs.json')).read_text(encoding='utf-8'))
    source_meta = json.loads((ROOT/(SOURCE+'.json')).read_text(encoding='utf-8'))
    eye_seed = seeds['eyes'][0]
    assert packet['eyes'][0]['geodetic_bits'] == eye_seed['geodetic_bits']
    east_x, east_y, up_z, north_z = eye_seed['raw_east_xy_upz_northz_bits']
    assert source_meta['exact_f64_inputs'] == {
        'a': seeds['equatorial_radius_bits'], 'eccentricity_squared': seeds['eccentricity_bits'],
        'eye_height': eye_seed['geodetic_bits'][2], 'east_x': east_x, 'east_y': east_y,
        'up_z': up_z, 'north_z': north_z}
    columns = packet['eyes'][0]['ideal_columns_interval']
    basis = np.array([[sum(x)/2 for x in c] for c in columns], dtype=np.float64)
    eye = np.array([sum(x)/2 for x in packet['eyes'][0]['ideal_origin_interval']], dtype=np.float64)
    e2 = decode64(seeds['eccentricity_bits'])
    vertices = np.memmap(ROOT/(SOURCE+'-faces.bin'), dtype='<f8', mode='r').reshape(-1, 3, 3)
    packed_faces = np.memmap(ROOT/(PATCH+'-faces.bin'), dtype='<f4', mode='r').reshape(-1, 7, 4)
    assert len(vertices) == source_meta['face_count'] and len(packed_faces) == packet['faces'] == source_meta['terrain_faces']
    origins = np.array([[decode64(x) for x in p['origin_bits']] for p in packet['patches']])
    records = load_actual_cases()
    results = []
    for record in records:
        case = record['case']
        assert case['e_interval'][0] <= e2 <= case['e_interval'][1]
        for c, bound in zip(columns, case['axis_interval']):
            assert bound[0] <= c[2][0] <= c[2][1] <= bound[1]
        original = lambda t: map_enclosure(case, t)
        initial, _ = original((0., 0.))
        approximate_ecef = eye+np.array([sum(x)/2 for x in initial])@basis
        # Selection is only a seed; no correctness claim depends on nearest
        # arithmetic. Exact source vertices and full anchor residual follow.
        delta = vertices[:, 0]-approximate_ecef
        face_id = int(np.argmin(np.einsum('ij,ij->i', delta, delta)))
        source_vertex = tuple(map(float, vertices[face_id, 0]))
        patch_id = int(np.argmin(np.sum((origins-approximate_ecef)**2, axis=1)))
        chart = tuple(map(float, case['start'][1:]))
        height = float(sum(map(Q, case['anchor_split']))+Q(case['start'][0])+
                       sum(map(Q, case['anchor_residual']))/2)
        row = {key: record[key] for key in ('id', 'population', 'ray', 'raw_input', 'raw_word_offset',
            'raw_words', 'theta_end', 'numerical_error', 'tolerance', 'quarter_remaining_limit', 'source_anchor_index', 'owner_candidates')}
        for key in ('trial', 'pixel', 'prefix_path_m', 'owner'):
            if key in record: row[key] = record[key]
        row['unchanged_existing_numerical_error_m'] = record['numerical_error']
        row['selected_source_face'] = face_id
        row['selected_source_vertex_bits'] = list(map(encode64, source_vertex))
        row['selected_source_packet_check'] = assert_static_face(packet, packed_faces, vertices[face_id], face_id)
        row['seed_chart_height_bits'] = list(map(encode64, (*chart, height)))
        row['variants'] = {}
        for name, vertex in [('original-eye', None), ('existing-patch-origin', tuple(origins[patch_id])), ('near-source-vertex', source_vertex)]:
            try:
                if vertex is None:
                    bounds = original
                    anchor_metadata = None
                else:
                    anchor = make_anchor(columns, e2, chart, height, vertex)
                    bounds = lambda t, a=anchor: anchor_enclosure(case, a, t)
                    anchor_metadata = {k: anchor[k] for k in ('a_split', 'b_split', 'h_split', 'residual_interval')}
                result = evaluate(record, bounds)
                result['anchor'] = anchor_metadata
                if vertex is not None:
                    result['reference_vertex_bits'] = list(map(encode64, vertex))
                    result['reference_distance_seed_m_not_certificate'] = float(np.linalg.norm(np.array(vertex)-approximate_ecef))
                if name == 'existing-patch-origin':
                    result['patch_id'] = packet['patches'][patch_id]['patch']
                    result['construction_scope'] = 'Same curve-start chart/height seed with one f32 interval residual to this existing patch origin; no claim about all possible split patch-origin representations.'
                row['variants'][name] = result
            except (ValueError, OverflowError, AssertionError) as error:
                # A failed mathematical prerequisite is recorded, never
                # replaced by a geometric ray or enlarged allowance.
                row['variants'][name] = {'fit_admitted': False, 'contact_admitted': False,
                                         'failure': f'{type(error).__name__}: {error}'}
        results.append(row)
        print(json.dumps({'completed': len(results), 'total': len(records), 'case': record['id'],
                          'near_source_fit': row['variants']['near-source-vertex']['fit_admitted'],
                          'near_source_endpoint_floor_failure': row['variants']['near-source-vertex'].get('endpoint_floor_exceeds_allocation')}), flush=True)
    summary = {}
    for name in ('original-eye', 'existing-patch-origin', 'near-source-vertex'):
        values = [r['variants'][name] for r in results]
        summary[name] = {'accepted_capture_fit_passes': sum(v['fit_admitted'] for v in values),
                        'endpoint_floor_failures_all_57': sum(v.get('endpoint_floor_exceeds_allocation', False) for v in values),
                        'invalid_or_failed_prerequisites': sum('failure' in v for v in values)}
    admitted = summary['near-source-vertex']['accepted_capture_fit_passes']
    report = {'schema': 'patch-relative-fit-screen-v1', 'scope': 'CPU f32 interval transcription, necessary representation/fit gate only. No GPU, complete contact, ODE accuracy, first-owner, full-ray, upload-cost or native-FPS acceptance.',
              'gate': 'Stop before contact work if no accepted real capture curve fits. Any fit pass only permits a separately reviewed contact screen; it authorizes no GPU integration.',
              'decision': 'stop-representation-gate' if not admitted else 'fit-only-some-admission-contact-gate-still-open',
              'complete_feasibility': False, 'contact_admitted': False, 'chronology_admitted': False,
              'accepted_capture_count': 34, 'pending_floor_diagnostic_count': 23,
              'maximum_partition_pieces': 1 << MAX_DEPTH,
              'mandatory_scope_limits': ['Final pending records are uncertainty-floor retries, not earlier crossing history.',
                'Nearby source anchor uses a new host certificate for each retained curve; no claim that runtime metadata preparation is free or reusable.',
                'Patch-origin negative control uses the same curve-start chart/height and a single interval residual; failure does not exclude other split residual representations.',
                'Control vectors remain in ideal-eye metres. Patch orientation and returned-state transformations are not evaluated in this first fit-only gate.',
                'Existing numerical tube remains charged before quarter allocation. Current strict 1% contact-root policy has not been changed or tested here.',
                'A failed interval enclosure is failure of this representation/certificate, not proof of physical ray error.'],
              'summary': summary, 'decoder_input_inventory': input_inventory(), 'cases': results}
    output.write_text(json.dumps(report, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    print(json.dumps({'output': str(output), 'decision': report['decision'], 'summary': summary}))


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: optical_patch_relative_feasibility.py artifacts/<fresh-prefix>')
    main(sys.argv[1])
