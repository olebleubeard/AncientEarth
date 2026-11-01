"""Exact packet/frame checks for sufficient-only static coverage metadata."""
import argparse
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path
import numpy as np


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def linear_interval(row, intervals):
    lower = upper = Q(0)
    for coefficient, (a, b) in zip(row, intervals, strict=True):
        products = [Q(coefficient)*Q(a), Q(coefficient)*Q(b)]
        lower += min(products); upper += max(products)
    return lower, upper


def review(prefix):
    data = load(prefix+'.json')
    assert data['schema'] == 'ancientearth.canonical-coverage-cores.v1'
    assert data['all_canonical_eye_to_patch_transform_words_exact']
    assert load(prefix+'-input-stability.json')['inputs_unchanged_through_run']
    for output in load(prefix+'-outputs.json'):
        assert hashlib.sha256(Path(output['path']).read_bytes()).hexdigest() == output['sha256']
    raw = np.fromfile(prefix+'-cores.bin', dtype='<u4').reshape(-1, 12)
    assert len(raw) == data['patch_count'] == len(data['records']) == 11
    assert data['record_words'] == 12
    values = raw.view('<f4')
    scene = load('artifacts/optical-normal-scene-packet.json')
    cert = np.fromfile(data['retained_scene_certificates'], dtype='<u4').reshape(-1,4)
    cf = cert.view('<f4'); patch_first = int(cert[0,3])
    assert len(scene['patches']) == int(cert[0,0]) == len(raw)
    rows = []
    for i, (record, bits, floats) in enumerate(zip(data['records'],raw,values,strict=True)):
        assert record['patch'] == i and bits.reshape(3,4).tolist() == record['words']
        base = patch_first + 9*i
        assert record['root'] == scene['patches'][i]['root'] == int(cert[base,1])
        assert record['triangles'] == scene['patches'][i]['triangles']
        assert np.isfinite(floats[:9]).all() and bits[10] == bits[11] == 0
        if bits[9] == 0:
            assert not np.any(bits) and record['proof'] is None
            rows.append({'patch':i,'valid':False}); continue
        assert bits[9] == 1
        proof = record['proof']; assert proof['degree'] != 0
        assert floats[3] >= 0 and floats[4] > 0 and floats[5] > 0 and floats[6] <= floats[7] and floats[8] >= 0
        nominal = list(map(Q, proof['nominal_center']))
        center_error = [abs(Q(float(floats[j]))-nominal[j]) for j in range(3)]
        assert Q(float(floats[4]))+center_error[0] <= Q(proof['nominal_half_extent_xz'][0])
        assert Q(float(floats[5]))+center_error[2] <= Q(proof['nominal_half_extent_xz'][1])
        assert Q(float(floats[3])) >= Q(proof['nominal_boundary_radius'])+sum(center_error)
        assert Q(float(floats[6])) <= Q(proof['nominal_vertical_bounds_y'][0])
        assert Q(float(floats[7])) >= Q(proof['nominal_vertical_bounds_y'][1])
        assert Q(float(floats[8])) >= Q(proof['canonical_vertex_error_m']) >= 0
        matrix = [list(map(Q,row)) for row in record['source_frame_rows_x_up_z']]
        a,b,c = matrix
        determinant = a[0]*(b[1]*c[2]-b[2]*c[1])-a[1]*(b[0]*c[2]-b[2]*c[0])+a[2]*(b[0]*c[1]-b[1]*c[0])
        lo,hi = map(Q,proof['determinant_interval'])
        assert lo <= determinant <= hi and (lo>0 or hi<0)
        # Independently enclose the exact stored matrix applied to every
        # point of the retained ideal-eye interval box, using rational linear
        # extrema rather than the preparer's floating interval implementation.
        shift = [[Q(v)-Q(origin) for v in interval]
                 for interval,origin in zip(scene['ideal_eye_origin_ecef_interval_m'],record['source_frame_origin'],strict=True)]
        vectors = [[linear_interval(row,shift) for row in matrix]]
        vectors += [[linear_interval(row,column) for column in scene['ideal_frame_columns']] for row in matrix]
        for j,vector in enumerate(vectors):
            for axis,(lower,upper) in enumerate(vector):
                assert Q(float(cf[base+1+2*j,axis])) <= lower <= upper <= Q(float(cf[base+2+2*j,axis]))
        rows.append({'patch':i,'valid':True,'degree':proof['degree'],
                     'canonical_vertex_error_m':proof['canonical_vertex_error_m'],
                     'packed_vertex_error_m':float(floats[8]),
                     'exact_determinant':str(determinant),'frame_interval_vectors_checked':4})
    return {'scope':'Static sufficient-core packet audit. Exact rational arithmetic checks all inward rectangles, outward radius/Y/error packing, determinants, and retained query-transform interval containment. Full source-vertex discrepancy loops and topology proofs are source-reviewed; this audit does not independently reimport every canonical mesh vertex. False cores defer to triangles. No GPU, contact, complete-ray or performance acceptance.',
            'prefix':prefix,'raw_hashes_and_immutability_verified':True,'patches':len(rows),
            'valid_cores':sum(r['valid'] for r in rows),'frame_interval_vectors_checked':sum(r.get('frame_interval_vectors_checked',0) for r in rows),
            'records':rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');args=parser.parse_args()
    result=review(args.prefix);output=Path(args.prefix+'-independent-review.json')
    assert not output.exists();output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='records'},indent=2))
