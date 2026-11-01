"""Frozen scalar correction-stencil cost/rejection evidence, not an interpolation certificate."""
import argparse
from collections import Counter
from decimal import Decimal
import hashlib
import json
import math
from pathlib import Path
import struct


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def number(bits):
    assert isinstance(bits, int) and 0 <= bits < 2**64
    value = struct.unpack('<d', struct.pack('<Q', bits))[0]
    assert math.isfinite(value)
    return value


def review(prefix, baseline=None, progress_baseline=None):
    report = read(prefix + '.json')
    assert report['schema'] == 'ancientearth.deviation-stencil-cost-screen.v1'
    for item in read(prefix + '-outputs.json'):
        assert sha(item['path']) == item['sha256']
    sources = read(prefix + '-sources.json')
    stability = read(prefix + '-input-stability.json')
    assert stability['inputs_unchanged_through_run']
    before = {p['path']:p['sha256'] for p in sources['inputs']}
    after = {p['path']:p['sha256'] for p in stability['after_run']}
    assert before == after
    current_changes = [p for p, digest in before.items() if not Path(p).exists() or sha(p) != digest]
    inputs = report['input_bits']
    knots = [(number(h), number(n)) for h, n in inputs['knots']]
    eye = number(inputs['height']);axis = list(map(number, inputs['axis']))
    view = list(map(number, inputs['view']))
    assert len(axis) == 3 and len(view) == 20 and view[14] == 0.
    assert abs(sum(v*v for v in axis) - 1.) <= 1e-15
    assert len(knots) == 45 and all(a[0] < b[0] for a, b in zip(knots, knots[1:]))
    expected = [h for h, _ in knots if h > eye]
    assert len(expected) == 44
    assert report['certified_admitted_cells'] == 0 and len(report['cases']) == 36
    tolerance = report['local_estimator_tolerance_m']
    assert tolerance == 1e-5
    rows=[];decoded_samples=0;terminal_reasons=Counter();terminal_rows=[]
    for c in report['cases']:
        x,y,w,h = c['cell']
        assert w == h and w in (4,8,16) and 0 <= x <= 1920-w and 0 <= y <= 1080-h
        assert c['certified_admission'] is False
        assert len(c['corner_launch_bits']) == len(c['corner_source_states']) == 4
        lengths=[]
        for launch_bits, states in zip(c['corner_launch_bits'], c['corner_source_states']):
            launch=list(map(number,launch_bits));assert len(launch)==3
            assert abs(sum(v*v for v in launch)-1.) <= 1e-15
            heights=[]
            for state in states:
                heights.append(number(state['height_bits']))
                values=list(map(number,state['state_bits']))
                assert len(values)==10
                estimate=number(state['local_estimate_bits'])
                assert 0. <= estimate <= tolerance
                decoded_samples += 1
            assert heights == expected[:len(heights)]
            lengths.append(len(heights))
        assert c['corner_exits'] == all(n == len(expected) for n in lengths)
        if 'corner_terminal' in c:
            assert len(c['corner_terminal'])==4
            for index,t in enumerate(c['corner_terminal']):
                reason=t['reason'];terminal_reasons[reason]+=1
                assert reason in ('initial domain','height step floor','no represented height progress','attempt budget','top exit')
                final_height=number(t['height_bits']);target=number(t['target_bits'])
                step=number(t['step_bits']);proposal=number(t['proposal_bits'])
                assert t['exit']==(reason=='top exit')
                assert 0<=t['accepted']<=t['attempts']<=8192
                assert t['stage_rejections']>=0 and t['error_rejections']>=0
                assert t['attempts']==t['accepted']+t['stage_rejections']+t['error_rejections']+int(reason in ('height step floor','no represented height progress'))
                if reason=='top exit':assert final_height==target==knots[-1][0]
                if reason=='height step floor':assert 0<=step<1e-8 and final_height<target
                if reason=='no represented height progress':assert step<=0 and final_height<target
                if reason=='initial domain':assert t['attempts']==0 and not c['corner_source_states'][index]
                if reason=='attempt budget':assert t['attempts']==8192
                terminal_rows.append({'cell':c['cell'],'corner':index,'reason':reason,'height_m':final_height,
                                      'target_m':target,'remaining_height_m':target-final_height,
                                      'last_step_m':step,'last_proposal_m':proposal,'attempts':t['attempts'],
                                      'accepted':t['accepted'],'stage_rejections':t['stage_rejections'],
                                      'error_rejections':t['error_rejections']})
        for field in ('producer_attempts','direct_attempts','direct_pixel_exits','source_height_comparisons'):
            assert isinstance(c[field],int) and c[field] >= 0
        for field in ('producer_seconds','direct_check_seconds','existing_scalar_control_seconds'):
            assert math.isfinite(c[field]) and c[field] >= 0
        errors=c['observed_maximum_correction_difference']
        assert all(math.isfinite(v) and v >= 0 for v in errors.values())
        if c['corner_exits']:
            assert c['direct_pixel_exits'] == w*h
            assert c['source_height_comparisons'] == w*h*len(expected)
        else:
            assert c['direct_pixel_exits'] == c['direct_attempts'] == c['source_height_comparisons'] == 0
            assert all(v == 0 for v in errors.values())
        control=c['existing_scalar_control']
        assert control['exits']+control['deferred']==w*h
        assert 0 <= control['accepted'] <= control['attempts']
        assert math.isfinite(control['checksum'])
        rows.append({'cell':c['cell'],'corner_exits':c['corner_exits'],
                     'corner_saved_source_counts':lengths,'direct_pixel_exits':c['direct_pixel_exits'],
                     'source_height_comparisons':c['source_height_comparisons'],
                     'observed_maximum_correction_difference':errors if c['source_height_comparisons'] else None,
                     'unmeasured_zero_fields_preserved_in_raw':not bool(c['source_height_comparisons'])})
    sizes=[]
    for size in (4,8,16):
        group=[c for c in report['cases'] if c['cell'][2] == size]
        assert len(group)==12 and len({tuple(c['cell'][:2]) for c in group})==12
        measured=[c for c in group if c['corner_exits']]
        totals={field:str(sum((Decimal(str(c[field])) for c in group), Decimal(0)))
                for field in ('producer_seconds','direct_check_seconds','existing_scalar_control_seconds')}
        sizes.append({'size':size,'cases':len(group),'all_corner_exit_cells':len(measured),
                      'direct_pixel_exits':sum(c['direct_pixel_exits'] for c in group),
                      'source_height_comparisons':sum(c['source_height_comparisons'] for c in group),
                      'single_pass_seconds_sums':totals,
                      'measured_position_difference_min_m':min(c['observed_maximum_correction_difference']['position_m'] for c in measured),
                      'measured_position_difference_max_m':max(c['observed_maximum_correction_difference']['position_m'] for c in measured)})
    comparison=None
    if baseline:
        old=read(baseline+'.json')
        assert {k:v for k,v in report.items() if k!='cases'}=={k:v for k,v in old.items() if k!='cases'}
        assert len(old['cases'])==len(report['cases'])
        for old_case,new_case in zip(old['cases'],report['cases'],strict=True):
            comparable=lambda c:{k:v for k,v in c.items() if not k.endswith('_seconds') and k!='corner_terminal'}
            assert comparable(old_case)==comparable(new_case)
        comparison={'baseline':baseline,'baseline_sha256':sha(baseline+'.json'),
                    'all_preexisting_non_timing_fields_exact':True}
    progress=None
    if progress_baseline:
        old=read(progress_baseline+'.json')
        assert old['input_bits']==report['input_bits'] and len(old['cases'])==len(report['cases'])
        retained=0;added=0;transitions=Counter();new_final_steps=[]
        for a,b in zip(old['cases'],report['cases'],strict=True):
            assert a['cell']==b['cell'] and a['corner_launch_bits']==b['corner_launch_bits']
            for index,(sa,sb,ta,tb) in enumerate(zip(a['corner_source_states'],b['corner_source_states'],a['corner_terminal'],b['corner_terminal'],strict=True)):
                assert sa==sb[:len(sa)]
                retained+=len(sa);added+=len(sb)-len(sa)
                transitions[(ta['reason'],tb['reason'])]+=1
                if ta['reason']=='height step floor':
                    assert tb['reason']=='top exit' and len(sb)==len(sa)+1
                    assert tb['accepted']==ta['accepted']+1 and tb['attempts']==ta['attempts']
                    assert tb['stage_rejections']==ta['stage_rejections'] and tb['error_rejections']==ta['error_rejections']
                    final_step=number(tb['step_bits'])
                    assert 0<final_step==number(ta['target_bits'])-number(ta['height_bits'])
                    assert sb[-1]['height_bits']==tb['target_bits']
                    new_final_steps.append({'cell':b['cell'],'corner':index,'final_positive_step_m':final_step})
                else:
                    assert ta==tb and sa==sb
            if a['corner_exits']:
                comparable=lambda c:{k:v for k,v in c.items() if not k.endswith('_seconds')}
                assert comparable(a)==comparable(b)
        progress={'baseline':progress_baseline,'baseline_sha256':sha(progress_baseline+'.json'),
                  'retained_source_records_exact':retained,'new_top_records':added,
                  'corner_transitions':[{'before':a,'after':b,'count':n} for (a,b),n in transitions.items()],
                  'new_final_steps':new_final_steps,
                  'limitation':'Prior telemetry has no state immediately before the tiny final interval; its '
                               'positive represented step/count and new finite top record are checked, not an '
                               'unavailable direct subtraction of pre/post tiny-step states.'}
    return {'scope':__doc__,'input_report_sha256':sha(prefix+'.json'),
            'outputs_and_frozen_input_manifest_verified':True,'current_input_paths_changed_since_run':current_changes,
            'certified_admitted_cells':0,'decoded_corner_source_states':decoded_samples,'sizes':sizes,'cases':rows,
            'baseline_comparison':comparison,'progress_comparison':progress,'corner_terminal_reason_counts':dict(terminal_reasons),
            'corner_terminals':terminal_rows,
            'limitations':'Only cases marked as all-corner exits reached all source endpoints; other reported zero differences '
                         'are unmeasured. The saved source states and local estimators are finite numerical outputs, '
                         'not independently integrated exact-ODE references. Four-corner interpolation comparisons '
                         'are finite samples, not whole-cell or between-source bounds. Each cost is one CPU pass, '
                         'with different scalar-control numerical policies and no directly comparable accuracy '
                         'guarantee. No certified sharing, exact-line replacement, scene, GPU or native acceptance.'}


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('prefix');p.add_argument('--baseline');p.add_argument('--progress-baseline');a=p.parse_args()
    result=review(a.prefix,a.baseline,a.progress_baseline);output=Path(a.prefix+'-independent-review.json');assert not output.exists()
    output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ('cases','corner_terminals')},indent=2))
