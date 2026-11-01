"""Raw selected continuation audit; completion is distinct from ray resolution."""
import argparse
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path

import numpy as np

from review_optical_normal_scene import read


def load(path):return json.loads(Path(path).read_text(encoding='utf-8-sig'))
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def review(prefix,reference):
    report,terminal,detail=read(prefix)
    old,old_terminal,old_detail=read(reference)
    n=report['record_count'];assert n==23 and report['split_certificate']
    sources=load(prefix+'-sources.json');assert sources['compiler_inputs_unchanged']
    before={r['path']:r['sha256'] for r in sources['files']}
    after={r['path']:r['sha256'] for r in load(prefix+'-input-stability.json')['after_run']}
    manifest_only=[p for p in before if p not in after]
    assert manifest_only==['.cache\\optical-normal-selected-probe.exe']
    assert all(before[p]==digest for p,digest in after.items())
    lifecycle=load(prefix+'-lifecycle.json');assert lifecycle['exit_code']==0 and not lifecycle['timed_out']
    for suffix in ['primary','field','frames','certificates']:
        assert Path(prefix+'-'+suffix+'.bin').read_bytes()==Path(reference+'-'+suffix+'.bin').read_bytes(),suffix
    assert Path(prefix+'-geometry.bin').read_bytes()==Path('artifacts/optical-normal-scene-packet-geometry.bin').read_bytes()
    assert [r['pixel'] for r in report['samples']]==[r['pixel'] for r in old['samples']]
    assert np.array_equal(detail[:,4:7],old_detail[:,4:7])
    state=np.fromfile(prefix+'-states.bin',dtype='<u4').reshape(n,32,4);sf=state.view('<f4')
    piece=np.fromfile(prefix+'-pieces.bin',dtype='<u4').reshape(n,20,4);pf=piece.view('<f4')
    progress=np.fromfile(prefix+'-progress.bin',dtype='<u4')
    assert state.nbytes==report['state_bytes']==n*512 and piece.nbytes==report['piece_bytes']==n*320
    assert progress.nbytes==report['progress_bytes']==16
    phase=state[:,7,3];attempt=state[:,7,1]
    recomputed=[int(np.count_nonzero(phase!=4)),int(np.count_nonzero(phase==4)),int(attempt.max()),
                int(np.count_nonzero((state[:,12,2]!=np.arange(n))|(attempt>4096)|(phase>4)))]
    assert progress.tolist()==report['progress_words']==recomputed
    assert report['header_matches']==(recomputed[0]==0 and recomputed[1]==n and recomputed[3]==0)
    assert np.array_equal(state[:,12,2],np.arange(n)) and np.all(attempt<=4096) and np.all(phase<=4)
    assert not np.any(state[:,27:])
    for slot in [5,6,23,24,25,26]:
        reserved=[3] if slot!=23 else [1,2,3]
        assert not np.any(state[:,slot,reserved])
    float_slots=[0,1,2,3,4,5,6,9,10,11,13,14,15,17,18,19,20,21,23,25,26]
    assert np.isfinite(sf[:,float_slots]).all() and np.isfinite(sf[:,16,:3]).all()
    assert np.all(state[:,12,0]<=1) and np.all(np.isin(state[:,12,1].view('<i4'),[-1,0,1]))
    assert np.all(state[:,24,0]<=2)
    assert np.array_equal(attempt,detail[:,0]) and np.array_equal(state[:,8,0],detail[:,1])
    assert np.array_equal(state[:,12,3],detail[:,2]) and np.array_equal(state[:,8,3],detail[:,3])
    field=np.fromfile(prefix+'-field.bin',dtype='<u4').reshape(-1,4);ff=field.view('<f4')
    cert=np.fromfile(prefix+'-certificates.bin',dtype='<u4').reshape(-1,4)
    count=int(ff[1,3]);source_start=int(cert[1,1]);rows=[]
    for i,row in enumerate(report['samples']):
        status=int(terminal[i,7]);owner=int(state[i,7,0]);assert owner<=count
        if phase[i]==4 and status in (0,2):
            # nc_result publishes this current prefix verbatim. Contact has a
            # different path and state and intentionally does not use this gate.
            assert terminal[i,3]==state[i,0,3] and np.array_equal(terminal[i,4:7],state[i,1,:3])
        pairs=np.flatnonzero(np.all(field[5:5+count,:2]==state[i,3,2:],axis=1))
        if len(pairs):
            assert len(pairs)==1;j=int(pairs[0]);assert owner in (j,j+1)
            assert state[i,4,0]==field[5+j,2]
            assert state[i,4,1]==(field[5+owner-1,3] if 0<owner<count else 0)
            assert np.array_equal(state[i,4,2:],cert[source_start+j,:2])
            if owner==j+1:
                assert not np.any(state[i,5,:3])
                expected=field[133+j,:3].copy();expected[:2]^=np.uint32(0x80000000)
                assert np.array_equal(state[i,6,:3],expected)
            else:
                assert not np.any(state[i,6,:3])
                assert np.array_equal(state[i,5,:3],field[133+(j-1 if j else count-1),:3])
        else:
            assert np.array_equal(state[i,3,2:],field[0,2:])
            assert np.array_equal(state[i,4,:2],field[1,:2])
            assert np.array_equal(state[i,4,2:],cert[4,:2])
        pieces,index=map(int,state[i,22,:2]);assert pieces in (0,1,2,4,8,16,32,64)
        assert index<=pieces
        valid=int(piece[i,19,3]);assert valid in (0,1)
        if valid:
            published_id,published_index,published_count=map(int,piece[i,19,:3])
            assert published_id==i and published_count in (1,2,4,8,16,32,64) and published_index<published_count
            assert pf[i,0,:3].tolist()==[published_index/published_count,(published_index+1)/published_count,float(published_count)]
            assert np.isfinite(pf[i,:19]).all()
            for slot in [3,5,7,9,11,13,15]:assert np.all(pf[i,slot,:3]<=pf[i,slot+1,:3])
            assert np.all(pf[i,17:19,:3]>=0)
            assert not np.any(piece[i,1:19,3])
        reason=int(detail[i,2]);cause='not classified'
        if status==2 and reason==8:
            assert phase[i]==4 and state[i,24,0]==2
            assert state[i,16,3]==1 and 0<sf[i,14,3]<=1
            assert 0<=sf[i,15,3]<=sf[i,23,0]
            assert 0<sf[i,9,3]<=np.float32(.001)
            assert np.all(terminal[i,8:]==0)
            cause=('first partition admission failed; prior piece buffer is stale'
                   if pieces==0 else 'partition admitted; final current-piece certificate or scene query remained uncertain')
        rows.append({'pixel':row['pixel'],'status':status,'reason':reason,'phase':int(phase[i]),
                     'attempts':int(attempt[i]),'owner':owner,'path_m':float(sf[i,0,3]),
                     'height_attempts':int(state[i,8,1]),'height_accepted':int(state[i,8,2]),
                     'source_roots':int(state[i,8,0]),'scene_visits':int(state[i,8,3]),
                     'pieces':pieces,'piece_index':index,'queries':int(state[i,22,2]),'certificates':int(state[i,22,3]),
                     'last_delta_path_m':float(sf[i,9,3]),'last_numerical_error_m':float(sf[i,15,3]),
                     'last_tolerance_m':float(sf[i,23,0]),'last_event_kind':int(state[i,24,0]),
                     'cause_scope':cause,'legacy_status':int(old_terminal[i,7]),
                     'legacy_path_m':float(old_terminal[i].view('<f4')[3]),
                     'terminal_words':terminal[i].tolist(),'diagnostic_words':detail[i].tolist(),
                     'state_words':state[i].reshape(-1).tolist(),'last_piece_words':piece[i].reshape(-1).tolist()})
    return {'scope':'Selected split-stage continuation evidence only. Exact raw state, publication, source ownership, launch and immutable-input checks. A current header means all records terminated, including unresolved outcomes; it does not mean valid sky/contact solutions. Stale last-piece bytes are not associated with the final curve without a generation tag. No attribution to a particular inner scene failure, exact-ODE accuracy, runtime adoption, or GPU/native throughput is inferred.',
            'candidate':prefix,'reference':reference,'all_output_hashes_verified':True,'frozen_inputs_verified':True,
            'manifest_only_not_in_after_run_guard':manifest_only,
            'auditor_sha256':sha(__file__),'input_report_sha256':sha(prefix+'.json'),
            'progress_words':recomputed,'statuses':dict(Counter(map(int,terminal[:,7]))),
            'reasons':dict(Counter(map(int,detail[:,2]))),'all_launch_words_equal_legacy':True,
            'phase_counts':dict(Counter(map(int,phase))),'resolved_rays':int(np.count_nonzero(np.isin(terminal[:,7],[0,1]))),
            'last_partition_not_admitted':int(np.count_nonzero(state[:,22,0]==0)),
            'last_partition_admitted_but_uncertainty_origin_not_tagged':int(np.count_nonzero(state[:,22,0]>0)),
            'path_range_m':[float(sf[:,0,3].min()),float(sf[:,0,3].max())],
            'pipelines_compilation_seconds_not_gpu_timing':report['pipelines'],
            'records':rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('prefix');parser.add_argument('--reference',default='artifacts/optical-normal-scene-reference-selected');args=parser.parse_args()
    result=review(args.prefix,args.reference);output=Path(args.prefix+'-independent-review.json');assert not output.exists()
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k!='records'},indent=2))
