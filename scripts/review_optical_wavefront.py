"""Independent raw continuation/count/timing comparison; no runtime inference."""
import argparse
from collections import Counter
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import statistics

import numpy as np


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def sha(path):
    digest=hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''):digest.update(chunk)
    return digest.hexdigest()


def timings(report):
    if not report['timed']:
        assert not report['iterations']
        return None
    assert report['strictly_increasing_unique_timestamp_pairs']
    rows=report['iterations'];assert len(rows)==8
    previous=0;converted=[]
    for i,row in enumerate(rows):
        assert row['iteration']==i and row['warmup']==(i<3)
        begin,end=row['raw_timestamps'];assert previous<begin<end;previous=end
        period=Decimal(str(row['timestamp_period_ns']));assert period>0
        seconds=Decimal(end-begin)*period*Decimal('1e-9')
        assert abs(seconds-Decimal(str(row['gpu_seconds'])))<=Decimal('1e-15')
        converted.append(seconds*1000)
    kept=converted[3:]
    return {'all_gpu_ms':list(map(float,converted)),'retained_gpu_ms':list(map(float,kept)),
            'retained_median_gpu_ms':float(statistics.median(kept)),
            'scope':'Complete ordered compute sequence, including all fixed empty slices and finalizer; '
                    'progress clear immediately precedes timestamped pass. Compilation, readback, '
                    'rendering, presentation and native motion excluded.'}


def read(prefix,wave):
    report=load(prefix+'.json')
    assert load(prefix+'-input-stability.json')['inputs_unchanged_through_run']
    for output in load(prefix+'-outputs.json'):
        assert sha(output['path'])==output['sha256'],output['path']
    count=report['record_count'];assert count>0
    raw=np.fromfile(prefix+'-readback.bin',dtype='<u4')
    assert raw.nbytes==report['output_bytes']==16+80*count
    assert report['terminal_bytes']==16+48*count and report['diagnostic_bytes_per_record']==32
    assert raw[:4].tolist()==report['header']==report['expected_header'] and report['header_matches']
    terminal=raw[4:4+12*count].reshape(count,12)
    detail=raw[4+12*count:].reshape(count,8)
    tf=terminal.view('<f4');df=detail.view('<f4')
    assert np.isfinite(tf[:,:7]).all() and np.isfinite(tf[:,8:11]).all() and np.isfinite(df[:,4:]).all()
    assert set(map(int,np.unique(terminal[:,7])))<={0,1,2,3}
    status={str(k):v for k,v in Counter(map(int,terminal[:,7])).items()}
    assert status==report['raw_status_counts']
    if not report['selected']:assert count==report['dimensions'][0]*report['dimensions'][1]
    state=progress=None
    if wave:
        assert not np.any(detail[:,1:4]) and not np.any(detail[:,7])
        assert np.all(detail[:,0]<=4096)
        state=np.fromfile(prefix+'-states.bin',dtype='<u4').reshape(count,12)
        progress=np.fromfile(prefix+'-progress.bin',dtype='<u4')
        assert state.nbytes==report['state_bytes']==48*count
        assert progress.nbytes==report['progress_bytes']==16 and progress.tolist()==report['progress_words']
        if report['reference']:
            assert not np.any(state) and not np.any(progress)
            assert report['steps']==[{'entry':'normal_scene','dispatch':[(count+31)//32,1,1],'repetitions':1}]
        else:
            slice_size=report['slice_attempts'];assert slice_size in [1,8,32,128]
            expected_steps=[
                {'entry':'wave_initialize','dispatch':[(count+31)//32,1,1],'repetitions':1},
                {'entry':'wave_slice','dispatch':[(count+31)//32,1,1],'repetitions':4096//slice_size}]
            if report.get('reduce_bookkeeping',False):
                blocks=(count+255)//256
                assert report['partial_layout']=={'blocks':blocks,'bytes':16*blocks}
                expected_steps.extend([
                    {'entry':'wave_validate','dispatch':[(blocks+31)//32,1,1],'repetitions':1},
                    {'entry':'wave_finalize_reduced','dispatch':[1,1,1],'repetitions':1}])
                partials=np.fromfile(prefix+'-partials.bin',dtype='<u4').reshape(blocks,4)
                # Recompute independently from every raw state metadata row;
                # do not infer correctness from final reported progress alone.
                for block in range(blocks):
                    first=256*block;stop=min(count,first+256);metadata=state[first:stop,9:]
                    invalid=(metadata[:,0]>4096)|(metadata[:,1]!=np.arange(first,stop))|(metadata[:,2]>1)
                    expected=[int(np.count_nonzero(metadata[:,2]==0)),int(np.count_nonzero(metadata[:,2]==1)),
                              int(metadata[:,0].max()),int(np.count_nonzero(invalid))]
                    assert partials[block].tolist()==expected
                assert progress.tolist()==[int(partials[:,0].astype(np.uint64).sum()),
                                            int(partials[:,1].astype(np.uint64).sum()),int(partials[:,2].max()),
                                            int(partials[:,3].astype(np.uint64).sum())]
            else:
                expected_steps.append({'entry':'wave_finalize','dispatch':[1,1,1],'repetitions':1})
            assert report['steps']==expected_steps
            assert np.array_equal(state[:,9],detail[:,0])
            assert np.array_equal(state[:,10],np.arange(count,dtype='<u4'))
            assert np.all(state[:,11]==1)
            assert progress.tolist()==[0,count,int(detail[:,0].max()),0]
            sf=state.view('<f4');assert np.isfinite(sf[:,:9]).all()
            assert np.all(sf[:,3]>=sf[:,8]) and np.all(sf[:,8]>=0)
            assert np.all(sf[terminal[:,7]!=3,7]>0)
            unresolved=terminal[:,7]==2
            assert np.array_equal(terminal[unresolved,:4],state[unresolved,:4])
            assert np.array_equal(terminal[unresolved,4:7],state[unresolved,4:7])
    for sample in report['samples']:
        i=sample['record_index']
        assert sample['terminal_words']==terminal[i].tolist()
        assert sample['diagnostic_words']==detail[i].tolist()
        if wave:assert sample['state_words']==state[i].tolist()
    return report,terminal,detail,state,progress


def comparisons(a,b):
    assert a.shape==b.shape
    changed=np.nonzero(np.any(a!=b,axis=1))[0]
    return {'words_compared':int(a.size),'word_differences':int(np.count_nonzero(a!=b)),
            'changed_records':[{'record_index':int(i),'before_words':b[i].tolist(),'after_words':a[i].tolist()}
                               for i in changed]}


def distribution(values):
    values=np.sort(values)
    return {**{f'p{p}':int(values[max(0,(len(values)*p+99)//100-1)]) for p in [50,90,99]},
            'minimum':int(values[0]),'maximum':int(values[-1]),
            'total':int(values.astype(np.uint64).sum())}


def review(candidate,reference,original,continuation_baseline=None):
    ar,a,ad,state,progress=read(candidate,True)
    br,b,bd,_,_=read(reference,True)
    cr,c,cd,_,_=read(original,False)
    assert not ar['reference'] and br['reference']
    assert ar['dimensions']==br['dimensions']==cr['dimensions']
    assert ar['record_count']==br['record_count']==cr['record_count']
    assert ar['selected']==br['selected']==cr['selected']
    inputs={}
    for kind in ['primary','field','geometry','frames','certificates']:
        hashes=[sha(p+'-'+kind+'.bin') for p in [candidate,reference,original]]
        assert hashes[0]==hashes[1]==hashes[2],kind
        inputs[kind]=hashes[0]
    assert [s['pixel'] for s in ar['samples']]==[s['pixel'] for s in br['samples']]==[s['pixel'] for s in cr['samples']]
    counts=ad[:,0];number=len(counts)
    count_differences=np.nonzero(counts!=bd[:,0])[0]
    rounded_groups=np.pad(counts.astype(np.uint64),(0,(-number)%32)).reshape(-1,32)
    group_max=rounded_groups.max(axis=1)
    selected_rows=[]
    if ar['selected']:
        selected_rows=[{'record_index':i,'pixel':ar['samples'][i]['pixel'],'attempts':int(counts[i]),
                        'status':int(a[i,7]),'state_words':state[i].tolist()} for i in range(number)]
    additional=None
    if continuation_baseline:
        previous,pt,pd,ps,pp=read(continuation_baseline,True)
        assert not previous['reference']
        assert ar['dimensions']==previous['dimensions'] and ar['record_count']==previous['record_count']
        assert ar['selected']==previous['selected']
        assert [s['pixel'] for s in ar['samples']]==[s['pixel'] for s in previous['samples']]
        for kind in inputs:assert sha(continuation_baseline+'-'+kind+'.bin')==inputs[kind]
        additional={'prefix':continuation_baseline,'candidate_slice_attempts':ar['slice_attempts'],
                    'baseline_slice_attempts':previous['slice_attempts'],
                    'terminal':comparisons(a,pt),'all_detail':comparisons(ad,pd),
                    'all_state':comparisons(state,ps),'progress_words_identical':np.array_equal(progress,pp),
                    'timing':timings(previous)}
    return {'scope':'Independent exact-word comparison of original midpoint trace, counted full trace, and '
                    'fixed-index complete-attempt continuation. Header/progress/state checks prove this retained '
                    'dispatch chain completed; they do not infer production/native acceptance or GPU arithmetic '
                    'equivalence for other scenes. Terminal context changes and counts are retained separately. '
                    'Attempt distributions describe this workload, not guaranteed compaction or lane cost.',
            'candidate':candidate,'counted_reference':reference,'original':original,'records':number,
            'all_frozen_flags_output_hashes_verified':True,'all_five_upload_hashes_identical':inputs,
            'candidate_progress_words':progress.tolist(),'all_final_phases_terminal':True,
            'reduced_raw_partial_records_recomputed':ar.get('partial_layout') if ar.get('reduce_bookkeeping') else None,
            'all_output_record_ids_and_state_counts_match':True,
            'candidate_vs_counted_terminal':comparisons(a,b),
            'counted_vs_original_terminal':comparisons(b,c),
            'candidate_vs_original_terminal':comparisons(a,c),
            'candidate_vs_counted_launch':comparisons(ad[:,4:],bd[:,4:]),
            'counted_vs_original_launch':comparisons(bd[:,4:],cd[:,4:]),
            'candidate_count_differences':[{'record_index':int(i),'candidate':int(counts[i]),
                                          'reference':int(bd[i,0])} for i in count_differences],
            'candidate_status_counts':ar['raw_status_counts'],'reference_status_counts':br['raw_status_counts'],
            'attempt_distribution':distribution(counts),
            'attempt_distribution_by_status':{str(k):distribution(counts[a[:,7]==k]) for k in np.unique(a[:,7])},
            'zero_attempt_terminals':int(np.count_nonzero(counts==0)),
            'attempt4096_terminals':int(np.count_nonzero(counts==4096)),
            'fixed32_group_max_attempt_sum':int(group_max.sum()),
            'fixed32_group_slot_attempt_sum':int(group_max.sum())*32,
            'candidate_timing':timings(ar),'counted_reference_timing':timings(br),
            'continuation_baseline_comparison':additional,
            'selected_records':selected_rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('candidate');parser.add_argument('reference');parser.add_argument('original');parser.add_argument('--continuation-baseline');args=parser.parse_args()
    target=Path(args.candidate+'-independent-review.json');assert not target.exists()
    result=review(args.candidate,args.reference,args.original,args.continuation_baseline)
    target.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ['selected_records']},indent=2))
