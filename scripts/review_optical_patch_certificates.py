"""Exact all-face dyadic audit and high-precision dynamic-frame audit; no GPU claim."""
import argparse
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path
import struct
import mpmath as mp
import numpy as np


def read(path):return json.loads(Path(path).read_text(encoding='utf-8-sig'))
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def decoded(bits):return struct.unpack('<d',struct.pack('<Q',bits))[0]


def decomposition(values):
    """Exact integer significands/exponents, without floating subtraction."""
    values=np.asarray(values,dtype='<f8');assert np.isfinite(values).all()
    raw=values.view('<u8');encoded=(raw>>np.uint64(52))&np.uint64(2047)
    mantissa=raw&np.uint64((1<<52)-1)
    mantissa=np.where(encoded==0,mantissa,mantissa|np.uint64(1<<52))
    exponent=np.where(encoded==0,-1074,encoded.astype(np.int64)-1075)
    sign=np.where((raw>>np.uint64(63))==0,1,-1)
    return mantissa,exponent,sign


def minimum_exponent(values):
    mantissa,exponent,_=decomposition(values)
    return int(exponent[mantissa!=0].min()) if np.any(mantissa!=0) else 0


def integers(values,exponent):
    mantissa,powers,sign=decomposition(values)
    shifts=np.where(mantissa==0,0,powers-exponent)
    assert (shifts>=0).all()
    shifted=np.frompyfunc(lambda a,b:int(a)<<int(b),2,1)(mantissa,shifts)
    return shifted*sign.astype(object)


def compare_exact(values,exponent,lower,upper,high=None):
    """Check exact dyadic truth against every stored f32 bound, optionally split."""
    all_bounds=[lower,upper] if high is None else [lower,upper,high]
    scale=min(exponent,minimum_exponent(np.concatenate([np.asarray(v).ravel() for v in all_bounds])))
    truth=values*(1<<(exponent-scale))
    lo=integers(lower,scale);hi=integers(upper,scale)
    if high is not None:
        origin=integers(high,scale);lo=lo+origin;hi=hi+origin
    assert np.all(lo<=truth) and np.all(truth<=hi), 'Outward face interval excludes exact source projection'


def review(prefix):
    data=read(prefix+'.json');assert data['schema']=='ancientearth.immutable-patch-certificates.v1'
    for entry in read(prefix+'-outputs.json'):assert sha(entry['path'])==entry['sha256']
    source=read(prefix+'-sources.json');stability=read(prefix+'-input-stability.json')
    assert source['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    before={x['path']:x['sha256'] for x in source['files']}
    after={x['path']:x['sha256'] for x in stability['after_run']}
    assert all(before[p]==h for p,h in after.items()) and len(before)==len(after)+1
    life=read(prefix+'-lifecycle.json');assert life['exit_code']==0 and not life['timed_out'] and life['failure'] is None
    faces=np.fromfile(prefix+'-faces.bin',dtype='<f4').reshape(-1,7,4)
    original=np.fromfile(data['retained_geometry'],dtype='<u4').reshape(-1,4)
    geometry=np.fromfile(prefix+'-geometry.bin',dtype='<u4').reshape(-1,4)
    source_faces=np.fromfile(data['source_faces'],dtype='<f8').reshape(-1,3,3)
    assert data['faces']==len(faces)==618560 and data['patch_count']==11
    assert faces.nbytes==data['static_face_bytes'] and data['face_stride_vec4']==7 and data['eye_patch_stride_vec4']==9
    assert original.shape==geometry.shape and np.isfinite(faces).all()
    assert np.all(faces[:,:,3].view('<u4')==0)
    assert np.all((faces==0)|(np.abs(faces)>=np.finfo(np.float32).tiny))
    assert np.all(faces[:,1,:3]<=faces[:,2,:3]) and np.all(faces[:,3,:3]<=faces[:,4,:3]) and np.all(faces[:,5,:3]<=faces[:,6,:3])
    allowed=np.zeros(original.shape,dtype=bool);cursor=0;patch_rows=[]
    old_scene=read('artifacts/optical-normal-scene-packet.json')
    for patch,old in zip(data['patches'],old_scene['patches'],strict=True):
        count=patch['faces'];root=patch['root']
        assert patch['patch']==old['patch'] and root==old['root'] and count==old['triangles']
        assert patch['first_face_vec4']==7*cursor
        # The retained root-node escape is the first triangle address; its
        # complete node subtree precedes the contiguous triangle payload.
        triangle_start=int(original[root,3]);assert triangle_start>root and (triangle_start-root)%2==0
        addresses=triangle_start+3*np.arange(count)
        allowed[addresses,3]=True
        assert np.array_equal(geometry[addresses,3],7*np.arange(cursor,cursor+count,dtype=np.uint32))
        rows=np.array([[decoded(x) for x in row] for row in patch['rows_bits']]);origin=np.array(list(map(decoded,patch['origin_bits'])))
        determinant=sum(Q(float(rows[0,i]))*(Q(float(rows[1,(i+1)%3]))*Q(float(rows[2,(i+2)%3]))-Q(float(rows[1,(i+2)%3]))*Q(float(rows[2,(i+1)%3]))) for i in range(3))
        assert determinant>0
        assert Q(patch['positive_determinant_interval'][0])<=determinant<=Q(patch['positive_determinant_interval'][1])
        # Exact Python integer arithmetic checks ALL faces, chunked only for
        # memory. Rows and canonical source vertices are binary64 inputs;
        # no common f64 dot product or rounded coordinate inverse is reused.
        for start in range(cursor,cursor+count,4096):
            stop=min(cursor+count,start+4096);points=source_faces[start:stop]
            exponent=min(minimum_exponent(points),minimum_exponent(origin))
            point_int=integers(points,exponent);origin_int=integers(origin,exponent)
            vectors=[origin_int-point_int[:,0,:],point_int[:,1,:]-point_int[:,0,:],point_int[:,2,:]-point_int[:,0,:]]
            packet=faces[start:stop]
            for axis,row in enumerate(rows):
                row_exp=minimum_exponent(row);row_int=integers(row,row_exp)
                for component,vector in enumerate(vectors):
                    exact=sum((vector[:,i]*row_int[i] for i in range(3)))
                    lo_index=1+2*component
                    compare_exact(exact,exponent+row_exp,packet[:,lo_index,axis],packet[:,lo_index+1,axis],
                                  packet[:,0,axis] if component==0 else None)
        cursor+=count;patch_rows.append((rows,origin))
    assert cursor==len(faces)
    changed=geometry!=original
    assert not np.any(changed&~allowed)
    assert int(changed.sum())==data['geometry_w_address_slots_changed']
    # Re-exported actual Rust libm inputs define the ideal frame; geodetic-angle
    # sin/cos from a different library are not substituted. The exporter compile
    # command was manually checked in retained tool evidence. Its retrospective
    # *-eye-inputs-provenance.json records retained source/executable/rlib hashes
    # and their preparation/audit matches; this audit does not itself establish
    # a before/after exporter compilation guard.
    eye_inputs=read(prefix+'-eye-inputs.json')
    assert eye_inputs['source']==prefix+'.json'
    mp.mp.dps=90;a=mp.mpf(decoded(eye_inputs['equatorial_radius_bits']));e=mp.mpf(decoded(eye_inputs['eccentricity_bits']))
    eye_checks=0
    for metadata,inputs in zip(data['eyes'],eye_inputs['eyes'],strict=True):
        assert metadata['eye']==inputs['eye'] and metadata['geodetic_bits']==inputs['geodetic_bits']
        ex,ey,s,c=map(lambda b:mp.mpf(decoded(b)),inputs['raw_east_xy_upz_northz_bits'])
        longitude=mp.atan2(-ex,ey);latitude=mp.atan2(s,c)
        east=[-mp.sin(longitude),mp.cos(longitude),mp.mpf(0)]
        up=[mp.cos(latitude)*mp.cos(longitude),mp.cos(latitude)*mp.sin(longitude),mp.sin(latitude)]
        minus_north=[mp.sin(latitude)*mp.cos(longitude),mp.sin(latitude)*mp.sin(longitude),-mp.cos(latitude)]
        columns=[east,up,minus_north];height=mp.mpf(decoded(inputs['geodetic_bits'][2]))
        radius=a/mp.sqrt(1-e*mp.sin(latitude)**2)
        eye=[(radius+height)*up[0],(radius+height)*up[1],(radius*(1-e)+height)*up[2]]
        dynamic=np.fromfile(prefix+f'-eye-{metadata["eye"]}.bin',dtype='<f4').reshape(11,9,4)
        assert dynamic.nbytes==metadata['metadata_bytes']==1584 and np.isfinite(dynamic).all()
        assert np.all(dynamic[:,:,3].view('<u4')==0)
        assert np.all((dynamic==0)|(np.abs(dynamic)>=np.finfo(np.float32).tiny))
        for patch,(rows,origin) in enumerate(patch_rows):
            packet=dynamic[patch]
            for axis,row in enumerate(rows):
                shift=sum(mp.mpf(float(row[i]))*(eye[i]-mp.mpf(float(origin[i]))) for i in range(3))
                lo=mp.mpf(float(packet[0,axis]))+mp.mpf(float(packet[1,axis]));hi=mp.mpf(float(packet[0,axis]))+mp.mpf(float(packet[2,axis]))
                assert lo<=shift<=hi
                for column in range(3):
                    truth=sum(mp.mpf(float(row[i]))*columns[column][i] for i in range(3))
                    assert mp.mpf(float(packet[3+2*axis,column]))<=truth<=mp.mpf(float(packet[4+2*axis,column]))
                    eye_checks+=1
    return {'scope':__doc__,'report_sha256':sha(prefix+'.json'),
            'frozen_sources_and_outputs_verified':True,
            'current_source_paths_changed_since_run':[p for p,h in after.items() if not Path(p).exists() or sha(p)!=h],
            'exact_source_faces_checked':len(faces),'exact_coordinate_intervals_checked':len(faces)*9,
            'patches':len(patch_rows),'geometry_words_changed_only_at_exact_first_vertex_address_slots':int(changed.sum()),
            'all_other_geometry_words_exact':True,'eye_records':3,'eye_matrix_components_checked':eye_checks,
            'eye_translation_components_checked':3*11*3,'static_face_bytes':faces.nbytes,
            'per_eye_metadata_bytes':1584,
            'static_preparation_seconds_single_sample':data['static_preparation_seconds'],
            'eye_update_seconds_single_samples':[r['single_pass_update_seconds'] for r in data['eyes']],
            'additional_exact_eye_input_sha256':sha(prefix+'-eye-inputs.json'),
            'eye_exporter_sha256':sha('artifacts/export-immutable-eye-inputs.rs'),
            'limitations':'All static narrow-face interval components are checked against exact dyadic source projections. '
                          'The three dynamic eyes use90-digit independent spherical-frame/Cartesian construction from '
                          'actual re-exported libm input bits. This checks packet preparation, not a GPU transform or '
                          'contact predicate. Raw patch rows are not assumed orthogonal; returned normals require '
                          'transpose transformation. Original packed-BVH/source-coordinate allowances remain a '
                          'separate required broadphase input. Billboard signs and runtime upload/invalidation '
                          'are outside this terrain packet. Costs are single CPU samples, not a native/GPU gain.'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('prefix');args=parser.parse_args()
    output=Path(args.prefix+'-independent-review.json');assert not output.exists()
    result=review(args.prefix);output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
