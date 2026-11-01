"""Raw six-case manufactured scene audit against the independent nominal oracle."""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path

import numpy as np
import mpmath as mp
from review_optical_positive_contacts import f64, scalar, dot, sub, cross, normalize


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def metadata_differences(a,b,path=''):
    if isinstance(a,dict):
        assert isinstance(b,dict) and a.keys()==b.keys()
        return sum((metadata_differences(a[k],b[k],path+'/'+str(k)) for k in a),[])
    if isinstance(a,list):
        assert isinstance(b,list) and len(a)==len(b)
        return sum((metadata_differences(x,y,path+'/'+str(i)) for i,(x,y) in enumerate(zip(a,b,strict=True))),[])
    if a==b:return []
    assert isinstance(a,float) and isinstance(b,float) and math.isfinite(a) and math.isfinite(b),(path,a,b)
    return [{'field':path,'original_metadata':a,'embedded_metadata':b}]


def contact_state_reference(packet, values, case, face, theta, lower, upper):
    """Separate direct geocentric map and oriented source-plane normal."""
    mp.mp.dps=90
    exact={k:mp.mpf(f64(v)) for k,v in packet['exact_f64_inputs'].items()}
    longitude=mp.atan2(-exact['east_x'],exact['east_y'])
    latitude=mp.atan2(exact['up_z'],exact['north_z'])
    east=[-mp.sin(longitude),mp.cos(longitude),mp.mpf(0)]
    up=[mp.cos(latitude)*mp.cos(longitude),mp.cos(latitude)*mp.sin(longitude),mp.sin(latitude)]
    south=[mp.sin(latitude)*mp.cos(longitude),mp.sin(latitude)*mp.sin(longitude),-mp.cos(latitude)]
    columns=[east,up,south];a=exact['a'];e=exact['eccentricity_squared']
    def absolute(n,h):
        radius=a/mp.sqrt(1-e*n[2]**2)
        return [(radius+h)*n[0],(radius+h)*n[1],(radius*(1-e)+h)*n[2]]
    origin=absolute(up,exact['eye_height'])
    anchor=mp.mpf(f64(packet['cases'][case]['canonical_anchor_height_f64_bits']))
    start=list(map(scalar,values[:3]));delta=list(map(scalar,values[12:15]))
    def position(t):
        q=[x+t*d for x,d in zip(start,delta,strict=True)]
        local_n=normalize([q[1],mp.mpf(1),-q[2]])
        n=[sum(columns[j][i]*local_n[j] for j in range(3)) for i in range(3)]
        return absolute(n,anchor+q[0])
    vertices=[list(map(scalar,p)) for p in face]
    plane_normal=normalize(cross(sub(vertices[1],vertices[0]),sub(vertices[2],vertices[0])))
    residual=lambda t:dot(plane_normal,sub(position(t),vertices[0]))
    lo,hi=scalar(lower),scalar(upper);rlo,rhi=residual(lo),residual(hi)
    assert rlo*rhi<=0
    for _ in range(180):
        mid=(lo+hi)/2;rm=residual(mid)
        if rlo*rm>0:lo=mid;rlo=rm
        else:hi=mid
    root=(lo+hi)/2
    local=lambda t:[dot(column,sub(position(t),origin)) for column in columns]
    return {'root':root,'position':local(root),'at_returned_theta':local(scalar(theta)),
            'direction':normalize(list(map(scalar,values[4:7]))),
            'normal':[dot(column,plane_normal) for column in columns]}


def review(prefix, baseline=None, compare_to=None, contact_states=False):
    report=load(prefix+'.json')
    assert report['positive_contacts'] and report['cartesian_curves'] and report['cartesian_covered']
    assert not report['selected'] and not report['timed'] and not report['reference']
    assert load(prefix+'-input-stability.json')['inputs_unchanged_through_run']
    for item in load(prefix+'-outputs.json'):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest()==item['sha256'],item['path']
    fixture=report['fixture'];packet_prefix=fixture['positive_packet_prefix']
    for item in load(packet_prefix+'-outputs.json'):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest()==item['sha256'],item['path']
    packet=load(packet_prefix+'.json');oracle=load(packet_prefix+'-independent-review.json')
    faces=np.fromfile(packet_prefix+'-faces.bin',dtype='<f8').reshape(-1,3,3) if contact_states else None
    metadata=metadata_differences(packet,fixture['positive_packet'])
    curve_bytes=Path(packet_prefix+'-curves.bin').read_bytes();piece_bytes=Path(packet_prefix+'-pieces.bin').read_bytes()
    assert Path(prefix+'-curves.bin').read_bytes()==curve_bytes+piece_bytes
    curves=np.frombuffer(curve_bytes,dtype='<u4').reshape(6,64)
    if contact_states:
        assert not np.any(curves[:,16:48]), 'Direct oracle requires affine q and constant u/optical coefficients'
    pieces=np.frombuffer(piece_bytes,dtype='<u4').reshape(-1,80);n=len(pieces)
    assert n==packet['piece_count']==report['record_count']==6
    assert report['dimensions']==[n,1] and report['dispatch']==[(n+31)//32,1,1]
    assert fixture['binding6_layout']=={'curve_records':6,'curve_record_vec4':16,'piece_base_vec4':96,'piece_record_vec4':20,'pieces':n}
    for kind in ['geometry','frames','certificates']:
        assert Path(prefix+'-'+kind+'.bin').read_bytes()==Path(packet['source_scene']+'-'+kind+'.bin').read_bytes()
    assert Path(prefix+'-field.bin').read_bytes()==Path('artifacts/optical-height-trial-capture-uniform.bin').read_bytes()
    primary=np.fromfile(prefix+'-primary.bin',dtype='<u4')
    old=np.fromfile('artifacts/optical-cartesian-covered-first-primary.bin',dtype='<u4').copy()
    assert primary.size==old.size==560
    old[44:48]=report['expected_header']
    if report.get('uniform_loops'):
        assert report['loop_policy']['uniform_byte_offset']==156 and report['loop_policy']['iterations']==32
        assert primary[39]==0x42000000;old[39]=primary[39]
    assert np.array_equal(primary,old)
    raw=np.fromfile(prefix+'-readback.bin',dtype='<u4')
    stride=report['diagnostic_bytes_per_record']
    assert stride in (32,48) and bool(report.get('positive_stages'))==(stride==48)
    assert raw.nbytes==report['output_bytes']==16+(48+stride)*n
    assert report['terminal_bytes']==16+48*n
    assert raw[:4].tolist()==report['header']==report['expected_header'] and report['header_matches']
    terminal=raw[4:4+12*n].reshape(n,12);details=raw[4+12*n:].reshape(n,stride//4)
    tf=terminal.view('<f4');df=details.view('<f4');cf=curves.view('<f4');pf=pieces.view('<f4')
    assert np.isfinite(tf[:,:7]).all() and np.isfinite(tf[:,8:11]).all() and np.isfinite(df[:,4:8]).all()
    assert set(map(int,terminal[:,7]))<={1,2,4}
    assert set(map(int,details[:,0]))<={0,1,2}
    assert np.array_equal(np.array([4,1,2],dtype='<u4')[details[:,0]],terminal[:,7])
    assert np.array_equal(details[:,2:4],pieces[:,76:78])
    assert np.array_equal(details[:,7],curves[pieces[:,76],49])
    assert np.all(details[:,1]<=65537)
    assert {str(k):v for k,v in Counter(map(int,terminal[:,7])).items()}==report['raw_status_counts']
    assert len(report['samples'])==n
    stage_table={}
    if stride==48:
        stage=fixture['stage_diagnostics'];assert stage['diagnostic_bytes_per_record']==48
        stage_table={v['code']:v for v in stage['stages']}
        assert len(stage_table)==len(stage['stages'])
        assert set(map(int,details[:,8])) <= ({0}|set(stage_table))
        assert set(map(int,details[:,10])) <= {0,1,2,0xffffffff}
        assert np.all(details[:,11]==0)
        cert=np.fromfile(prefix+'-certificates.bin',dtype='<u4').reshape(-1,4)
        address_count=len(cert);first_face=int(cert[1,2])
        assert all(v==0xffffffff or (first_face<=v and int(v)+6<=address_count and (int(v)-first_face)%6==0) for v in details[:,9])
    comparison=None
    assert not (baseline and compare_to)
    if baseline or compare_to:
        reference_prefix=baseline or compare_to
        old_report=load(reference_prefix+'.json')
        assert old_report['record_count']==n and old_report['diagnostic_bytes_per_record']==32
        for kind in ('primary','field','geometry','frames','certificates','curves'):
            assert Path(prefix+'-'+kind+'.bin').read_bytes()==Path(reference_prefix+'-'+kind+'.bin').read_bytes()
        old_raw=np.fromfile(reference_prefix+'-readback.bin',dtype='<u4')
        assert old_raw.nbytes==16+80*n
        assert np.array_equal(raw[:4],old_raw[:4])
        old_terminal=old_raw[4:4+12*n].reshape(n,12);old_detail=old_raw[4+12*n:].reshape(n,8)
        terminal_changed=np.nonzero(np.any(terminal!=old_terminal,axis=1))[0]
        detail_changed=np.nonzero(np.any(details[:,:8]!=old_detail,axis=1))[0]
        if baseline:assert not len(terminal_changed) and not len(detail_changed)
        comparison={'baseline':reference_prefix,'six_input_buffers_exact':True,'header_exact':True,
                    'terminal_word_differences':int(np.count_nonzero(terminal!=old_terminal)),
                    'original_detail_word_differences':int(np.count_nonzero(details[:,:8]!=old_detail)),
                    'changed_records':[{'index':int(i),'old_terminal_words':old_terminal[i].tolist(),
                                        'new_terminal_words':terminal[i].tolist(),'old_detail_words':old_detail[i].tolist(),
                                        'new_detail_words':details[i,:8].tolist()} for i in sorted(set(terminal_changed)|set(detail_changed))],
                    'baseline_raw_sha256':hashlib.sha256(old_raw.tobytes()).hexdigest()}
    rows=[];false_clear=[];unmatched_hit=[]
    for i,sample in enumerate(report['samples']):
        assert sample['record_index']==i and sample['pixel']==[i,0]
        assert sample['terminal_words']==terminal[i].tolist() and sample['diagnostic_words']==details[i].tolist()
        case=int(pieces[i,76]);ground=oracle['cases'][case];first=ground['first_nominal_contact']
        status=int(terminal[i,7]);theta,lower,upper,tolerance=map(float,df[i,4:8])
        lower_range,upper_range=map(float,pf[i,:2]);match=None
        if status==1:
            assert lower_range<=lower<=theta<=upper<=upper_range
            assert abs(float(tf[i,3])-theta*float(cf[case,7]))<=max(1e-7,abs(float(tf[i,3]))*2e-7)
            packed_owner=int(terminal[i,11]);material=packed_owner>>24;owner=packed_owner&0xffffff
            match={'material_owner_match':bool(first and material==first['material'] and owner==first['owner']),
                   'path_error_to_first_nominal_m':abs(float(tf[i,3])-first['path_coordinate_m']) if first else None,
                   # Oracle theta is saved as binary64; use its adjacent floats
                   # rather than treating the serialized decimal as exact truth.
                   'first_nominal_root_in_gpu_enclosure':bool(first and lower<=math.nextafter(first['theta'],-math.inf)
                                                            and upper>=math.nextafter(first['theta'],math.inf)),
                   'shared_edge_tube_requires_uncertainty':case==4}
            valid=match['material_owner_match'] and match['first_nominal_root_in_gpu_enclosure'] and case!=4
            valid=valid and match['path_error_to_first_nominal_m']<=tolerance
            if contact_states and valid:
                truth=contact_state_reference(packet,cf[case],case,faces[first['face']],theta,lower,upper)
                vector_error=lambda got,want:mp.sqrt(sum((scalar(v)-w)**2 for v,w in zip(got,want,strict=True)))
                pe=vector_error(tf[i,:3],truth['position']);at_theta=vector_error(tf[i,:3],truth['at_returned_theta'])
                de=vector_error(tf[i,4:7],truth['direction']);ne=vector_error(tf[i,8:11],truth['normal'])
                se=abs(scalar(tf[i,3])-truth['root']*scalar(cf[case,7]))
                metric=max(pe,scalar(cf[case,7])*scalar(upper_range)*de,se)+scalar(cf[case,48])
                orientation=dot(list(map(scalar,tf[i,8:11])),truth['normal'])
                match['returned_state']={'position_error_to_nominal_root_m':float(pe),
                    'position_error_at_returned_theta_m':float(at_theta),'synthetic_direction_vector_error':float(de),
                    'path_error_to_refined_nominal_root_m':float(se),'oriented_face_normal_vector_error':float(ne),
                    'oriented_face_normal_positive_dot':bool(orientation>0),
                    'complete_nominal_metric_plus_declared_tube_m':float(metric),
                    'within_unchanged_tolerance':bool(metric<=scalar(tolerance)),
                    'normal_scope':'Normal error is reported separately; the existing complete metric charges position, direction and path.'}
                valid=valid and metric<=scalar(tolerance) and orientation>0
            if not valid:unmatched_hit.append(i)
        else:
            assert theta==lower==upper==0
            if status==4 and first and lower_range<=first['theta']<=upper_range:false_clear.append(i)
        stage_result=None
        if stride==48:
            code,address,coverage,_=map(int,details[i,8:12])
            stage_result={'return_code':code,'last_face_certificate_address':address,
                          'h0_coverage_or_unreached':coverage,'source_tag':stage_table.get(code)}
        rows.append({'record_index':i,'case':case,'name':ground['name'],'status':status,
                     'event_kind':int(details[i,0]),'scene_visits':int(details[i,1]),
                     'theta_range':[lower_range,upper_range],'theta':theta,'lower':lower,'upper':upper,
                     'path_m':float(tf[i,3]),'tolerance_m':tolerance,'hit_comparison':match,
                     'nominal_oracle_first_contact':first,'terminal_words':terminal[i].tolist(),
                     'diagnostic_words':details[i].tolist(),'stage_diagnostic':stage_result})
    return {'scope':__doc__,'prefix':prefix,'all_hashes_frozen_inputs_and_packet_bytes_verified':True,
            'embedded_descriptive_float_differences':metadata,'piece_count':n,
            'status_counts':dict(Counter(map(int,terminal[:,7]))),'false_clear_records':false_clear,
            'unmatched_or_forbidden_hit_records':unmatched_hit,'records':rows,'baseline_comparison':comparison,
            'contact_state_oracle_enabled':contact_states,
            'limitations':'Each output is an independent manufactured geometric-curve piece, not an accepted ODE '
                          'trajectory or complete camera ray. Nominal source-face oracle and synthetic position '
                          'tube are distinct; shared-edge tube cannot establish a strict single-face hit. '
                          +('Returned hit position, normalized synthetic direction and oriented canonical normal are compared '
                            'with a direct90-digit source oracle; normal error is separately reported without inventing a normal budget. '
                            if contact_states else 'Returned position/direction/normal payloads are finite-only in this audit. ')+
                          'Uncertain pieces remain unresolved. '
                          'No native/runtime/performance acceptance.'}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');parser.add_argument('--output');parser.add_argument('--baseline');parser.add_argument('--compare-to');parser.add_argument('--contact-states',action='store_true');args=parser.parse_args()
    output=Path(args.output or args.prefix+'-independent-review.json');assert not output.exists()
    result=review(args.prefix,args.baseline,args.compare_to,args.contact_states);output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='embedded_descriptive_float_differences'},indent=2))
