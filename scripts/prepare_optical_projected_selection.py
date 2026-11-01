"""Select retained critical and largest changed rays for projected-RHS review."""
import hashlib
import json
from pathlib import Path
import numpy as np


def build():
    candidate='artifacts/optical-height-performance-projected-first'
    baseline='artifacts/optical-height-performance-fast-metric'
    for suffix in ['-uniform.bin','-view.bin']:
        assert Path(candidate+suffix).read_bytes()==Path(baseline+suffix).read_bytes()
    a=np.fromfile(candidate+'-readback.bin',dtype='<u4').reshape(-1,12)
    b=np.fromfile(baseline+'-readback.bin',dtype='<u4').reshape(-1,12)
    assert a.shape==b.shape==(1920*1080,12)
    assert np.array_equal(a[:,7],b[:,7]) and np.array_equal(a[:,10],b[:,10])
    af,bf=a.view('<f4'),b.view('<f4')
    old=json.loads(Path('artifacts/optical-height-performance-fast-bounds-selection.json').read_text())
    samples=[(r['label'],r['pixel']) for r in old['samples']]
    maxima={}
    for label,columns,status in [('maximum_completed_endpoint_delta',slice(0,3),0),
                                  ('maximum_completed_direction_delta',slice(4,7),0),
                                  ('maximum_completed_path_delta',slice(3,4),0),
                                  ('maximum_lower_domain_prefix_path_delta',slice(3,4),2)]:
        assert np.isfinite(af[a[:,7]==status,columns]).all()
        assert np.isfinite(bf[b[:,7]==status,columns]).all()
        differences=np.linalg.norm(af[:,columns].astype(np.float64)-bf[:,columns],axis=1)
        differences[a[:,7]!=status]=-1
        index=int(np.argmax(differences));pixel=[index%1920,index//1920]
        maxima[label]={'pixel':pixel,'difference':float(differences[index]),'status':status}
        if pixel not in [p for _,p in samples]:samples.append((label,pixel))
    records=[]
    for label,pixel in samples:
        index=pixel[1]*1920+pixel[0]
        records.append({'label':label,'pixel':pixel,'raw_terminal_words':a[index].tolist(),'baseline_terminal_words':b[index].tolist()})
    return {'scope':'Original nine critical rays plus largest completed p/u/path changes and a separate lower-domain prefix path change. Raw differences select probes, not accuracy; exact camera readback launch and independent reference required.',
            'candidate':candidate,'baseline':baseline,'dimensions':[1920,1080],
            'source_raw_sha256':{name:hashlib.sha256(Path(prefix+'-readback.bin').read_bytes()).hexdigest() for name,prefix in [('candidate',candidate),('baseline',baseline)]},
            'maxima':maxima,'samples':records}


if __name__=='__main__':
    output=Path('artifacts/optical-height-performance-projected-selection.json');assert not output.exists(),'retain earlier evidence'
    result=build();output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'count':len(result['samples']),'maxima':result['maxima']},indent=2))
