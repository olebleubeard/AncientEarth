"""One predeclared loop-counter performance decision; no sweep or adoption."""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path

from review_optical_midpoint_full import review


def load(path):return json.loads(Path(path).read_text(encoding='utf-8-sig'))
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def same_bytes(a,b):
    with open(a,'rb') as left,open(b,'rb') as right:
        while True:
            x,y=left.read(1024*1024),right.read(1024*1024)
            if x!=y:return False
            if not x:return True


def audit(candidate,baseline):
    result=review(candidate,baseline)
    a,b=load(candidate+'.json'),load(baseline+'.json')
    assert a['finite_loops'] and not b['finite_loops'] and a['reference'] and b['reference']
    for report in (a,b):
        for flag in ['reference_reuse','reference_carry','prune_unused','uniform_loops','selected','curves','cartesian_curves','positive_contacts']:
            assert not report[flag],flag
        assert report['strictly_increasing_unique_timestamp_pairs']
    for key in ['adapter','dispatch','dimensions','record_count','output_bytes','terminal_bytes','diagnostic_bytes_per_record']:
        assert a[key]==b[key],key
    termination=a['termination'];assert termination['admitted'] and termination['source_exact_bytes'] and termination['uniform_exact_bytes']
    assert b['termination'] is None
    assert same_bytes(candidate+'.wgsl',baseline+'.wgsl')
    assert same_bytes(candidate+'.wgsl',termination['reference_shader_path'])
    assert same_bytes(candidate+'-primary.bin',termination['reference_uniform_path'])
    assert same_bytes(candidate+'-readback.bin',baseline+'-readback.bin')
    assert result['terminal_word_differences']==result['diagnostic_word_differences']==0
    provenance={}
    for prefix in (candidate,baseline):
        life=load(prefix+'-lifecycle.json');assert life['exit_code']==0 and not life['timed_out']
        source=load(prefix+'-sources.json');assert source['compiler_inputs_unchanged']
        provenance[prefix]={
            'sources_manifest_sha256':digest(prefix+'-sources.json'),
            'input_stability_sha256':digest(prefix+'-input-stability.json'),
            'outputs_manifest_sha256':digest(prefix+'-outputs.json'),
            'lifecycle_sha256':digest(prefix+'-lifecycle.json'),
            'source_shader_sha256':digest(prefix+'.wgsl'),
            'raw_output_sha256':digest(prefix+'-readback.bin'),
            'uploaded_inputs':{name:digest(prefix+'-'+name+'.bin') for name in ['primary','field','geometry','frames','certificates']},
        }
    ct=list(map(Decimal,result['candidate_timing']['retained_ms']))
    bt=list(map(Decimal,result['baseline_timing']['retained_ms']))
    assert len(ct)==len(bt)==5
    cm,bm=sorted(ct)[2],sorted(bt)[2]
    material=cm<=Decimal('0.8')*bm;separated=max(ct)<min(bt)
    result.update({
        'scope':'Single fixed-scene headless primary loop-counter control. All mathematical source/input/output bytes match; only the admitted compiler loop-bounding policy differs. Predeclared performance gate fails; no sweep or production adoption. This is not a native FPS, physical accuracy, or general shader-safety result.',
        'complete_output_bytes_equal':True,'complete_output_bytes':a['output_bytes'],
        'all_shader_bytes_equal':True,'reference_source_admission_verified':True,
        'source_and_output_provenance':provenance,'auditor_sha256':digest(__file__),
        'decision_rule':{'median_ratio_maximum':'0.8','candidate_max_below_baseline_min_required':True,
                         'warmup_iterations':[0,1,2],'all_retained_iterations':[3,4,5,6,7]},
        'median_ratio':str(cm/bm),'median_reduction_percent':str(100*(bm-cm)/bm),
        'twenty_percent_gate_passes':material,'observed_ranges_separated':separated,
        'performance_branch_admitted':material and separated,
        'decision':'reject: less than the predeclared20% median reduction' if not material else ('reject: measured ranges overlap' if not separated else 'pass bounded performance gate only'),
        'production_adoption':False,'additional_sweep_authorized_by_this_result':False,
    })
    return result


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('candidate');parser.add_argument('baseline');args=parser.parse_args()
    result=audit(args.candidate,args.baseline);output=Path(args.candidate+'-independent-review.json');assert not output.exists()
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({key:result[key] for key in ['complete_output_bytes','terminal_word_differences','diagnostic_word_differences','candidate_timing','baseline_timing','median_reduction_percent','observed_ranges_separated','twenty_percent_gate_passes','decision']},indent=2))
