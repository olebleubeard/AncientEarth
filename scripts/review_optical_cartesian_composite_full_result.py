"""One independent raw closure of the full composite decision; no new runs."""
from collections import Counter
from decimal import Decimal, localcontext
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path

import numpy as np
from review_optical_cartesian_composite_matrix_result import errors

ROOT = Path(__file__).resolve().parents[1]
C = 'artifacts/optical-composite-f64-full-candidate-first'
B = 'artifacts/optical-composite-f64-full-reference-first'
OUTPUT = C+'-closure-review.json'
N = 1920*1080
NAMES = {0: 'unhanded-original', 1: 'accepted-suffix', 2: 'original-fallback', 3: 'masked'}
METRICS = ('position_m', 'path_m', 'direction')
CACHE = {}


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def sha(path):
    key = (ROOT/path).resolve()
    if key not in CACHE:
        with key.open('rb') as stream:
            CACHE[key] = hashlib.file_digest(stream, 'sha256').hexdigest()
    return CACHE[key]


def verify(rows):
    unique = {}
    for row in rows:
        p = row['path'].replace('\\', '/')
        assert sha(p) == row['sha256'], p
        assert p not in unique or unique[p] == row['sha256']
        unique[p] = row['sha256']
    return unique


def provenance(prefix, retained):
    manifest, compiled, stable = [load(prefix+'-'+s+'.json') for s in ('sources', 'compile-inputs', 'input-stability')]
    assert manifest['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert manifest['files'][:-1] == compiled['inputs'] == stable['after_run']
    inputs, outputs = verify(manifest['files'][:-1]), verify(load(prefix+'-outputs.json'))
    exe_sha = sha(prefix+'-executable.exe')
    assert exe_sha == manifest['files'][-1]['sha256']
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None
    assert not life['timed_out'] and not life['terminated_by_wrapper']
    mode = '--cartesian-composite-full' if prefix == C else '--reference'
    assert life['arguments'] == ['--name', Path(prefix).name, '--full', '--time', mode]
    if prefix == C:
        assert manifest['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
        assert life['executable_sha256'] == exe_sha
    expected = retained['provenance'][prefix]
    assert len(inputs) == expected['input_hashes_verified'] and len(outputs) == expected['output_hashes_verified']
    assert exe_sha == expected['retained_executable_sha256']
    for suffix, key in (('sources', 'sources_sha256'), ('outputs', 'outputs_manifest_sha256'),
                        ('lifecycle', 'lifecycle_sha256'), ('input-stability', 'input_stability_sha256')):
        assert sha(prefix+'-'+suffix+'.json') == expected[key]
    return {'unique_inputs': len(inputs), 'output_hashes': len(outputs), 'retained_executable_sha256': exe_sha}


def raw(prefix):
    report = load(prefix+'.json')
    assert report['record_count'] == N and report['dimensions'] == [1920, 1080]
    assert not report['selected'] and report['timed'] and report['diagnostic_bytes_per_record'] == 32
    assert report['terminal_bytes'] == 16+48*N and report['output_bytes'] == 16+80*N
    assert (ROOT/(prefix+'-readback.bin')).stat().st_size == report['output_bytes']
    words = np.memmap(ROOT/(prefix+'-readback.bin'), dtype='<u4', mode='r')
    assert words[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    return report, words, words[4:4+12*N].reshape(N, 12), words[4+12*N:].reshape(N, 8)


def times(report):
    assert report['strictly_increasing_unique_timestamp_pairs'] and len(report['iterations']) == 8
    previous = 0
    result = []
    for i, row in enumerate(report['iterations']):
        assert row['iteration'] == i and row['warmup'] == (i < 3)
        begin, end = row['raw_timestamps']
        assert 0 < begin < end and begin > previous
        previous = end
        period = Decimal(str(row['timestamp_period_ns']))
        assert period.is_finite() and period > 0
        seconds = Decimal(end-begin)*period/Decimal(10**9)
        observed = row['gpu_seconds']
        assert math.isfinite(observed) and observed > 0
        assert abs(float(seconds)-observed) <= 2*math.ulp(observed)
        if i >= 3: result.append(seconds*1000)
    return result


def run():
    retained = load(C+'-independent-review.json')
    assert retained['correctness_passed'] and not retained['passed']
    assert retained['decision'] == 'reject-without-tuning'
    proof = load(retained['source_gate']['path'])
    assert proof['passed'] and proof['all_three_modules_approved'] and proof['five_uploads_exact']
    assert sha(retained['source_gate']['path']) == retained['source_gate']['sha256']
    for path, digest in proof['sha256'].items(): assert sha(path) == digest, path
    for path, digest in retained['prerequisite_closures'].items(): assert sha(path) == digest, path
    p = {prefix: provenance(prefix, retained) for prefix in (B, C)}
    for row in proof['modules']:
        suffix = '-'+row['name']+'.wgsl'
        assert sha(C+suffix) == sha(proof['candidate_prefix']+suffix) == row['sha256']
    assert sha(C+'.wgsl') == sha(C+'-prefix.wgsl')
    assert sha(B+'.wgsl') == sha(proof['original_wgsl'])
    for suffix, digest in proof['uploaded_sha256'].items():
        assert sha(C+suffix) == sha(B+suffix) == sha(proof['candidate_prefix']+suffix) == digest
    a, ar, at, ad = raw(C)
    b, br, bt, bd = raw(B)
    assert a['adapter'] == b['adapter'] and a['cartesian_composite_full'] and b['reference']
    assert not a['reference'] and not a['selected'] and not b['selected']
    assert 'No pre-pass progress clear.' in a['timing_scope']
    assert a['fixture']['cartesian_composite_full']['progress_reset_inside_timed_sequence']
    assert not a['fixture']['cartesian_composite_full']['host_progress_clear_required']
    assert np.array_equal(ar[:4], br[:4])
    primary_words = np.fromfile(ROOT/(C+'-primary.bin'), dtype='<u4')
    assert np.array_equal(ar[:4], primary_words[44:48])
    primary = primary_words.copy().view('<f4').reshape(140, 4).astype(float)
    count = int(primary[9, 0]); layers = primary[12:12+count]
    assert primary[9, 0] == count and 2 <= count <= 128
    assert np.isfinite(layers).all() and np.all(np.diff(layers[:, 0]) > 0)
    assert primary[10, 1] == layers[0, 0] and primary[9, 1] == layers[-1, 0]
    assert a['state_words_per_record'] == 24 and a['state_bytes'] == 96*N
    assert (ROOT/(C+'-states.bin')).stat().st_size == 96*N
    state = np.memmap(ROOT/(C+'-states.bin'), dtype='<u4', mode='r').reshape(N, 24)
    routes, statuses = Counter(), Counter()
    sums = np.zeros(3, dtype=np.uint64)
    max_work = exact_original = sky_owner = 0
    for begin in range(0, N, 65536):
        end = min(begin+65536, N)
        s, t, d, old, launch = state[begin:end], at[begin:end], ad[begin:end], bt[begin:end], bd[begin:end]
        assert np.array_equal(d[:, 4:7], launch[:, 4:7]) and np.array_equal(t[:, 7], old[:, 7])
        for values in (t, old):
            assert np.isfinite(values[:, :7].copy().view('<f4')).all()
            assert np.isfinite(values[:, 8:11].copy().view('<f4')).all()
        f = s.view('<f4')
        assert np.isfinite(f[:, :11]).all() and np.isfinite(f[:, 16:20]).all()
        assert np.all(s[:, 11] == 0) and np.all(s[:, 23] == 0)
        assert np.array_equal(s[:, 21], np.arange(begin, end, dtype=np.uint32))
        assert np.array_equal(s[:, 8:11], d[:, 4:7]) and np.all(d[:, 7] == 0)
        assert np.array_equal(s[:, 12:15], d[:, :3])
        assert np.array_equal(s[:, 22] | (s[:, 15] << 8), d[:, 3])
        assert np.all(s[:, 12:15] <= 4096)
        work = s[:, 12:15].astype(np.uint64)
        assert np.all(work[:, 0]+work[:, 1] <= 4096)
        sums += work.sum(axis=0); max_work = max(max_work, int(work.sum(axis=1).max()))
        route = s[:, 22]; assert np.all(route <= 3) and np.all(t[:, 7] <= 3)
        original = route != 1; handed = (route == 1) | (route == 2)
        assert np.array_equal(t[original], old[original])
        assert np.array_equal(t[~original, 8:12], old[~original, 8:12])
        exact_original += 12*int(original.sum()); sky_owner += 4*int((~original).sum())
        assert np.all(s[~handed, 13:16] == 0) and np.all(s[~handed, :8] == 0)
        assert np.all(s[~handed, 16:20] == 0) and np.all(s[~handed, 20] == np.uint32(0xfffffffe))
        assert np.all(s[handed, 12:14] > 0)
        assert np.all(t[route == 1, 7] == 0) and np.all(s[route == 1, 14:16] == 0)
        assert np.all(s[route == 2, 14] > 0) and np.isin(s[route == 2, 15], [2, 3, 4, 5]).all()
        assert np.all(t[route == 3, 7] == 3) and np.all(s[route == 3, 12:15] == 0)
        assert np.all(t[route == 0, 7] <= 2)
        owner = s[handed, 20].view('<i4').astype(np.int64)
        assert np.all((-1 <= owner) & (owner < count-1))
        low = np.where(owner == -1, primary[10, 2], layers[np.maximum(owner, 0), 0])
        high = np.where(owner == -1, primary[10, 1], layers[owner+1, 0])
        assert np.all((low < f[handed, 19]) & (f[handed, 19] < high))
        assert np.all(f[handed, 3] >= 0) and np.all(f[handed, 7] >= 0)
        assert np.any(f[handed, 4:7] != 0, axis=1).all() and np.any(f[handed, 16:19] != 0, axis=1).all()
        routes.update({NAMES[int(k)]: int(v) for k, v in zip(*np.unique(route, return_counts=True))})
        statuses.update({str(int(k)): int(v) for k, v in zip(*np.unique(t[:, 7], return_counts=True))})
    assert routes['accepted-suffix'] > 0 and sum(v for k, v in routes.items() if k != 'accepted-suffix') > 0
    assert dict(routes) == retained['route_counts'] and dict(statuses) == retained['status_counts'] == a['raw_status_counts']
    assert exact_original == retained['original_route_terminal_words_exact']
    assert sky_owner == retained['accepted_sky_normal_and_owner_words_exact']
    assert sums.tolist() == retained['attempt_sums']
    progress = np.fromfile(ROOT/(C+'-progress.bin'), dtype='<u4').tolist()
    assert progress == a['progress_words'] == retained['progress'] == [0, N, max_work, 0]
    print(json.dumps({'all_full_records_passed': N, 'route_counts': dict(routes)}), flush=True)

    policy = load('artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json')
    baseline = load(policy['baseline_report']); contract = load(policy['initial_contract'])
    assert sha(policy['baseline_report']) == policy['baseline_sha256']
    assert sha(policy['initial_contract']) == policy['initial_contract_sha256']
    refs = {r['record_index']: r for r in baseline['cases']}
    selected = np.fromfile(ROOT/'artifacts/optical-cartesian-suffix-tight-selected-first-states.bin', dtype='<u4').reshape(23, 24)
    cols = list(range(13))+list(range(16, 21))+[23]
    metric_checks = 0
    for i, old in enumerate(contract['records']):
        pixel = old.get('pixel', old.get('pixel_label')); index = pixel[1]*1920+pixel[0]
        assert bt[index].tolist() == old['original_terminal_words'] and ad[index, 4:7].tolist() == old['launch_words']
        assert state[index, 21] == index and selected[i, 21] == i
        assert np.array_equal(state[index, cols], selected[i, cols])
        route = int(state[index, 22]); measured = errors(at[index], refs[i]) if i in refs else None
        authored = retained['samples'][i]
        assert authored['selected_index'] == i and authored['full_index'] == index and authored['pixel'] == pixel
        assert authored['route'] == NAMES[route] and authored['candidate_error'] == measured and authored['passed']
        if route == 1:
            checks = {m: Q(measured[m]) <= Q(policy['records'][i]['per_metric_error_ceiling_exact'][m]) for m in METRICS}
            assert all(checks.values()) and checks == authored['frozen_metric_checks']; metric_checks += 3
        else: assert at[index].tolist() == old['original_terminal_words']
    assert len(a['samples']) == len(b['samples']) == 23
    for report, terminal, detail in ((a, at, ad), (b, bt, bd)):
        for sample in report['samples']:
            i = sample['record_index']
            assert sample['pixel'] == [i % 1920, i//1920]
            assert sample['terminal_words'] == terminal[i].tolist() and sample['diagnostic_words'] == detail[i].tolist()
            if report is a: assert sample['state_words'] == state[i].tolist()

    # Interpret raw timestamps only after every raw correctness check above.
    ct, ot = times(a), times(b)
    cm, om = sorted(ct)[2], sorted(ot)[2]
    observed = retained['performance_gate']
    assert ct == list(map(Decimal, observed['candidate_retained_ms'])) and ot == list(map(Decimal, observed['original_retained_ms']))
    assert cm == Decimal(observed['candidate_median_ms']) and om == Decimal(observed['original_median_ms'])
    median_gate = Q(cm) <= Q(4, 5)*Q(om); range_gate = max(ct) < min(ot)
    assert median_gate == observed['twenty_percent_gate_passed'] and range_gate == observed['ranges_separated']
    assert (median_gate and range_gate) == observed['passed'] == retained['passed'] == False
    assert 100*(om-cm)/om == Decimal(observed['median_reduction_percent'])
    with localcontext() as ctx:
        ctx.prec = 50
        ratio = str(cm/om)
    sha('scripts/review_optical_cartesian_composite_full_result.py')
    sha('scripts/review_optical_cartesian_composite_matrix_result.py')
    sha(C+'-independent-review.json')
    return {'schema': 'cartesian-composite-full-independent-closure-v1', 'review_completed': True,
            'correctness_passed': True, 'effort_gate_passed': False, 'decision': 'REJECT: no tuning or production adoption',
            'record_count': N, 'route_counts': dict(routes), 'status_counts': dict(statuses),
            'all_launch_words_exact': 3*N, 'all_final_class_words_exact': N,
            'exact_original_terminal_words': exact_original, 'exact_accepted_sky_normal_owner_words': sky_owner,
            'state_and_uploaded_owner_checks_passed': True, 'progress': progress, 'attempt_sums': sums.tolist(),
            'selected_records_checked': 23, 'selected_same_ray_metrics_passed': metric_checks,
            'provenance': p, 'candidate_retained_ms': list(map(str, ct)), 'original_retained_ms': list(map(str, ot)),
            'candidate_median_ms': str(cm), 'original_median_ms': str(om), 'median_time_ratio': ratio,
            'median_time_ratio_exact': str(Q(cm)/Q(om)), 'twenty_percent_gate_passed': median_gate,
            'range_gate_passed': range_gate,
            'sha256': {path.relative_to(ROOT).as_posix() if path.is_relative_to(ROOT) else path.as_posix(): h for path, h in sorted(CACHE.items())},
            'prospective_auditor_reexecutions': 0, 'ode_executions': 0, 'gpu_executions': 0,
            'limitations': ['One frozen complete compute-chain pair; no native moving-flight FPS, presentation or production claim.',
                            'All-record classes/original routes are checked; canonical numerical accuracy is the retained finite matrix and23sample scope, not a global exact-ODE bound.',
                            'Attempt counts do not measure cycles or attribute the slowdown to a particular kernel or precision instruction.']}


if __name__ == '__main__':
    answer = run()
    with (ROOT/OUTPUT).open('x', encoding='utf-8') as stream:
        json.dump(answer, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({k: answer[k] for k in ('correctness_passed', 'effort_gate_passed', 'candidate_median_ms', 'original_median_ms', 'median_time_ratio')}), flush=True)
