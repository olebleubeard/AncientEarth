"""Independent exact-input audit of manufactured canonical contact curves.

The all-face exclusion uses the already reviewed Cartesian position enclosure,
transformed at 90 digits to an outward binary64 ECEF box. Remaining face roots
use direct geocentric normal coordinates and bisection, independently of the
shader's stable-map, interval Newton, and membership code. This is a finite
static source/curve oracle, not an exact-ODE or runtime acceptance test.
"""
import argparse
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path
import struct

import mpmath as mp
import numpy as np


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def f64(bits):
    return struct.unpack('<d', struct.pack('<Q', bits))[0]


def scalar(x):
    return mp.mpf(float(x))


def dot(a, b):
    return sum(x*y for x, y in zip(a, b, strict=True))


def sub(a, b):
    return [x-y for x, y in zip(a, b, strict=True)]


def cross(a, b):
    return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]


def normalize(a):
    length = mp.sqrt(dot(a, a))
    return [x/length for x in a]


def linear_range(coefficients, bounds):
    products = [[c*lo, c*hi] for c, (lo, hi) in zip(coefficients, bounds, strict=True)]
    return [sum(min(p) for p in products), sum(max(p) for p in products)]


def outward_double(x, upper):
    result = float(x)
    if (mp.mpf(result) < x) if upper else (mp.mpf(result) > x):
        result = math.nextafter(result, math.inf if upper else -math.inf)
    return result


def review(prefix):
    meta = load(prefix+'.json')
    assert meta['schema'] == 'ancientearth.manufactured-canonical-contact.v1'
    assert meta['fresh_canonical_face_certificates_all_bytes_equal']
    assert load(prefix+'-input-stability.json')['inputs_unchanged_through_run']
    assert load(prefix+'-lifecycle.json')['exit_code'] == 0
    for item in load(prefix+'-outputs.json'):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
    source = load(prefix+'-sources.json')
    assert source['compiler_inputs_unchanged']
    sources_current = {row['path']: hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
                       for row in source['inputs']}
    records = np.fromfile(prefix+'-curves.bin', dtype='<u4').reshape(-1,64)
    pieces = np.fromfile(prefix+'-pieces.bin', dtype='<u4').reshape(-1,80)
    rf = records.view('<f4'); pf = pieces.view('<f4')
    faces = np.fromfile(prefix+'-faces.bin', dtype='<f8').reshape(-1,3,3)
    assert len(records) == meta['curve_count'] == 6
    assert len(pieces) == meta['piece_count']
    assert len(faces) == meta['face_count'] and np.isfinite(faces).all()
    cert = np.fromfile(meta['source_scene']+'-certificates.bin', dtype='<u4').reshape(-1,4)
    cf = cert.view('<f4'); first_face = int(cert[1,2])
    assert first_face+6*len(faces) == len(cert)
    cursor = 0
    owners = []
    for material, groups in [(0,meta['patches']), (1,meta['markers'])]:
        for group in groups:
            assert group['first_face'] == cursor
            for j in range(group['faces']):
                owners.append((2 if material and j>=12 else material, group['owner']))
            cursor += group['faces']
    assert cursor == len(faces) and meta['terrain_faces'] == sum(g['faces'] for g in meta['patches'])
    face_min, face_max = faces.min(axis=1), faces.max(axis=1)
    mp.mp.dps = 90
    exact = {k:mp.mpf(f64(v)) for k,v in meta['exact_f64_inputs'].items()}
    assert exact['a'] == 6378137
    # Independent spherical-angle construction of the exact ideal frame.
    longitude = mp.atan2(-exact['east_x'], exact['east_y'])
    latitude = mp.atan2(exact['up_z'], exact['north_z'])
    east = [-mp.sin(longitude), mp.cos(longitude), mp.mpf(0)]
    up = [mp.cos(latitude)*mp.cos(longitude), mp.cos(latitude)*mp.sin(longitude), mp.sin(latitude)]
    south = [mp.sin(latitude)*mp.cos(longitude), mp.sin(latitude)*mp.sin(longitude), -mp.cos(latitude)]
    columns = [east,up,south]
    axis = [mp.mpf(0), mp.sin(latitude), -mp.cos(latitude)]
    e = exact['eccentricity_squared']; a = exact['a']; eye_h = exact['eye_height']
    def absolute(normal, height):
        radius = a/mp.sqrt(1-e*normal[2]**2)
        return [(radius+height)*normal[0], (radius+height)*normal[1],
                (radius*(1-e)+height)*normal[2]]
    origin = absolute(up, eye_h)
    for i in range(3):
        assert scalar(meta['axis_intervals'][i][0]) <= axis[i] <= scalar(meta['axis_intervals'][i][1])
        for j in range(3):
            assert scalar(meta['ideal_frame_columns'][i][j][0]) <= columns[i][j] <= scalar(meta['ideal_frame_columns'][i][j][1])
    assert scalar(meta['eccentricity_interval'][0]) <= e <= scalar(meta['eccentricity_interval'][1])
    rows = []; sample_count = 0; certificate_faces = set(); cursor = 0
    maximum_fit_fraction = Q(0)
    for case, (row, words, values) in enumerate(zip(meta['cases'],records,rf,strict=True)):
        assert row['case'] == case and row['first_piece'] == cursor and words[63] == case
        assert words[:3].tolist() == row['start_words']
        assert words[12:15].tolist() == row['delta_words'] and words[3] == words[12]
        assert words[8:10].tolist() == row['anchor_words']
        assert words[58:60].tolist() == row['anchor_residual_words']
        assert words[4:7].tolist() == row['constant_direction_words']
        assert words[60:63].tolist() == [row['source_certificate_vec4'],row['source_material'],row['source_owner']]
        assert words[51] == 1 and words[50] == 0
        assert not np.any(words[16:48]) and not np.any(words[10:12])
        assert np.isfinite(values[:51]).all() and np.isfinite(values[52:60]).all()
        assert values[48] == row['declared_position_tube_m'] and values[49] == row['tolerance_m']
        assert row['source_certificate_vec4'] == first_face+6*row['source_face']
        assert owners[row['source_face']] == (row['source_material'],row['source_owner'])
        assert np.array_equal(faces[row['source_face']],np.array(row['source_vertices_ecef_m']))
        anchor = mp.mpf(f64(row['canonical_anchor_height_f64_bits']))
        difference = anchor-scalar(values[8])-scalar(values[9])
        assert scalar(values[58]) <= difference <= scalar(values[59])
        start = list(map(scalar,values[:3])); delta = list(map(scalar,values[12:15]))
        def position(theta):
            q = [x+theta*d for x,d in zip(start,delta,strict=True)]
            local_n = normalize([q[1],mp.mpf(1),-q[2]])
            n = [sum(columns[j][i]*local_n[j] for j in range(3)) for i in range(3)]
            return absolute(n,anchor+q[0])
        def local_position(theta):
            relative = sub(position(theta),origin)
            return [dot(column,relative) for column in columns]
        hits = []; excluded_by_plane = 0; excluded_by_box = 0; unresolved = []; all_candidates = set()
        for piece in range(row['pieces']):
            w = pieces[cursor+piece]; v = pf[cursor+piece]
            count = row['pieces']; assert count in [1,2,4,8,16,32,64]
            assert w[76:].tolist() == [case,piece,count,0]
            assert v[:3].tolist() == [piece/count,(piece+1)/count,float(count)]
            assert np.isfinite(v[:76]).all() and not np.any(w[list(range(7,76,4))])
            controls = v[12:44].reshape(4,2,4)[:,:,:3]
            derivatives = v[44:68].reshape(3,2,4)[:,:,:3]
            assert np.all(controls[:,0]<=controls[:,1]) and np.all(derivatives[:,0]<=derivatives[:,1])
            assert np.all(v[68:71]>=0) and np.all(v[72:75]>=0)
            radius = max(sum((Q(float(hi))-Q(float(lo)))/2 for lo,hi in zip(c[0],c[1])) for c in controls)
            radius += sum(Q(float(x)) for x in v[68:71])
            remaining = Q(float(values[49]))-Q(float(values[48]))
            assert radius <= Q(float(v[3])) <= remaining/4
            maximum_fit_fraction = max(maximum_fit_fraction,Q(float(v[3]))/remaining)
            split = [scalar(v[4+i])+scalar(v[8+i]) for i in range(3)]
            local_box = [[split[i]+min(scalar(c[0,i]) for c in controls)-scalar(v[68+i]),
                          split[i]+max(scalar(c[1,i]) for c in controls)+scalar(v[68+i])] for i in range(3)]
            local_derivative = [[min(scalar(c[0,i]) for c in derivatives)-scalar(v[72+i]),
                                 max(scalar(c[1,i]) for c in derivatives)+scalar(v[72+i])] for i in range(3)]
            box = [linear_range([column[i] for column in columns],local_box) for i in range(3)]
            box = [[lo+origin[i],hi+origin[i]] for i,(lo,hi) in enumerate(box)]
            lower = np.array([outward_double(x[0],False) for x in box])
            upper = np.array([outward_double(x[1],True) for x in box])
            candidates = np.nonzero(np.all(face_max>=lower,axis=1)&np.all(face_min<=upper,axis=1))[0]
            excluded_by_box += len(faces)-len(candidates); all_candidates.update(map(int,candidates))
            left, right = scalar(v[0]),scalar(v[1])
            for x in [mp.mpf(0),mp.mpf('.25'),mp.mpf('.5'),mp.mpf('.75'),mp.mpf(1)]:
                theta = left+(right-left)*x
                truth = local_position(theta)
                derivative = [mp.diff(lambda t:local_position(t)[i],theta) for i in range(3)]
                bp=[(1-x)**3,3*x*(1-x)**2,3*x*x*(1-x),x**3];bd=[(1-x)**2,2*x*(1-x),x*x]
                for i in range(3):
                    p=[split[i]+sum(b*scalar(controls[k,s,i]) for k,b in enumerate(bp)) for s in [0,1]]
                    d=[sum(b*scalar(derivatives[k,s,i]) for k,b in enumerate(bd)) for s in [0,1]]
                    rp=scalar(v[68+i]) if 0<x<1 else mp.mpf(0)
                    rd=scalar(v[72+i]) if 0<x<1 else mp.mpf(0)
                    assert p[0]-rp <= truth[i] <= p[1]+rp
                    assert d[0]-rd <= derivative[i] <= d[1]+rd
                sample_count += 1
            for index in map(int,candidates):
                vertices = [list(map(scalar,p)) for p in faces[index]]
                va,vb,vc = vertices; ab=sub(vb,va); ac=sub(vc,va)
                normal=normalize(cross(ab,ac)); local_normal=[dot(c,normal) for c in columns]
                # Independently audit every potentially intersecting face's retained
                # canonical coordinate certificate, including all earlier blockers.
                address=first_face+6*index
                vectors=[[dot(c,sub(origin,va)) for c in columns],
                         [dot(c,ab) for c in columns],[dot(c,ac) for c in columns]]
                for j,vector in enumerate(vectors):
                    for i in range(3):
                        assert scalar(cf[address+2*j,i]) <= vector[i] <= scalar(cf[address+2*j+1,i])
                certificate_faces.add(index)
                distance0=dot(normal,sub(origin,va))
                plane=linear_range(local_normal,local_box); plane=[x+distance0 for x in plane]
                if plane[0]>0 or plane[1]<0:
                    excluded_by_plane += 1; continue
                velocity=linear_range(local_normal,local_derivative)
                if not (velocity[0]>0 or velocity[1]<0):
                    unresolved.append({'face':index,'piece':piece,'reason':'derivative enclosure contains zero'})
                    continue
                def residual(t):
                    return dot(normal,sub(position(t),va))
                lo,hi=left,right; rlo,rhi=residual(lo),residual(hi)
                if rlo*rhi>0:
                    continue  # derivative certificate excludes any hidden root
                for _ in range(220):
                    mid=(lo+hi)/2; value=residual(mid)
                    if value*rlo>0:lo=mid;rlo=value
                    else:hi=mid
                theta=(lo+hi)/2; point=position(theta)
                # Gram barycentrics and independent signed edge distances.
                ap=sub(point,va); aa=dot(ab,ab); bb=dot(ac,ac); cc=dot(ab,ac)
                det=aa*bb-cc*cc
                beta=(dot(ap,ab)*bb-dot(ap,ac)*cc)/det
                gamma=(dot(ap,ac)*aa-dot(ap,ab)*cc)/det
                barycentric=[1-beta-gamma,beta,gamma]
                edges=[]
                for origin_edge,end_edge in zip(vertices,vertices[1:]+vertices[:1]):
                    edge=sub(end_edge,origin_edge)
                    edges.append(dot(cross(edge,sub(point,origin_edge)),normal)/mp.sqrt(dot(edge,edge)))
                if min(barycentric)<0:
                    continue
                hits.append({'face':index,'material':owners[index][0],'owner':owners[index][1],
                             'piece':piece,'theta':float(theta),'theta_bracket_width':float(hi-lo),
                             'path_coordinate_m':float(theta*scalar(values[7])),
                             'minimum_edge_distance_m':float(min(edges)),
                             'barycentric':list(map(float,barycentric)),
                             'absolute_plane_derivative_lower_m':float(min(abs(x) for x in velocity)),
                             'within_declared_edge_tube':min(edges)<=scalar(values[48])})
        hits.sort(key=lambda hit:(hit['theta'],hit['face']))
        assert not unresolved, (case,unresolved)
        rows.append({'case':case,'name':row['name'],'all_face_count':len(faces),
                     'broadphase_candidate_faces':sorted(all_candidates),
                     'excluded_face_piece_pairs_by_outward_box':excluded_by_box,
                     'excluded_by_certified_plane_range':excluded_by_plane,
                     'nominal_curve_intersections':hits,'first_nominal_contact':hits[0] if hits else None,
                     'declared_position_tube_m':float(values[48]),'unresolved_oracle_faces':unresolved})
        cursor+=row['pieces']
    assert cursor==len(pieces)
    assert rows[0]['first_nominal_contact']['material']==0
    assert rows[1]['first_nominal_contact']['material']==1
    assert rows[2]['first_nominal_contact']['material']==2
    assert not rows[3]['nominal_curve_intersections']
    assert rows[4]['first_nominal_contact']['within_declared_edge_tube']
    assert len(rows[5]['nominal_curve_intersections'])>=2
    assert rows[5]['nominal_curve_intersections'][0]['theta']<rows[5]['nominal_curve_intersections'][1]['theta']
    return {'scope':__doc__, 'prefix':prefix,'all_output_hashes_verified':True,
            'source_hashes_current':sources_current,'curve_count':len(records),'piece_count':len(pieces),
            'canonical_source_face_count':len(faces),'high_precision_position_derivative_samples':sample_count,
            'independently_checked_candidate_face_certificates':len(certificate_faces),
            'maximum_packed_fit_fraction_remaining':float(maximum_fit_fraction),
            'precision_digits':90,'bisection_iterations':220,
            'limitations':'Nominal center-curve intersections are distinct from robust ownership under input tubes. '
                          'The shared-edge tube intentionally prevents a unique strict face claim. All-face exclusion '
                          'depends on the separately reviewed analytic Cartesian enclosure; 30 point samples alone '
                          'do not prove it. No GPU result, ODE correctness or complete-ray first contact is inferred.',
            'cases':rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');args=parser.parse_args()
    target=Path(args.prefix+'-independent-review.json');assert not target.exists()
    result=review(args.prefix);target.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ['source_hashes_current','cases']},indent=2))
    print(json.dumps(result['cases'],indent=2))
