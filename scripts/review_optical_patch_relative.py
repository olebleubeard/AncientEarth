"""Bounded audit of the retained 57-case representation screen, not contacts.

Exact input/floor arithmetic and direct high-precision geocentric references
are independent checks. Partition replay deliberately reuses the separately
reviewed scalar certificate; it is reproducibility evidence, not a new proof.
"""
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

import mpmath as mp
import numpy as np

from optical_patch_feasibility_inputs import load_actual_cases, input_inventory
from optical_patch_anchor_bounds import make_anchor
from optical_patch_relative_feasibility import fit
from test_optical_normal_map_interval import map_enclosure
from test_optical_normal_map_anchored import anchor_enclosure

ROOT = Path(__file__).resolve().parents[1]
NAMES = ('original-eye', 'existing-patch-origin', 'near-source-vertex')


def read(path):
    return json.loads((ROOT / path).read_text(encoding='utf-8-sig'))


def digest(path):
    with (ROOT / path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def f64(word):
    return struct.unpack('<d', struct.pack('<Q', int(word)))[0]


def bits(x):
    return struct.unpack('<Q', struct.pack('<d', float(x)))[0]


def scalar(x):
    x = Q(x)
    return mp.mpf(x.numerator) / x.denominator


def direct_frame(seeds):
    ex, ey, uz, nz = map(f64, seeds['eyes'][0]['raw_east_xy_upz_northz_bits'])
    longitude = mp.atan2(-mp.mpf(ex), mp.mpf(ey))
    latitude = mp.atan2(mp.mpf(uz), mp.mpf(nz))
    sl, cl = mp.sin(longitude), mp.cos(longitude)
    sp, cp = mp.sin(latitude), mp.cos(latitude)
    return [[-sl, cl, mp.mpf(0)], [cp*cl, cp*sl, sp], [sp*cl, sp*sl, -cp]]


def direct_point(frame, e2, chart, height):
    """Direct geocentric construction, independent of stable delta identities."""
    v = [scalar(chart[0]), mp.mpf(1), -scalar(chart[1])]
    length = mp.sqrt(sum(x*x for x in v))
    n = [sum(frame[j][i]*v[j]/length for j in range(3)) for i in range(3)]
    radius = mp.mpf(6378137)/mp.sqrt(1-e2*n[2]**2)
    p = [(radius+height)*n[0], (radius+height)*n[1], (radius*(1-e2)+height)*n[2]]
    return [sum(c[i]*p[i] for i in range(3)) for c in frame]


def direct_curve(frame, e2, case, theta, anchor_height):
    """Expanded polynomial and explicit differentiated geocentric equations."""
    t = scalar(theta)
    q, dq = [], []
    for j in range(3):
        a, b, c, d = [scalar(row[j]) for row in case['coeff']]
        q.append(scalar(case['start'][j])+(a+b)*t+(c-b+d)*t*t+(-c-2*d)*t**3+d*t**4)
        dq.append(a+b+2*(c-b+d)*t+3*(-c-2*d)*t*t+4*d*t**3)
    v, dv = [q[1], mp.mpf(1), -q[2]], [dq[1], mp.mpf(0), -dq[2]]
    length = mp.sqrt(sum(x*x for x in v))
    length_prime = sum(x*y for x, y in zip(v, dv))/length
    nl = [x/length for x in v]
    dl = [y/length-x*length_prime/length**2 for x, y in zip(v, dv)]
    n = [sum(frame[j][i]*nl[j] for j in range(3)) for i in range(3)]
    dn = [sum(frame[j][i]*dl[j] for j in range(3)) for i in range(3)]
    radius = mp.mpf(6378137)/mp.sqrt(1-e2*n[2]**2)
    dr = mp.mpf(6378137)*e2*n[2]*dn[2]/(1-e2*n[2]**2)**mp.mpf('1.5')
    h, dh = anchor_height+q[0], dq[0]
    p = [(radius+h)*n[0], (radius+h)*n[1], (radius*(1-e2)+h)*n[2]]
    dp = [(dr+dh)*n[0]+(radius+h)*dn[0], (dr+dh)*n[1]+(radius+h)*dn[1],
          ((1-e2)*dr+dh)*n[2]+((1-e2)*radius+h)*dn[2]]
    return [[sum(c[i]*x[i] for i in range(3)) for c in frame] for x in (p, dp)]


def contained(bounds, values):
    assert len(bounds) == len(values)
    for (lo, hi), value in zip(bounds, values):
        assert math.isfinite(lo) and math.isfinite(hi) and lo <= hi
        assert mp.mpf(lo) <= value <= mp.mpf(hi), (lo, str(value), hi)


def review(prefix):
    report = read(prefix+'.json')
    sources, stability = read(prefix+'-sources.json'), read(prefix+'-input-stability.json')
    outputs, lifecycle = read(prefix+'-outputs.json'), read(prefix+'-lifecycle.json')
    assert stability['inputs_unchanged_through_run'] and sources['files'] == stability['after_run']
    assert lifecycle['exit_code'] == 0 and not lifecycle['timed_out'] and not lifecycle['terminated_by_wrapper']
    assert lifecycle['failure'] is None
    for item in sources['files']+outputs:
        assert digest(item['path']) == item['sha256'], item['path']
    assert report['decoder_input_inventory'] == input_inventory()
    assert not any(report[k] for k in ('complete_feasibility', 'contact_admitted', 'chronology_admitted'))
    actual = load_actual_cases()
    assert len(actual) == len(report['cases']) == 57
    packet = read('artifacts/optical-immutable-patch-second.json')
    seeds = read('artifacts/optical-immutable-patch-second-eye-inputs.json')
    source = read('artifacts/optical-positive-contact-packet-first.json')
    faces = np.memmap(ROOT/'artifacts/optical-positive-contact-packet-first-faces.bin', dtype='<f8', mode='r').reshape(-1, 3, 3)
    assert len(faces) == source['face_count']
    patches = {p['patch']: p for p in packet['patches']}
    columns = packet['eyes'][0]['ideal_columns_interval']
    counts = dict(anchor_residual_vectors=0, split_scalar_checks=0, endpoint_oracle_vectors=0,
                  endpoint_replays=0, partition_piece_replays=0, raw_words=0)
    summaries = {population: {} for population in ('accepted-height-capture', 'final-selected-pending')}
    details = []
    with mp.workdps(100):
        frame = direct_frame(seeds)
        e2 = mp.mpf(f64(seeds['eccentricity_bits']))
        contained([x for c in columns for x in c], [x for c in frame for x in c])
        eye = direct_point(frame, e2, (0, 0), scalar(f64(seeds['eyes'][0]['geodetic_bits'][2])))
        for row, record in zip(report['cases'], actual):
            for key in ('id', 'population', 'ray', 'raw_input', 'raw_word_offset', 'raw_words', 'theta_end',
                        'numerical_error', 'tolerance', 'quarter_remaining_limit', 'source_anchor_index', 'owner_candidates'):
                assert row[key] == record[key], (row['id'], key)
            raw = np.memmap(ROOT/row['raw_input'], dtype='<u4', mode='r')
            offset = row['raw_word_offset']
            assert raw[offset:offset+len(row['raw_words'])].tolist() == row['raw_words']
            counts['raw_words'] += len(row['raw_words'])
            assert Q(row['quarter_remaining_limit']) <= (Q(row['tolerance'])-Q(row['numerical_error']))/4
            case, limit = record['case'], Q(record['quarter_remaining_limit'])
            chart = tuple(map(float, case['start'][1:]))
            height = float(sum(map(Q, case['anchor_split']))+Q(case['start'][0])+sum(map(Q, case['anchor_residual']))/2)
            assert row['seed_chart_height_bits'] == list(map(bits, (*chart, height)))
            source_vertex = list(map(float, faces[row['selected_source_face'], 0]))
            assert row['selected_source_vertex_bits'] == list(map(bits, source_vertex))
            item = {'id': row['id'], 'population': row['population'], 'variants': {}}
            for name in NAMES:
                variant = row['variants'][name]
                assert 'failure' not in variant and not variant['contact_admitted']
                assert variant['quarter_remaining_limit_m'] == float(limit)
                if name == 'original-eye':
                    assert variant['anchor'] is None
                    origin = eye
                    bounds = lambda t: map_enclosure(case, t)
                else:
                    vertex = list(map(f64, variant['reference_vertex_bits']))
                    expected_vertex = (source_vertex if name == 'near-source-vertex' else
                                       list(map(f64, patches[variant['patch_id']]['origin_bits'])))
                    assert vertex == expected_vertex
                    anchor = make_anchor(columns, float(e2), chart, height, vertex)
                    assert json.loads(json.dumps({k: anchor[k] for k in variant['anchor']})) == variant['anchor']
                    origin = [sum(c[i]*mp.mpf(vertex[i]) for i in range(3)) for c in frame]
                    target = direct_point(frame, e2, chart, scalar(height))
                    contained(variant['anchor']['residual_interval'], [x-y for x, y in zip(target, origin)])
                    counts['anchor_residual_vectors'] += 1
                    for key, value in zip(('a_split', 'b_split', 'h_split'), (*chart, height)):
                        (hi, lo), (lower, upper) = variant['anchor'][key]
                        assert Q(hi)+Q(lo)+Q(lower) <= Q(value) <= Q(hi)+Q(lo)+Q(upper)
                        counts['split_scalar_checks'] += 1
                    bounds = lambda t, a=anchor: anchor_enclosure(case, a, t)
                floors = []
                for endpoint, theta in zip(variant['endpoint_intervals'], (0., record['theta_end'])):
                    assert endpoint['theta'] == theta
                    point, velocity = bounds((theta, theta))
                    assert [list(x) for x in point] == endpoint['position_m']
                    assert [list(x) for x in velocity] == endpoint['derivative_m_per_theta']
                    counts['endpoint_replays'] += 1
                    floor = sum((Q(hi)-Q(lo))/2 for lo, hi in endpoint['position_m'])
                    assert floor == Q(endpoint['l1_halfwidth_exact_m'])
                    floors.append(floor)
                    # Both encoded source-height residual endpoints: a finite
                    # direct oracle, not exhaustive interval-error validation.
                    for residual in case['anchor_residual']:
                        h0 = sum(map(scalar, case['anchor_split']))+scalar(residual)
                        p, dp = direct_curve(frame, e2, case, theta, h0)
                        contained(endpoint['position_m'], [x-y for x, y in zip(p, origin)])
                        contained(endpoint['derivative_m_per_theta'], dp)
                        counts['endpoint_oracle_vectors'] += 1
                floor_failure = max(floors) > limit
                assert variant['endpoint_floor_exceeds_allocation'] == floor_failure
                assert variant['largest_endpoint_halfwidth_m'] == float(max(floors))
                if floor_failure or record['population'] == 'final-selected-pending':
                    assert not variant['fit_admitted'] and 'partitions' not in variant
                else:
                    for depth, partition in enumerate(variant['partitions']):
                        n = 1 << depth
                        assert partition['pieces'] == n and n <= 16
                        maximum = 0.
                        for index in range(partition['evaluated_pieces']):
                            certificate = fit(case, bounds, (index/n, (index+1)/n))
                            maximum = max(maximum, certificate['radius_m'])
                            counts['partition_piece_replays'] += 1
                            if index+1 < partition['evaluated_pieces']:
                                assert certificate['radius_m'] <= float(limit)
                        assert maximum == partition['largest_evaluated_radius_m']
                        passed = partition['evaluated_pieces'] == n and Q(maximum) <= limit
                        assert passed == partition['all_pieces_fit']
                        if not passed:
                            assert Q(certificate['radius_m']) > limit
                    assert variant['fit_admitted'] == variant['partitions'][-1]['all_pieces_fit']
                    if variant['fit_admitted']:
                        assert variant['pieces'] == variant['partitions'][-1]['pieces']
                item['variants'][name] = {'endpoint_halfwidth_m': float(max(floors)),
                    'endpoint_floor_failed': floor_failure, 'fit_admitted': variant['fit_admitted'],
                    'pieces': variant.get('pieces')}
            details.append(item)
    for population in summaries:
        group = [x for x in details if x['population'] == population]
        for name in NAMES:
            values = [x['variants'][name] for x in group]
            summaries[population][name] = {
                'count': len(values), 'fit_ids': [x['id'] for x in group if x['variants'][name]['fit_admitted']],
                'floor_failed_ids': [x['id'] for x in group if x['variants'][name]['endpoint_floor_failed']],
                'maximum_endpoint_halfwidth_m': max(v['endpoint_halfwidth_m'] for v in values),
            }
    accepted = summaries['accepted-height-capture']
    old, new = set(accepted['original-eye']['fit_ids']), set(accepted['near-source-vertex']['fit_ids'])
    assert old == new and len(old) == 11
    assert all(x['variants']['original-eye']['endpoint_floor_failed'] == x['variants']['near-source-vertex']['endpoint_floor_failed'] for x in details)
    assert all(x['variants']['original-eye']['pieces'] == x['variants']['near-source-vertex']['pieces'] for x in details)
    for name in NAMES:
        assert report['summary'][name] == {
            'accepted_capture_fit_passes': len(accepted[name]['fit_ids']),
            'endpoint_floor_failures_all_57': sum(x['variants'][name]['endpoint_floor_failed'] for x in details),
            'invalid_or_failed_prerequisites': 0}
    return {'schema': 'patch-relative-independent-review-v1', 'report_sha256': digest(prefix+'.json'),
        'auditor_sha256': digest('scripts/review_optical_patch_relative.py'),
        'inputs_verified': len(sources['files']), 'outputs_verified': len(outputs),
        'input_manifest_sha256': digest(prefix+'-sources.json'), 'output_manifest_sha256': digest(prefix+'-outputs.json'),
        'unchanged_inputs': True, 'lifecycle_exit_code': 0, 'counts': counts,
        'populations': summaries, 'newly_admitted_ids': sorted(new-old), 'lost_ids': sorted(old-new),
        'same_floor_decisions': True, 'same_partition_counts': True,
        'decision': 'reject-no-additional-real-curve-admission',
        'decision_policy': 'Root requires additional real-curve admission before contact work; the raw any-fit gate is insufficient for this usefulness decision.',
        'contact_work_authorized': False, 'gpu_admitted': False, 'production_admitted': False,
        'findings': [
            'Both variants admit exactly the same 11 of 34 accepted curves, with identical one/two-piece counts. The other 23 accepted curves exceed the unchanged endpoint fit allocation.',
            'All 23 final pending retries pass the endpoint-floor check in both variants. They were not partition-tested; this does not make their preceding uncertain trials or contacts valid.',
            'The nearby anchor reduces some endpoint interval widths but crosses no admission threshold in this retained workload.',
            'The existing-patch negative control fails all 57 floors for its specified same-start-chart and single interval residual representation only.'],
        'limits': [
            'Direct 100-digit ECEF references check all chosen anchor residuals and both stored endpoints at both source-height residual endpoints; this is finite numerical evidence, not a new analytic bound.',
            'Endpoint and partition replay reuses separately reviewed interval primitives and scalar M4/Hermite arithmetic; it is reproducibility, not an independent universal rounding proof.',
            'No earlier pending history, first contact, owner, strict root width, returned-state, ODE/global accuracy, GPU or preparation-performance claim is made.',
            'Failure of the current interval representation does not prove physical ray error or impossibility of other anchor representations.'],
        'cases': details}


if __name__ == '__main__':
    prefix = sys.argv[1] if len(sys.argv) > 1 else 'artifacts/optical-patch-relative-fit-first'
    output = ROOT/(prefix+'-independent-review.json')
    assert not output.exists(), 'Preserve the previous independent review.'
    result = review(prefix)
    output.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    print(json.dumps({k: result[k] for k in ('decision', 'counts', 'newly_admitted_ids', 'same_partition_counts')}))
