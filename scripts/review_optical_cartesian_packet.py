"""Exact packet arithmetic and finite independent canonical map checks."""
import argparse
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path
import struct
import mpmath as mp
import numpy as np
from review_optical_normal_map import direct_map, decoded_f64


def audit(prefix):
    data=json.loads(Path(prefix+'.json').read_text());proof=json.loads(Path(prefix+'-provenance.json').read_text())
    for group in ['inputs','outputs']:
        for path,sha in proof[group].items():assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==sha,path
    source_hashes={path:hashlib.sha256(Path(path).read_bytes()).hexdigest()==sha for path,sha in proof['sources'].items()}
    raw=np.fromfile(prefix+'-pieces.bin',dtype='<u4').reshape(-1,80);values=raw.view('<f4')
    assert data['record_words']==80 and len(raw)==data['piece_count'] and data['curve_count']==495
    captured=np.fromfile('artifacts/optical-height-trial-capture-readback.bin',dtype='<u4').reshape(7,13864)
    uniform=np.fromfile('artifacts/optical-height-trial-capture-uniform.bin',dtype='<u4');uf=uniform.view('<f4')
    cert=np.fromfile('artifacts/optical-normal-scene-packet-certificates.bin',dtype='<u4');cf=cert.view('<f4')
    packet=json.loads(Path('artifacts/optical-normal-scene-packet.json').read_text())
    inputs=json.loads(Path(packet['canonical_input']).read_text())['cases'][0]['inputs']
    expected=[]
    for ray,record in enumerate(captured):
        assert record[38]==0
        for trial in range(int(record[36])):
            words=record[40+108*trial:40+108*(trial+1)]
            if words[96]==2:expected.append((ray,trial,words))
    assert len(expected)==495
    near=[];selected={0,247,494};cursor=0;pieces_hist=Counter();largest_fraction=Q(0)
    for i,(row,(ray,trial,words)) in enumerate(zip(data['curves'],expected,strict=True)):
        assert (row['curve_index'],row['capture_ray'],row['capture_trial'],row['first_piece'])==(i,ray,trial,cursor)
        pieces=row['pieces'];assert pieces in [1,2,4,8,16,32,64];pieces_hist[pieces]+=1
        f=words.view('<f4');exact_numerical=Q(float(f[48]))+Q(float(f[97]));tolerance=Q(float(f[98]))
        assert Q(row['numerical_source_error_m'])>=exact_numerical
        assert Q(row['remaining_m'])<=tolerance-Q(row['numerical_source_error_m'])
        assert Q(row['tolerance_m'])==tolerance and row['remaining_m']>0
        if trial==0:selected.add(i)
        if pieces>1:selected.add(i)
        if float(f[8])+float(f[9])+float(f[0])+min(0.,float(f[3]))<=167:near.append(i);selected.add(i)
        for j in range(pieces):
            w=raw[cursor+j];v=values[cursor+j]
            assert w[76:].tolist()==[i,j,pieces,int(data['scalar_majorant'])]
            assert np.isfinite(v[:76]).all()
            assert Q(float(v[0]))==Q(j,pieces) and Q(float(v[1]))==Q(j+1,pieces) and v[2]==pieces
            assert all(w[k]==0 for k in [7,11,*range(15,68,4),71,75])
            position=v[12:44].reshape(4,2,4)[:,:,:3];derivative=v[44:68].reshape(3,2,4)[:,:,:3]
            assert np.all(position[:,0]<=position[:,1]) and np.all(derivative[:,0]<=derivative[:,1])
            assert np.all(v[68:71]>=0) and np.all(v[72:75]>=0)
            radius=max(sum((Q(float(hi))-Q(float(lo)))/2 for lo,hi in zip(control[0],control[1])) for control in position)
            radius+=sum(Q(float(x)) for x in v[68:71])
            assert Q(float(v[3]))>=radius
            assert Q(float(v[3]))<=Q(row['remaining_m'])/4
            largest_fraction=max(largest_fraction,Q(float(v[3]))/Q(row['remaining_m']))
        cursor+=pieces
    assert cursor==len(raw)
    mp.mp.dps=90
    recorded=json.loads(Path('artifacts/optical-normal-map-readback-first.json').read_text())['fixture']['canonical_f64_bits']
    axis_raw=list(map(lambda b:mp.mpf(decoded_f64(b)),recorded['axis_before_normalization']))
    length=mp.sqrt(sum(x*x for x in axis_raw));axis=[x/length for x in axis_raw]
    e=mp.mpf(decoded_f64(recorded['eccentricity']));eye=mp.mpf(packet['canonical_eye_geodetic'][2])
    assert eye==mp.mpf(decoded_f64(recorded['eye_height_m']))
    for i in range(3):assert mp.mpf(float(cf[8+i]))<=axis[i]<=mp.mpf(float(cf[12+i]))
    assert mp.mpf(float(cf[11]))<=e<=mp.mpf(float(cf[15]))
    count=0;endpoints=0
    for index in sorted(selected):
        row=data['curves'][index];f=expected[index][2].view('<f4');w=expected[index][2]
        matches=[i for i in range(int(cert[2])) if np.array_equal(w[8:10],uniform[20+4*i:22+4*i])]
        if np.array_equal(w[8:10],uniform[2:4]):
            assert not matches;anchor=eye;residual=cf[16:18]
        else:
            assert len(matches)==1;i=matches[0]
            anchor=mp.mpf(inputs['profile_base_height_m']+inputs['layers'][i]['height_m'])
            offset=4*(int(cert[5])+i);residual=cf[offset:offset+2]
        difference=anchor-mp.mpf(float(f[8]))-mp.mpf(float(f[9]))
        assert mp.mpf(float(residual[0]))<=difference<=mp.mpf(float(residual[1]))
        start=list(map(float,f[:3]));coeff=[list(map(float,f[12+4*i:15+4*i])) for i in range(4)]
        coeff[0][0]=float(f[3])
        for r in coeff[1:]:r[0]=0.
        for j in range(row['pieces']):
            v=values[row['first_piece']+j];origin=[mp.mpf(float(v[4+i]))+mp.mpf(float(v[8+i])) for i in range(3)]
            for x in [mp.mpf(0),mp.mpf('0.25'),mp.mpf('0.5'),mp.mpf('0.75'),mp.mpf(1)]:
                theta=(j+x)/row['pieces'];truth=direct_map(axis,e,anchor,eye,start,coeff,theta)
                velocity=[mp.diff(lambda t:direct_map(axis,e,anchor,eye,start,coeff,t)[i],theta) for i in range(3)]
                wp=[(1-x)**3,3*x*(1-x)**2,3*x*x*(1-x),x**3];wd=[(1-x)**2,2*x*(1-x),x*x]
                for i in range(3):
                    p=[origin[i]+sum(wt*mp.mpf(float(v[12+8*k+4*side+i])) for k,wt in enumerate(wp)) for side in [0,1]]
                    d=[sum(wt*mp.mpf(float(v[44+8*k+4*side+i])) for k,wt in enumerate(wd)) for side in [0,1]]
                    # Endpoint fitting error is identically zero; stored endpoint intervals remain.
                    rp=mp.mpf(float(v[68+i])) if 0<x<1 else mp.mpf(0)
                    rd=mp.mpf(float(v[72+i])) if 0<x<1 else mp.mpf(0)
                    assert p[0]-rp<=truth[i]<=p[1]+rp,(index,j,x,'position',i)
                    assert d[0]-rd<=velocity[i]<=d[1]+rd,(index,j,x,'derivative',i)
                count+=1;endpoints+=x in [0,1]
    return {'scope':'All495 captured-curve identities, every packed piece/chronology/budget and exact dyadic metadata checked. Finite90-digit direct Cartesian/derivative oracle samples selected curves (all near-scene and subdivided curves included); this supplements analytic remainder proofs. No GPU arithmetic, face membership, contact order, global ray error, or runtime acceptance.',
            'prefix':prefix,'curve_count':495,'piece_count':len(raw),'byte_count':len(raw)*320,
            'pieces_per_curve':dict(pieces_hist),'maximum_packed_radius_fraction_remaining':float(largest_fraction),
            'all_input_output_hashes_verified':True,'current_source_hash_match':source_hashes,
            'selected_curve_indices':sorted(selected),'near_scene_curves':len(near),
            'high_precision_point_and_derivative_samples':count,'exact_piece_endpoint_samples_without_fit_remainder':endpoints}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');args=parser.parse_args()
    output=Path(args.prefix+'-independent-review.json');assert not output.exists()
    result=audit(args.prefix);output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
