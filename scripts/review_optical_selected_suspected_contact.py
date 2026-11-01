"""Converged numerical-ray check plus exact dense-polynomial/source exclusion.

The Fraction certificate is for the retained DOP853 interpolation polynomial,
not for the exact physical ODE or the GPU's unpublished accepted intervals.
"""
import argparse
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path

import mpmath as mp
import numpy as np
from scipy.integrate import solve_ivp

import verify_optical_dop853 as oracle
from review_optical_positive_contacts import f64


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def add(a,b):
    return [x+y for x,y in zip(a,b,strict=True)]


def sub(a,b):
    return [x-y for x,y in zip(a,b,strict=True)]


def dot(a,b):
    return sum((x*y for x,y in zip(a,b,strict=True)),Q())


def cross(a,b):
    return [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]


def dense_bernstein(interpolant):
    # SciPy's actual stored DOP853 recurrence, expanded exactly over Q.
    # y=0; for reverse F: y+=F; y*=x or(1-x); finally y+=y_old.
    axes=[]
    for axis in range(3):
        power=[Q()]
        for i,vector in enumerate(reversed(interpolant.F)):
            power[0]+=Q(float(vector[axis]))
            if i%2==0:power=[Q()]+power
            else:power=[power[0]]+[power[j]-power[j-1] for j in range(1,len(power))]+[-power[-1]]
        power[0]+=Q(float(interpolant.y_old[axis]));degree=len(power)-1
        axes.append([sum((power[i]*Q(math.comb(k,i),math.comb(degree,i))
                          for i in range(k+1)),Q()) for k in range(degree+1)])
    return list(map(list,zip(*axes,strict=True)))


def split_coefficients(values):
    levels=[values]
    while len(levels[-1])>1:
        row=levels[-1];levels.append([(a+b)/2 for a,b in zip(row,row[1:])])
    return [row[0] for row in levels],[row[-1] for row in reversed(levels)]


def exclude_face(controls,origin,face):
    vertices=[[Q(float(v)) for v in row] for row in face]
    a,b,c=vertices;normal=cross(sub(b,a),sub(c,a))
    assert dot(normal,normal)>0
    points=[add(origin,row) for row in controls]
    # Plane equality plus three inward edge halfspaces are necessary and
    # sufficient for membership in the closed source triangle.
    constraints=[[dot(sub(p,a),normal) for p in points]]
    for x,y in [(a,b),(b,c),(c,a)]:
        inward=cross(normal,sub(y,x))
        constraints.append([dot(sub(p,x),inward) for p in points])
    work=[(constraints,0)];maximum=0;nodes=0
    while work:
        rows,depth=work.pop();maximum=max(maximum,depth);nodes+=1
        if min(rows[0])>0 or max(rows[0])<0:continue
        if any(max(row)<0 for row in rows[1:]):continue
        if depth==28:return False,maximum,nodes
        halves=[split_coefficients(row) for row in rows]
        work.extend([([pair[0] for pair in halves],depth+1),([pair[1] for pair in halves],depth+1)])
    return True,maximum,nodes


def short_reference(case,max_step,tighter):
    origin=np.array(case['origin_ecef_m']);direction=np.array(case['initial_direction']);direction/=np.linalg.norm(direction)
    heights=np.array([case['profile_base_height_m']+x['height_m'] for x in case['layers']])
    refractivity=np.array([x['refractivity'] for x in case['layers']])
    state=np.r_[np.zeros(3),direction,0.];path=0.;stop=case['target_path_m']
    height,up=oracle.geometry(origin);layer=int(np.searchsorted(heights,height,side='right'))-1
    assert 0<=layer<len(heights)-1
    pieces=[];source_events=[]
    for _ in range(128):
        if layer<0:anchor=heights[0];value=refractivity[0];slope=0.;lower=-10000.;upper=heights[0]
        else:
            assert layer<len(heights)-1
            anchor=heights[layer];value=refractivity[layer];lower=anchor;upper=heights[layer+1]
            slope=(refractivity[layer+1]-value)/(upper-lower)
        def rhs(t,y):
            h,n=oracle.geometry(origin+y[:3]);excess=value+slope*(h-anchor);u=y[3:6]
            return np.r_[u,(n-u*(u@n))*slope/(1+excess),excess]
        def low(t,y):return oracle.geometry(origin+y[:3])[0]-lower
        def high(t,y):return oracle.geometry(origin+y[:3])[0]-upper
        low.direction=-1;high.direction=1;low.terminal=high.terminal=True
        result=solve_ivp(rhs,(path,stop),state,method='DOP853',dense_output=True,
                         rtol=5e-14 if tighter else 2e-12,
                         atol=([2e-9]*3+[5e-16]*3+[1e-11]) if tighter else ([2e-7]*3+[2e-14]*3+[1e-9]),
                         max_step=max_step,events=(low,high))
        assert result.success and result.t[-1]>path
        for segment in result.sol.interpolants:
            # Last interpolation interval may extend past terminal source
            # event; restrict its exact polynomial below by de Casteljau.
            physical_end=min(float(segment.t),float(result.t[-1]))
            controls=dense_bernstein(segment)
            fraction=(Q(physical_end)-Q(float(segment.t_old)))/Q(float(segment.h))
            # Stored h is the binary64 subtraction of endpoints, so exact
            # endpoint/h can differ slightly from1 even without event clipping.
            assert 0<fraction<Q(1)+Q(1,10**12)
            if fraction!=1:
                axes=list(zip(*controls,strict=True));trim=[]
                for axis in axes:
                    levels=[list(axis)]
                    while len(levels[-1])>1:
                        row=levels[-1];levels.append([(1-fraction)*a+fraction*b for a,b in zip(row,row[1:])])
                    trim.append([row[0] for row in levels])
                controls=list(map(list,zip(*trim,strict=True)))
            pieces.append((float(segment.t_old),physical_end,controls))
        path=float(result.t[-1]);state=result.y[:,-1]
        if result.status==0:break
        source_events.append(path);layer+=-1 if len(result.t_events[0]) else 1
    assert path==stop
    return state,pieces,source_events


def review(prefix):
    report=load(prefix+'.json');pixel=[762,945];row=next(r for r in report['samples'] if r['pixel']==pixel)
    assert row['terminal_words'][7]==2
    words=np.array(row['terminal_words'],dtype='<u4');launch=np.array(row['diagnostic_words'][4:7],dtype='<u4').view('<f4').astype(float)
    positive='artifacts/optical-positive-contact-packet-first';metadata=load(positive+'.json')
    faces=np.fromfile(positive+'-faces.bin',dtype='<f8').reshape(-1,3,3)
    mp.mp.dps=90;e={k:mp.mpf(f64(v)) for k,v in metadata['exact_f64_inputs'].items()}
    phi=mp.atan2(e['up_z'],e['north_z']);lam=mp.atan2(-e['east_x'],e['east_y'])
    E=[-mp.sin(lam),mp.cos(lam),mp.mpf(0)]
    U=[mp.cos(phi)*mp.cos(lam),mp.cos(phi)*mp.sin(lam),mp.sin(phi)]
    S=[mp.sin(phi)*mp.cos(lam),mp.sin(phi)*mp.sin(lam),-mp.cos(phi)]
    radius=e['a']/mp.sqrt(1-e['eccentricity_squared']*U[2]**2)
    exact_origin=[(radius+e['eye_height'])*U[0],(radius+e['eye_height'])*U[1],(radius*(1-e['eccentricity_squared'])+e['eye_height'])*U[2]]
    origin=np.array(list(map(float,exact_origin)));basis=np.array([[float(x) for x in c] for c in [E,U,S]]).T
    case=dict(report['fixture']['clear_fixture']['cases'][0]['canonical_inputs'])
    case.update(origin_ecef_m=origin.tolist(),initial_direction=(basis@launch).tolist(),target_path_m=float(words.view('<f4')[3]))
    # Match canonical-e2 geometry. The remaining binary64 inverse and ODE
    # arithmetic are independently converged, not an exact ODE enclosure.
    oracle.BETA2=1-float(e['eccentricity_squared'])
    runs=[];states=[]
    face_min=faces.min(axis=1);face_max=faces.max(axis=1);origin_q=list(map(Q,origin))
    for step,tighter in [(20.,False),(5.,True)]:
        state,pieces,events=short_reference(case,step,tighter);states.append(state)
        tests=nodes=depth=0;excluded_boxes=0;unresolved=[];union=set()
        for start,end,controls in pieces:
            lower=[min(p[k] for p in controls)+origin_q[k] for k in range(3)]
            upper=[max(p[k] for p in controls)+origin_q[k] for k in range(3)]
            lo=np.array([math.nextafter(float(x),-math.inf) for x in lower])
            hi=np.array([math.nextafter(float(x),math.inf) for x in upper])
            ids=np.flatnonzero(np.all(face_max>=lo,axis=1)&np.all(face_min<=hi,axis=1))
            excluded_boxes+=len(faces)-len(ids);union.update(map(int,ids))
            for face in ids:
                okay,d,n=exclude_face(controls,origin_q,faces[face]);tests+=1;nodes+=n;depth=max(depth,d)
                if not okay:unresolved.append({'face':int(face),'path_range':[start,end]})
        runs.append({'maximum_step_m':step,'tight':tighter,'segments':len(pieces),'source_events_m':events,
                     'all_source_faces':len(faces),'candidate_faces':sorted(union),'exact_triangle_segment_tests':tests,
                     'box_excluded_face_segments':excluded_boxes,'subdivision_nodes':nodes,'maximum_subdivision_depth':depth,
                     'unexcluded':unresolved,'endpoint_local_m':(basis.T@state[:3]).tolist(),
                     'dense_segments':[{'path_range':[a,b],'bernstein_displacement_m':[[str(x) for x in row] for row in c]} for a,b,c in pieces]})
    old=load('artifacts/optical-normal-scene-reference-selected.json');oldrow=next(r for r in old['samples'] if r['pixel']==pixel)
    oldvalues=np.array(oldrow['terminal_words'],dtype='<u4').view('<f4')
    return {'scope':'Finite independent physical-reference investigation of one suspected missed contact. All canonical source triangles are excluded from each exact retained DOP853 interpolation polynomial using exact rational Bernstein plane/edge tests and outward boxes. Two converged numerical trajectories agree; this is not a rigorous exact-ODE enclosure, proof about unpublished GPU intervals, or an approval of candidate clearance. No GPU ray resolves successfully.',
            'prefix':prefix,'pixel':pixel,'actual_launch_words':row['diagnostic_words'][4:7],
            'actual_launch_matches_legacy':row['diagnostic_words'][4:7]==oldrow['diagnostic_words'][4:7],
            'exact_f64_inputs':metadata['exact_f64_inputs'],'reference_case':case,
            'rounded_ideal_origin_error_m':float(mp.sqrt(sum((mp.mpf(float(x))-y)**2 for x,y in zip(origin,exact_origin,strict=True)))),
            'legacy_hit_path_m':float(oldvalues[3]),'candidate_failed_prefix_path_m':case['target_path_m'],
            'candidate_failed_prefix_position_difference_from_tight_reference_m':float(np.linalg.norm(words.view('<f4')[:3].astype(float)-basis.T@states[1][:3])),
            'reference_convergence_position_m':float(np.linalg.norm(states[0][:3]-states[1][:3])),
            'reference_convergence_direction':float(np.linalg.norm(states[0][3:6]-states[1][3:6])),
            'all_reference_polynomial_face_exclusions_complete':all(not run['unexcluded'] for run in runs),
            'inputs_sha256':{p:sha(p) for p in [prefix+'.json',prefix+'-readback.bin',positive+'.json',positive+'-faces.bin','scripts/verify_optical_dop853.py',__file__]},
            'runs':runs}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');args=parser.parse_args()
    result=review(args.prefix);output=Path(args.prefix+'-suspected-contact-review.json');assert not output.exists()
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k not in ('runs','reference_case','exact_f64_inputs','inputs_sha256')},indent=2))
    for row in result['runs']:print(json.dumps({k:v for k,v in row.items() if k!='dense_segments'}))
