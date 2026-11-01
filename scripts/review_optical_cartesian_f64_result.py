"""Independent closure of the one retained selected f64 controller run.

Replays the frozen auditor without a GPU or ODE solve, then independently
decodes identities, fallback cause scope, and the unchanged per-ray metrics.
"""
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path

import numpy as np

import review_optical_cartesian_suffix_f64 as frozen
from verify_optical_dop853 import geometry

ROOT = Path(__file__).resolve().parents[1]
PREFIX = 'artifacts/optical-cartesian-f64-selected-first'
CONTROL = 'artifacts/optical-cartesian-suffix-tight-selected-first'


def load(name):
    return json.loads((ROOT/name).read_text(encoding='utf-8-sig'))


def sha(name):
    with (ROOT/name).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run():
    retained = load(PREFIX+'-independent-review.json')
    replay = frozen.review(PREFIX, 'selected')
    assert replay == retained, 'Frozen auditor does not reproduce the retained report'
    policy = load(frozen.POLICY)
    contract = load(policy['initial_contract'])
    baseline = load(policy['baseline_report'])
    references = {x['record_index']: x for x in baseline['cases']}
    primary = np.fromfile(ROOT/(PREFIX+'-primary.bin'), dtype='<u4')
    raw = np.fromfile(ROOT/(PREFIX+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(PREFIX+'-states.bin'), dtype='<u4').reshape(23,24)
    oldraw = np.fromfile(ROOT/(CONTROL+'-readback.bin'), dtype='<u4')
    oldstate = np.fromfile(ROOT/(CONTROL+'-states.bin'), dtype='<u4').reshape(23,24)
    progress = np.fromfile(ROOT/(PREFIX+'-progress.bin'), dtype='<u4')
    assert np.array_equal(raw[:4], primary[44:48]) and np.array_equal(raw[:4], oldraw[:4])
    terminal, detail = raw[4:280].reshape(23,12), raw[280:].reshape(23,8)
    olddetail = oldraw[280:].reshape(23,8)
    required = [3,9,10,11,12,14,15,17,18,20]
    original_rows = sorted(set(range(23))-set(required))
    completed = np.flatnonzero(state[:,22] == 1).tolist()
    fallback = np.flatnonzero(state[:,22] == 2).tolist()
    assert completed == required[1:] and fallback == [3]
    assert np.flatnonzero(state[:,22] == 0).tolist() == original_rows
    assert np.all(state[:,11] == 0) and np.all(state[:,23] == 0)
    assert np.array_equal(state[:,21], np.arange(23))
    assert np.array_equal(state[:,12:15], detail[:,:3])
    assert np.array_equal(state[:,22] | (state[:,15] << 8), detail[:,3])
    assert np.array_equal(state[:,8:11], detail[:,4:7]) and np.all(detail[:,7] == 0)
    assert np.all(state[:,12:15] <= 4096)
    assert np.all(state[:,12].astype(np.uint64)+state[:,13] <= 4096)
    assert progress.tolist() == [0,23,int(state[:,12:15].astype(np.uint64).sum(axis=1).max()),0]
    # The prospective auditor deliberately compares route as part of its
    # composite identity. Separate raw physical/prefix identity here without
    # changing its historical report or weakening its required-route verdict.
    physical_columns = list(range(13))+list(range(16,22))+[23]
    assert np.array_equal(state[:,physical_columns], oldstate[:,physical_columns])
    assert np.array_equal(state[original_rows], oldstate[original_rows])
    assert np.array_equal(detail[original_rows], olddetail[original_rows])
    for i in original_rows+[3]:
        assert terminal[i].tolist() == contract['records'][i]['original_terminal_words']
    assert np.flatnonzero(state[3] != oldstate[3]).tolist() == [13,14,15,22]
    assert state[3,12:16].tolist() == [67,2,752,3]
    assert detail[3,3] == 2+(3<<8)
    assert terminal[3,7] == 0
    metrics = []
    for i in required:
        ref = references[i]
        value = terminal[i].copy().view('<f4').astype(float)
        assert np.isfinite(value[:7]).all()
        origin = np.asarray(ref['inputs']['origin_ecef_m'], dtype=float)
        _, up = geometry(origin)
        east = np.array([-origin[1],origin[0],0.]); east /= np.linalg.norm(east)
        basis = np.column_stack((east,up,-np.cross(up,east)))
        truth = ref['tight_reference']
        errors = {'position_m':float(np.linalg.norm(origin+basis@value[:3]-truth['endpoint_ecef_m'])),
            'path_m':abs(float(value[3])-truth['geometric_path_m']),
            'direction':float(np.linalg.norm(basis@value[4:7]-truth['exit_direction']))}
        assert errors == retained['records'][i]['candidate_error']
        ceilings = {m:Q(ref['original_error'][m])+Q(ref['reference_convergence'][m]) for m in errors}
        assert all(ceilings[m] == Q(policy['records'][i]['per_metric_error_ceiling_exact'][m]) for m in errors)
        checks = {m:Q(v) <= ceilings[m] for m,v in errors.items()}
        assert all(checks.values())
        if i in completed:
            assert checks == retained['records'][i]['per_ray_checks']
        else:
            assert errors == ref['original_error']
        metrics.append({'record_index':i,'route':'accepted-suffix' if i in completed else 'original-fallback',
            'errors':errors,'frozen_ceilings_exact':{m:str(x) for m,x in ceilings.items()},
            'per_metric_checks':checks,
            'suffix_accuracy_evidence':i in completed})
    assert retained['failed_records'] == [3] and not retained['passed']
    assert retained['records'][3]['failure_reasons'] == ['handoff-or-prefix-identity-differs','required-route-differs']
    assert retained['all_handoffs_and_prefixes_exact'] is False
    suffix = (ROOT/(PREFIX+'-suffix.wgsl')).read_text(encoding='utf-8')
    assert suffix.count('cs_failure_reason=3u;') == 1
    failure_site = '''if !cf64_curve_scene_clear(state.p,cf64_dp_curve.delta.p,cf64_dp_curve.r3,cf64_dp_curve.r4,cf64_dp_curve.r5,step,cf64_dp_curve_step,accepted.error,allocation,tolerance){
            cs_failure_reason=3u;return cf64_trace_result(state.p,state.u,path,2u);
        }'''
    assert failure_site in suffix
    report = load(PREFIX+'.json')
    assert not report['timed'] and report['iterations'] == [] and report['auxiliary_layout'] is None
    assert not (ROOT/'artifacts/optical-cartesian-f64-atmosphere-first.json').exists()
    return {'schema':'cartesian-f64-selected-independent-closure-v1','review_completed':True,
        'experiment_passed':False,'decision':'REJECT: required record3 lost suffix route; stop without retuning or atmosphere execution.',
        'frozen_auditor_replay_exact':True,
        'sha256':{name:sha(name) for name in [__file__,'scripts/review_optical_cartesian_suffix_f64.py',
            PREFIX+'-independent-review.json',PREFIX+'.json',PREFIX+'-sources.json',PREFIX+'-outputs.json',
            PREFIX+'-readback.bin',PREFIX+'-states.bin',PREFIX+'-progress.bin',PREFIX+'-suffix.wgsl',
            'docs/science/cartesian-f64-suffix-design.md']},
        'provenance':{'source_hashes_verified':len(retained['candidate_input_hashes']),
            'output_hashes_verified':len(retained['candidate_output_hashes']),
            'historical_control_outputs_verified':len(retained['historical_control_output_hashes']),
            'offline_modules_and_five_uploads_exact':True,'lifecycle_retained_executable_success':True},
        'route_counts':dict(Counter(map(int,state[:,22]))),'status_counts':dict(Counter(map(int,terminal[:,7]))),
        'required_suffix_records':required,'completed_suffix_records':completed,
        'all23_physical_handoff_and_prefix_columns_exact':True,'physical_identity_columns':physical_columns,
        'original13_state_details_and_terminals_exact':True,'fallback_original_terminal_exact':True,
        'composite_identity_flag_explanation':'The frozen auditor also includes final route word22 in identity. Its false aggregate is caused by record3 route1 becoming route2; all physical handoff and prefix words remain exact. Historical report and rejection are unchanged.',
        'record3':{'pixel':contract['records'][3]['pixel'],'raw_state':state[3].tolist(),
            'handoff_p_path':state[3,:4].copy().view('<f4').astype(float).tolist(),
            'raw_terminal':terminal[3].tolist(),'raw_details':detail[3].tolist(),
            'changed_state_columns_vs_control':[13,14,15,22],
            'counts_prefix_suffix_fallback':[67,2,752],'reason':3,
            'reason_scope':'Complete cf64_curve_scene_clear returned false. Retained data do not distinguish coefficient conversion/domain, total error budget, or terrain/marker root nonseparation; they do not establish an actual intersection.',
            'accepted_prefix_history':'No step log or final internal suffix state was retained. Two attempts do not identify rejection/commit chronology or the failing trial length.'},
        'progress_words':progress.tolist(),'completed_suffix_metric_gates_passed':27,
        'metric_rows':metrics,'atmosphere_case':'UNRUN under fixed fail-fast policy',
        'gpu_executions':0,'ode_executions':0,'timing_interpreted':False,
        'scope':'Finite raw selected-run closure only. No tuning, alternate allowance, new controller, global ODE, complete-image or native performance claim.'}


if __name__ == '__main__':
    result = run()
    with (ROOT/(PREFIX+'-closure-review.json')).open('x',encoding='utf-8') as stream:
        json.dump(result,stream,indent=2,allow_nan=False); stream.write('\n')
    print(json.dumps({k:result[k] for k in ['review_completed','experiment_passed','decision',
        'all23_physical_handoff_and_prefix_columns_exact','completed_suffix_metric_gates_passed']},indent=2))
