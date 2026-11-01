"""Audit selected canonical-scene bytes, tokens, launches and explicit outcomes."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import numpy as np


def read(prefix):
    report=json.loads(Path(prefix+'.json').read_text())
    assert json.loads(Path(prefix+'-input-stability.json').read_text(encoding='utf-8-sig'))['inputs_unchanged_through_run']
    for item in json.loads(Path(prefix+'-outputs.json').read_text(encoding='utf-8-sig')):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest()==item['sha256'],item['path']
    assert report['selected'] and not report['timed']
    raw=Path(prefix+'-readback.bin').read_bytes()
    n=report['record_count'];end=16+48*n
    assert len(raw)==report['output_bytes']==16+80*n
    assert report['terminal_bytes']==end and report['diagnostic_bytes_per_record']==32
    header=np.frombuffer(raw[:16],dtype='<u4')
    assert header.tolist()==report['header']==report['expected_header']
    assert header[:2].tolist()==report['dimensions'] and report['header_matches']
    terminal=np.frombuffer(raw[16:end],dtype='<u4').reshape(n,12)
    details=np.frombuffer(raw[end:],dtype='<u4').reshape(n,8)
    assert np.isfinite(terminal.view('<f4')[:,:7]).all()
    assert np.isfinite(terminal.view('<f4')[:,8:11]).all()
    assert np.isfinite(details[:,4:7].copy().view('<f4')).all()
    assert np.all(details[:,7]==0)
    counts=Counter(map(int,terminal[:,7]))
    assert {str(k):v for k,v in counts.items()}==report['raw_status_counts']
    assert len(report['samples'])==n
    seen=set()
    for row in report['samples']:
        index=row['record_index'];assert index not in seen;seen.add(index)
        assert row['terminal_words']==terminal[index].tolist()
        assert row['diagnostic_words']==details[index].tolist()
    return report,terminal,details


def review(candidate,reference):
    a,aw,ad=read(candidate);b,bw,bd=read(reference)
    assert not a['reference'] and b['reference']
    assert a['record_count']==b['record_count']
    assert [x['pixel'] for x in a['samples']]==[x['pixel'] for x in b['samples']]
    for suffix in ['-primary.bin','-field.bin','-frames.bin','-certificates.bin']:
        assert Path(candidate+suffix).read_bytes()==Path(reference+suffix).read_bytes(),suffix
    if a.get('reference_reuse',False) or a.get('reference_carry',False):
        assert Path(candidate+'-geometry.bin').read_bytes()==Path(reference+'-geometry.bin').read_bytes()
    else:
        assert Path(candidate+'-geometry.bin').read_bytes()==Path('artifacts/optical-normal-scene-packet-geometry.bin').read_bytes()
    assert Path(reference+'-geometry.bin').read_bytes()==Path('artifacts/optical-requested-stellar-sparse-chain-geometry.bin').read_bytes()
    archived=json.loads(Path('artifacts/optical-selected-launch-review.json').read_text())
    old=next(run for run in archived['runs'] if run['variant']=='baseline')
    old={tuple(row['pixel']):row for row in old['rows']}
    af,bf=aw.view('<f4'),bw.view('<f4')
    rows=[];old_rows=[]
    for ar,br in zip(a['samples'],b['samples'],strict=True):
        i=ar['record_index'];j=br['record_index'];assert i==j
        pixel=ar['pixel'];same_status=int(aw[i,7])==int(bw[i,7])
        row={'pixel':pixel,'candidate_status':int(aw[i,7]),'reference_status':int(bw[i,7]),
             'candidate_material_owner_word':int(aw[i,11]),'reference_material_owner_word':int(bw[i,11]),
             'candidate_attempts':int(ad[i,0]),'source_roots':int(ad[i,1]),'reason':int(ad[i,2]),'scene_visits':int(ad[i,3]),
             'candidate_path_m':float(af[i,3]),'reference_path_m':float(bf[i,3]),
             'launch_words_equal':bool(np.array_equal(ad[i,4:7],bd[i,4:7])),
             'candidate_launch_words':ad[i,4:7].tolist(),'reference_launch_words':bd[i,4:7].tolist(),
             'candidate_terminal_words':aw[i].tolist(),'reference_terminal_words':bw[i].tolist()}
        if same_status and aw[i,7] in (0,1):
            row['same_status_output_deltas_not_accuracy']={'position_m':float(np.linalg.norm(af[i,:3].astype(float)-bf[i,:3])),
                'direction':float(np.linalg.norm(af[i,4:7].astype(float)-bf[i,4:7])),
                'path_m':float(abs(float(af[i,3])-float(bf[i,3])))}
        if tuple(pixel) in old:
            prior=old[tuple(pixel)]
            old_rows.append({'pixel':pixel,'reference_terminal_words_equal':bw[i].tolist()==prior['terminal_words'],
                             'reference_launch_words_equal':bd[i,4:7].tolist()==prior['launch_words']})
        rows.append(row)
    return {'scope':'Selected current-eye diagnostic only. Same-input raw/token/source checks, exact launch records and explicit classes are preserved. Original midpoint output differences are not accuracy evidence; unresolved prefixes are not hits or accurate atmospheric exits. No full-image/native or scene-solver adoption.',
            'candidate':candidate,'reference':reference,'all_output_hashes_verified':True,'current_headers_match':True,
            'records':rows,'candidate_counts':dict(Counter(map(int,aw[:,7]))),'reference_counts':dict(Counter(map(int,bw[:,7]))),
            'candidate_reasons':dict(Counter(map(int,ad[:,2]))),'launch_word_differences':int(np.count_nonzero(ad[:,4:7]!=bd[:,4:7])),
            'changed_statuses':int(np.count_nonzero(aw[:,7]!=bw[:,7])),
            'reference_reuse':a.get('reference_reuse',False),
            'reference_carry':a.get('reference_carry',False),
            'terminal_word_differences':int(np.count_nonzero(aw!=bw)),
            'diagnostic_word_differences':int(np.count_nonzero(ad!=bd)),
            'complete_readback_bytes_equal':Path(candidate+'-readback.bin').read_bytes()==Path(reference+'-readback.bin').read_bytes(),
            'archived_original_context_checks':old_rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('candidate');parser.add_argument('reference');args=parser.parse_args()
    result=review(args.candidate,args.reference)
    output=Path(args.candidate+'-independent-review.json');assert not output.exists(),'retain earlier evidence'
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k not in ('records','archived_original_context_checks')},indent=2))
    for row in result['records']:
        print(json.dumps({k:v for k,v in row.items() if 'words' not in k}))
