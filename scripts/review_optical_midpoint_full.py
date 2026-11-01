"""Complete raw same-input midpoint comparison; isolated GPU durations only."""
import argparse
from collections import Counter
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import numpy as np


def load(prefix):
    report=json.loads(Path(prefix+'.json').read_text())
    assert not report['selected'] and report['timed'] and report['record_count']==1920*1080
    assert json.loads(Path(prefix+'-input-stability.json').read_text(encoding='utf-8-sig'))['inputs_unchanged_through_run']
    for entry in json.loads(Path(prefix+'-outputs.json').read_text(encoding='utf-8-sig')):
        assert hashlib.sha256(Path(entry['path']).read_bytes()).hexdigest()==entry['sha256']
    raw=np.fromfile(prefix+'-readback.bin',dtype='<u4');n=report['record_count']
    assert len(raw)*4==report['output_bytes']==16+80*n
    assert raw[:4].tolist()==report['expected_header']==report['header']
    terminals=raw[4:4+12*n].reshape(n,12);details=raw[4+12*n:].reshape(n,8)
    assert np.isfinite(terminals.view('<f4')[:,:7]).all()
    assert np.isfinite(terminals.view('<f4')[:,8:11]).all()
    assert np.isfinite(details[:,4:7].copy().view('<f4')).all()
    counts={str(k):v for k,v in Counter(map(int,terminals[:,7])).items()}
    assert counts==report['raw_status_counts']
    for sample in report['samples']:
        i=sample['record_index'];assert terminals[i].tolist()==sample['terminal_words'];assert details[i].tolist()==sample['diagnostic_words']
    kept=[];timers=[];previous=-1
    assert len(report['iterations'])==8
    for i,row in enumerate(report['iterations']):
        begin,end=row['raw_timestamps'];assert previous<begin<end;previous=end
        assert row['iteration']==i and row['warmup']==(i<3)
        ms=Decimal(end-begin)*Decimal(str(row['timestamp_period_ns']))/Decimal(10**6)
        assert abs(ms/1000-Decimal(str(row['gpu_seconds'])))<Decimal('1e-15')
        timers.append(str(ms))
        if i>=3:kept.append(ms)
    return report,terminals,details,{'all_ms':timers,'retained_ms':list(map(str,kept)),'retained_median_ms':str(sorted(kept)[2])}


def review(candidate,baseline):
    a,aw,ad,at=load(candidate);b,bw,bd,bt=load(baseline)
    for suffix in ['-primary.bin','-field.bin','-geometry.bin','-frames.bin','-certificates.bin']:
        assert Path(candidate+suffix).read_bytes()==Path(baseline+suffix).read_bytes(),suffix
    changed=np.flatnonzero(np.any(aw!=bw,axis=1)|np.any(ad!=bd,axis=1));rows=[]
    af,bf=aw.view('<f4'),bw.view('<f4');worst={}
    for i in changed:
        i=int(i);pixel=[i%1920,i//1920]
        row={'pixel':pixel,'candidate_words':aw[i].tolist(),'baseline_words':bw[i].tolist(),
             'candidate_details':ad[i].tolist(),'baseline_details':bd[i].tolist(),
             'candidate_status':int(aw[i,7]),'baseline_status':int(bw[i,7])}
        if aw[i,7]==bw[i,7] and aw[i,7] in [0,1]:
            deltas={'position_m':float(np.linalg.norm(af[i,:3].astype(float)-bf[i,:3])),
                    'direction':float(np.linalg.norm(af[i,4:7].astype(float)-bf[i,4:7])),
                    'path_m':abs(float(af[i,3])-float(bf[i,3]))}
            row['same_status_differences_not_accuracy']=deltas
            if aw[i,7]==0:
                for key,value in deltas.items():
                    if key not in worst or value>worst[key]['difference']:worst[key]={'pixel':pixel,'difference':value,'record_index':i}
        rows.append(row)
    return {'scope':'Complete same-upload primary midpoint terminal comparison and unique timestamp durations; no shading, stellar, UI, presentation, native FPS, or accuracy inference from baseline agreement.',
            'candidate':candidate,'baseline':baseline,'all_input_buffers_identical':True,'all_output_hashes_verified':True,
            'candidate_timing':at,'baseline_timing':bt,'candidate_status_counts':a['raw_status_counts'],'baseline_status_counts':b['raw_status_counts'],
            'pixels':len(aw),'changed_pixels':len(rows),'terminal_word_differences':int(np.count_nonzero(aw!=bw)),
            'diagnostic_word_differences':int(np.count_nonzero(ad!=bd)),
            'status_differences':int(np.count_nonzero(aw[:,7]!=bw[:,7])),
            'material_owner_differences':int(np.count_nonzero(aw[:,11]!=bw[:,11])),
            'normal_word_differences':int(np.count_nonzero(aw[:,8:11]!=bw[:,8:11])),
            'worst_matched_exit_differences_not_accuracy':worst,'changed_records':rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('candidate');parser.add_argument('baseline');args=parser.parse_args()
    output=Path(args.candidate+'-independent-review.json');assert not output.exists()
    result=review(args.candidate,args.baseline);output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='changed_records'},indent=2))
