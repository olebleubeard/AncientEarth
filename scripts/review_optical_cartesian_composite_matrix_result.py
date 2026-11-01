"""One independent raw closure of nine completed composite matrix captures.

No prospective auditor replay, ODE solve, shader compilation, dispatch or timing.
The retained per-case contracts supply the unchanged finite reference metrics.
"""
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path

import numpy as np

from verify_optical_dop853 import geometry

ROOT = Path(__file__).resolve().parents[1]
CASES = ('readiness', 'fisheye', 'vacuum-outward', 'vacuum-inward',
         'atmosphere-minus20-vacuum', 'atmosphere-minus20-gradient',
         'atmosphere-100-vacuum', 'atmosphere-5000-vacuum', 'atmosphere-5000-gradient')
METRICS = ('position_m', 'path_m', 'direction')
LIMITS = dict(zip(METRICS, (5., 5., 3e-6)))
NAMES = {0: 'unhanded-original', 1: 'accepted-suffix', 2: 'original-fallback', 3: 'masked'}
PHYSICAL = list(range(13))+list(range(16, 22))+[23]
OUTPUT = 'artifacts/optical-composite-f64-matrix-first-closure-review.json'
CACHE = {}


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def sha(path):
    absolute = (ROOT/path).resolve()
    if absolute not in CACHE:
        with absolute.open('rb') as stream:
            CACHE[absolute] = hashlib.file_digest(stream, 'sha256').hexdigest()
    return CACHE[absolute]


def hashes(rows):
    result = {}
    for row in rows:
        path = row['path'].replace('\\', '/')
        assert sha(path) == row['sha256'], path
        assert path not in result or result[path] == row['sha256']
        result[path] = row['sha256']
    return result


def errors(words, ref):
    # Independently reconstruct the prospectively pinned binary64 metric;
    # the retained converged endpoint is read, never integrated again.
    values = np.asarray(words, dtype='<u4').view('<f4').astype(float)
    origin = np.asarray(ref['inputs']['origin_ecef_m'], dtype=float)
    _, up = geometry(origin)
    east = np.array([-origin[1], origin[0], 0.])
    east /= np.linalg.norm(east)
    basis = np.column_stack((east, up, -np.cross(up, east)))
    truth = ref['tight_reference']
    return {'position_m': float(np.linalg.norm(origin+basis@values[:3]-truth['endpoint_ecef_m'])),
            'path_m': abs(float(values[3])-truth['geometric_path_m']),
            'direction': float(np.linalg.norm(basis@values[4:7]-truth['exit_direction']))}


def audit_case(case):
    prefix = 'artifacts/optical-composite-f64-matrix-'+case+'-first'
    retained = load(prefix+'-independent-review.json')
    assert retained['passed'] and retained['decision'] == 'packet-final-output-pass-only'
    assert not retained['returning_coverage_or_full_timing_authorized']
    source, compiled, stable = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert source['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert source['files'][:-1] == compiled['inputs'] == stable['after_run']
    inputs, outputs = hashes(source['files']), hashes(load(prefix+'-outputs.json'))
    assert inputs == retained['candidate_input_hashes'] and outputs == retained['candidate_output_hashes']
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None
    assert not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-composite-f64', case]
    assert source['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    assert sha(prefix+'-executable.exe') == life['executable_sha256'] == source['files'][-1]['sha256']
    source_row = retained['source_case']
    for suffix in ('-prefix.wgsl', '-suffix.wgsl', '-fallback.wgsl', '-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin'):
        assert sha(prefix+suffix) == sha(source_row['candidate_prefix']+suffix)
    baseline_path = 'artifacts/optical-cartesian-matrix-'+case+'-baseline-freeze-first.json'
    baseline = load(baseline_path)
    contract = load(baseline['contract'])
    assert sha(baseline['contract']) == baseline['contract_sha256'] == retained['baseline_contract_sha256']
    assert baseline['eligible_for_same_packet_candidate'] and baseline['candidate_readback_used'] is False
    for path, digest in contract['input_sha256'].items():
        assert sha(path) == digest, path
    refs = {r['record_index']: r for r in baseline['references']}
    report = load(prefix+'.json')
    assert report['selected'] and not report['timed'] and report['iterations'] == []
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['diagnostic_bytes_per_record'] == 32 and report['auxiliary_layout'] is None
    assert report['steps'] == load(contract['baseline']+'.json')['steps']
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    primary_words = np.fromfile(ROOT/(prefix+'-primary.bin'), dtype='<u4')
    primary = primary_words.copy().view('<f4').reshape(140, 4).astype(float)
    old_raw = np.fromfile(ROOT/(contract['baseline']+'-readback.bin'), dtype='<u4')
    old_state = np.fromfile(ROOT/(contract['baseline']+'-states.bin'), dtype='<u4').reshape(23, 24)
    assert raw.nbytes == 1856 and state.nbytes == 2208 and progress.nbytes == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    assert np.array_equal(raw[:4], old_raw[:4]) and np.array_equal(raw[:4], primary_words[44:48])
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(terminal.copy().view('<f4')[:, 8:11]).all()
    assert np.isfinite(state.copy().view('<f4')[:, :11]).all()
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.array_equal(state[:, 21], np.arange(23))
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.all(state[:, 12:15] <= 4096)
    assert np.all(state[:, 12].astype(np.uint64)+state[:, 13] <= 4096)
    assert progress.tolist() == report['progress_words'] == [0, 23, int(state[:, 12:15].astype(np.uint64).sum(axis=1).max()), 0]
    control = None
    controls = None
    if not case.startswith('atmosphere-5000-'):
        path = 'artifacts/optical-cartesian-matrix-'+case+'-candidate-first'
        controls = hashes(load(path+'-outputs.json'))
        control = np.fromfile(ROOT/(path+'-states.bin'), dtype='<u4').reshape(23, 24)
    assert controls == retained['result']['historical_control_output_hashes']
    routes, statuses, launches = Counter(), Counter(), Counter()
    same_count = absolute_count = exact_count = 0
    maximum = dict.fromkeys(METRICS, 0.)
    for i in range(23):
        authored = retained['result']['records'][i]
        old, sample = contract['records'][i], report['samples'][i]
        route, status = int(state[i, 22]), int(terminal[i, 7])
        assert route in NAMES and authored['passed'] and authored['failure_reasons'] == []
        assert authored['record_index'] == sample['record_index'] == i
        assert authored['pixel'] == sample['pixel'] == old['pixel_label']
        assert sample['terminal_words'] == terminal[i].tolist() and sample['state_words'] == state[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist()
        assert detail[i, 4:7].tolist() == old['launch_words']
        assert authored['route'] == NAMES[route] and authored['status'] == status
        assert authored['attempts'] == state[i, 12:15].tolist() and authored['reason'] == int(state[i, 15])
        assert authored['retained_handoff_comparison_available'] == (control is not None)
        identity = bool(np.array_equal(state[i, PHYSICAL], control[i, PHYSICAL])) if control is not None else None
        assert identity == authored['retained_physical_handoff_and_prefix_exact'] and identity is not False
        if route in (0, 3):
            assert np.all(state[i, 13:16] == 0)
            cols = [j for j in range(24) if j not in (12, 13, 14)]
            assert np.array_equal(state[i, cols], old_state[i, cols])
        if route in (1, 2):
            values = state[i].copy().view('<f4').astype(float)
            owner = int(state[i, 20].view('<i4'))
            count = int(primary[9, 0])
            assert primary[9, 0] == count and 2 <= count <= 128 and -1 <= owner < count-1
            layers = primary[12:12+count]
            assert np.isfinite(layers).all() and np.all(np.diff(layers[:, 0]) > 0)
            assert primary[10, 1] == layers[0, 0] and primary[9, 1] == layers[-1, 0]
            low = primary[10, 2] if owner == -1 else layers[owner, 0]
            high = layers[0, 0] if owner == -1 else layers[owner+1, 0]
            assert np.isfinite(values[16:20]).all() and low < values[19] < high
            assert values[3] >= 0 and values[7] >= 0
            assert np.linalg.norm(values[4:7]) > 0 and np.linalg.norm(values[16:19]) > 0
            assert state[i, 12] > 0 and state[i, 13] > 0
        if route == 1:
            assert status == old['original_status'] == 0 and np.all(state[i, 14:16] == 0)
            assert terminal[i, 8:12].tolist() == old['original_terminal_words'][8:12]
        if route == 2: assert state[i, 14] > 0 and state[i, 15] in (2, 3, 4, 5)
        if route == 3: assert status == 3 and np.all(state[i, 12:15] == 0)
        if case == 'readiness': assert route == 0 and status == 2 and np.all(state[i, 12:15] == 0)
        exact_required = route != 1 or case in ('readiness', 'vacuum-outward', 'vacuum-inward')
        exact = terminal[i].tolist() == old['original_terminal_words']
        assert exact == authored['original_terminal_exact'] and exact_required == authored['exact_original_required']
        if exact_required: assert exact
        exact_count += int(exact_required)
        measured = same = absolute = None
        if i in refs:
            ref = refs[i]
            measured = errors(terminal[i], ref)
            assert all(np.isfinite(v) for v in measured.values())
            original_error = errors(old['original_terminal_words'], ref)
            assert original_error == ref['original_error']
            ceilings = {m: Q(original_error[m])+Q(ref['reference_convergence'][m]) for m in METRICS}
            assert all(ceilings[m] == Q(ref['per_metric_ceiling_exact'][m]) for m in METRICS)
            if route == 1:
                same = {m: Q(measured[m]) <= ceilings[m] for m in METRICS}
                assert all(same.values())
                same_count += 3
            if case.startswith('atmosphere-'):
                absolute = {m: Q(measured[m]) < Q(LIMITS[m]) for m in METRICS}
                assert all(absolute.values())
                absolute_count += 3
            maximum = {m: max(maximum[m], measured[m]) for m in METRICS}
        elif route == 1: assert exact_required
        assert measured == authored['candidate_error']
        assert same == authored['accepted_suffix_metric_checks'] and absolute == authored['absolute_checks']
        routes[NAMES[route]] += 1
        statuses[str(status)] += 1
        launches[tuple(detail[i, 4:7])] += 1
    assert dict(routes) == retained['result']['route_counts'] and dict(statuses) == retained['result']['status_counts']
    if case == 'fisheye': assert routes['masked'] > 0 and routes['masked'] < 23
    return {'case': case, 'passed': True, 'records': 23, 'route_counts': dict(routes), 'status_counts': dict(statuses),
            'same_ray_checks_passed': same_count, 'absolute_checks_passed': absolute_count,
            'exact_original_terminal_records_required_and_passed': exact_count,
            'maximum_errors': maximum if refs else None, 'retained_handoff_comparison_available': control is not None,
            'unique_launch_count': len(launches), 'launch_populations': sorted(launches.values()),
            'progress_words': progress.tolist(), 'input_hashes_checked': len(inputs), 'output_hashes_checked': len(outputs),
            'control_output_hashes_checked': len(controls or {}), 'retained_report_sha256': sha(prefix+'-independent-review.json')}


def run():
    rows = []
    for case in CASES:
        rows.append(audit_case(case))
        print(json.dumps({'case': case, 'passed': True, 'route_counts': rows[-1]['route_counts']}), flush=True)
    routes, statuses = Counter(), Counter()
    for row in rows:
        routes.update(row['route_counts'])
        statuses.update(row['status_counts'])
    script = Path(__file__).relative_to(ROOT).as_posix()
    sha(script)
    sha('scripts/verify_optical_dop853.py')
    return {'schema': 'cartesian-composite-nine-packet-independent-closure-v1',
            'passed': True, 'review_completed': True, 'cases': rows, 'case_count': len(rows), 'record_count': 207,
            'route_counts': dict(routes), 'status_counts': dict(statuses),
            'same_ray_checks_passed': sum(r['same_ray_checks_passed'] for r in rows),
            'absolute_checks_passed': sum(r['absolute_checks_passed'] for r in rows),
            'exact_original_terminal_records_required_and_passed': sum(r['exact_original_terminal_records_required_and_passed'] for r in rows),
            'sha256': {p.relative_to(ROOT).as_posix() if p.is_relative_to(ROOT) else p.as_posix(): h for p, h in sorted(CACHE.items())},
            'prospective_auditor_reexecutions': 0, 'ode_executions': 0, 'gpu_executions': 0, 'timing_interpreted': False,
            'limitations': ['Nine retained23-record packets; repeated launches are not independent physical cases.',
                            'Two5000m cases have no historical handoff capture; source/structure and final-output gates apply.',
                            'Returning actual-launch reference and observed fallback remain separate gates.',
                            'No full-view throughput, native moving-frame or production approval. Historical rejected experiments remain rejected.']}


if __name__ == '__main__':
    result = run()
    with (ROOT/OUTPUT).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps({k: result[k] for k in ('passed', 'case_count', 'record_count', 'route_counts', 'same_ray_checks_passed', 'absolute_checks_passed')}), flush=True)
