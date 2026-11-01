"""Canonical DOP853 checks at the three largest matched-exit output changes."""
import json
from pathlib import Path
import numpy as np
from verify_optical_dop853 import geometry,integrate,differences


def audit(prefix):
    checked=json.loads(Path(prefix+'-independent-review.json').read_text())
    original=json.loads(Path('artifacts/optical-height-trace-conditioning.json').read_text())['fixture']['cases'][0]['canonical_inputs']
    origin=np.array(original['origin_ecef_m']);_,up=geometry(origin)
    east=np.array([-origin[1],origin[0],0.]);east/=np.linalg.norm(east)
    basis=np.column_stack((east,up,-np.cross(up,east)))
    selected={tuple(row['pixel']) for row in checked['worst_matched_exit_differences_not_accuracy'].values()}
    rows=[]
    for row in checked['changed_records']:
        if tuple(row['pixel']) not in selected:continue
        assert row['candidate_status']==row['baseline_status']==0
        assert row['candidate_details'][4:7]==row['baseline_details'][4:7]
        launch=np.array(row['candidate_details'][4:7],dtype='<u4').view('<f4').astype(float)
        case=dict(original,initial_direction=(basis@launch).tolist());case.pop('target_path_m',None)
        coarse=integrate(case,False);reference=integrate(case,True)
        result={'pixel':row['pixel'],'launch_words':row['candidate_details'][4:7],
                'canonical_inputs':case,'reference':reference,'reference_convergence':differences(coarse,reference)}
        for variant in ['baseline','candidate']:
            value=np.array(row[variant+'_words'],dtype='<u4').view('<f4').astype(float)
            result[variant+'_canonical_difference']={
                'position_m':float(np.linalg.norm(origin+basis@value[:3]-reference['endpoint_ecef_m'])),
                'direction':float(np.linalg.norm(basis@value[4:7]-reference['exit_direction'])),
                'path_m':abs(float(value[3])-reference['geometric_path_m'])}
        rows.append(result)
        print(json.dumps({k:v for k,v in result.items() if k not in ['canonical_inputs','reference']}),flush=True)
    assert len(rows)==len(selected)==3
    return {'scope':'Three finite atmospheric-exit checks selected by largest same-status raw output changes. DOP853 uses exact readback local launch transformed through independently reconstructed canonical E/up/−N and normalized by the reference. Canonical source/coordinate packing discrepancies remain in the errors. This is not a whole-image accuracy bound or scene-contact oracle.',
            'input_comparison':prefix+'-independent-review.json','cases':rows}


if __name__=='__main__':
    import sys
    prefix=sys.argv[1];output=Path(prefix+'-reference-review.json');assert not output.exists()
    output.write_text(json.dumps(audit(prefix),indent=2)+'\n')
