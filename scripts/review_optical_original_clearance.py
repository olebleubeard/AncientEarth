"""Exact original-terminal and complete-chain control audit; no physical oracle.

Selected and full packets use the same checks. Every raw record participates;
only the descriptive JSON row list is sampled for a full packet. Historical
baseline source/binary provenance is retained, not asserted to be current.
"""
import argparse
from decimal import Decimal
import hashlib
import json
import math
from pathlib import Path

import numpy as np

from test_optical_original_clearance_source import audit as assembled_audit, check


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify_rows(rows):
    for row in rows:
        assert digest(row['path']) == row['sha256'], row['path']


def same_bytes(a, b):
    with Path(a).open('rb') as left, Path(b).open('rb') as right:
        while True:
            x, y = left.read(1 << 20), right.read(1 << 20)
            if x != y:
                return False
            if not x:
                return True


def raw_records(prefix):
    report = load(prefix + '.json')
    n = report['record_count']
    assert n > 0 and report['diagnostic_bytes_per_record'] == 32
    assert report['terminal_bytes'] == 16 + 48*n
    assert report['output_bytes'] == 16 + 80*n
    assert Path(prefix + '-readback.bin').stat().st_size == report['output_bytes']
    raw = np.memmap(prefix + '-readback.bin', dtype='<u4', mode='r')
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    assert report['header_matches']
    terminal = raw[4:4+12*n].reshape(n, 12)
    detail = raw[4+12*n:].reshape(n, 8)
    ids = set()
    for sample in report['samples']:
        i = sample['record_index']
        assert 0 <= i < n and i not in ids
        ids.add(i)
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist()
    if report['selected']:
        assert ids == set(range(n))
    for indices in ([0, 1, 2, 3, 4, 5, 6], [8, 9, 10]):
        assert np.isfinite(terminal[:, indices].copy().view('<f4')).all()
    return report, raw, terminal, detail


def timestamps(report):
    assert report['timed'] and report['strictly_increasing_unique_timestamp_pairs']
    rows = report['iterations']
    assert len(rows) == 8
    prior = 0
    retained = []
    for i, row in enumerate(rows):
        assert row['iteration'] == i and row['warmup'] == (i < 3)
        a, b = row['raw_timestamps']
        assert 0 < a < b and a > prior
        prior = b
        period = Decimal(str(row['timestamp_period_ns']))
        assert period.is_finite() and period > 0
        seconds = Decimal(b-a)*period/Decimal(10**9)
        observed = row['gpu_seconds']
        assert math.isfinite(observed) and observed > 0
        assert abs(float(seconds)-observed) <= 2*math.ulp(observed)
        if i >= 3:
            retained.append(seconds*1000)
    return retained


def review(candidate, baseline):
    a, ar, at, ad = raw_records(candidate)
    b, br, bt, bd = raw_records(baseline)
    n = a['record_count']
    assert a['original_clearance'] and not a['reference']
    assert not a['cartesian_suffix'] and not a['reduce_bookkeeping']
    assert a['auxiliary_layout'] is None and a['partial_layout'] is None
    for key in ('record_count', 'dimensions', 'selected', 'output_bytes',
                'terminal_bytes', 'diagnostic_bytes_per_record', 'adapter'):
        assert a[key] == b[key], key
    assert b['reference']
    for flag in ('finite_loops', 'reference_reuse', 'reference_carry', 'uniform_loops'):
        assert not b.get(flag, False), 'Baseline must use the ordinary original: '+flag
    assert [(s['record_index'], s['pixel']) for s in a['samples']] == [
        (s['record_index'], s['pixel']) for s in b['samples']]
    assert a['modules'] == ['fast', 'fallback'] and a['state_words_per_record'] == 4
    expected_steps = [
        (0, 'oc_fast', [(n+31)//32, 1, 1]),
        (1, 'oc_fallback', [(n+31)//32, 1, 1]),
        (1, 'oc_validate', [((n+255)//256+31)//32, 1, 1]),
        (1, 'oc_finalize', [1, 1, 1]),
    ]
    assert a['steps'] == [dict(module_index=m, entry=e, dispatch=d, repetitions=1)
                          for m, e, d in expected_steps]
    provenance = {}
    for p in (candidate, baseline):
        stability = load(p+'-input-stability.json')
        assert stability['inputs_unchanged_through_run']
        outputs = load(p+'-outputs.json')
        verify_rows(outputs)
        sources = load(p+'-sources.json')
        assert sources['compiler_inputs_unchanged']
        provenance[p] = {
            'sources_sha256': digest(p+'-sources.json'),
            'input_stability_sha256': digest(p+'-input-stability.json'),
            'outputs_manifest_sha256': digest(p+'-outputs.json'),
            'outputs': outputs,
        }
    baseline_lifecycle = Path(baseline+'-lifecycle.json')
    # The old selected checkpoint predates lifecycle records; the fresh full
    # matched baseline must have one. Never require an old cache binary today.
    if baseline_lifecycle.exists():
        original_life = load(baseline_lifecycle)
        assert original_life['exit_code'] == 0 and original_life['failure'] is None
        assert not original_life['timed_out'] and not original_life['terminated_by_wrapper']
        provenance[baseline]['lifecycle_sha256'] = digest(baseline_lifecycle)
        provenance[baseline]['lifecycle_success_verified'] = True
    else:
        assert a['selected'] and Path(baseline).name == 'optical-normal-scene-reference-selected'
        provenance[baseline]['lifecycle_success_verified'] = False
        provenance[baseline]['lifecycle_scope'] = 'Historical selected reference has no retained lifecycle record. This exception is unavailable for a full matched baseline.'
    sources = load(candidate+'-sources.json')
    stability = load(candidate+'-input-stability.json')
    compilation = load(candidate+'-compile-inputs.json')
    assert sources['files'][:-1] == stability['after_run'] == compilation['inputs']
    verify_rows(sources['files'])
    assert sources['executable_retained']
    life = load(candidate+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None
    assert not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['executable_retained'] and life['executable_unchanged_through_run']
    assert digest(life['executable']) == life['executable_sha256'] == sources['files'][-1]['sha256']
    assert Path(life['executable']).resolve() == Path(sources['files'][-1]['path']).resolve()
    provenance[candidate]['current_source_and_executable_hashes'] = sources['files']
    provenance[candidate]['lifecycle_sha256'] = digest(candidate+'-lifecycle.json')
    validation = load(candidate+'-validation.json')
    assert validation['all_modules_valid'] and validation['module_count'] == 2
    assert validation['modules'] == a['modules'] and validation['state_words_per_record'] == 4
    uploads = {}
    for name in ('primary', 'geometry', 'frames', 'field', 'certificates'):
        x, y = candidate+'-'+name+'.bin', baseline+'-'+name+'.bin'
        assert same_bytes(x, y), name
        uploads[name] = {'sha256': digest(x), 'bytes': Path(x).stat().st_size}
    assert same_bytes(candidate+'.wgsl', candidate+'-fast.wgsl')
    retained = check(Path(baseline+'.wgsl').read_text(),
                     Path(candidate+'-fast.wgsl').read_text(),
                     Path(candidate+'-fallback.wgsl').read_text())
    assembly = assembled_audit(Path(candidate), full=not a['selected'])
    assert a['state_bytes'] == 16*n == Path(candidate+'-states.bin').stat().st_size
    assert a['progress_bytes'] == 16 == Path(candidate+'-progress.bin').stat().st_size
    state = np.memmap(candidate+'-states.bin', dtype='<u4', mode='r').reshape(n, 4)
    progress = np.fromfile(candidate+'-progress.bin', dtype='<u4').tolist()
    assert (state[:, 0] == 0).all()
    assert (state[:, 1:3] <= 4096).all() and (state[:, 3] <= 2).all()
    assert np.array_equal(ad[:, :2], state[:, 1:3])
    assert np.array_equal(ad[:, 2], state[:, 3]) and (ad[:, 3] == 0).all()
    assert (ad[:, 7] == 0).all()
    route = state[:, 3]
    fast, fallback, masked = route == 0, route == 1, route == 2
    assert (state[fast, 2] == 0).all() and (at[fast, 7] <= 2).all()
    assert (state[fallback, 1:3] > 0).all() and (at[fallback, 7] <= 2).all()
    assert (state[masked, 1:3] == 0).all() and (at[masked, 7] == 3).all()
    assert (ad[masked, 4:] == 0).all()
    work = state[:, 1].astype(np.uint64) + state[:, 2]
    assert progress == a['progress_words'] == [0, n, int(work.max()), 0]
    for sample in a['samples']:
        assert sample['state_words'] == state[sample['record_index']].tolist()
    status_counts = {str(int(x)): int(y) for x, y in zip(*np.unique(at[:, 7], return_counts=True))}
    assert status_counts == a['raw_status_counts']
    changed = np.count_nonzero(at != bt, axis=1)
    launch_changed = np.count_nonzero(ad[:, 4:] != bd[:, 4:], axis=1)
    bad_rows = np.flatnonzero((changed > 0) | (launch_changed > 0))
    exact = np.array_equal(ar[:4], br[:4]) and not len(bad_rows)
    coverage = bool(fast.any() and fallback.any())
    indices = list(range(n)) if a['selected'] else sorted(set(
        [s['record_index'] for s in a['samples']] + bad_rows[:64].tolist()))
    route_counts = {str(int(x)): int(y) for x, y in zip(*np.unique(route, return_counts=True))}
    result = {
        'schema': 'original-clearance-runtime-review-v1', 'candidate': candidate,
        'baseline': baseline, 'auditor_sha256': digest(__file__),
        'scope': __doc__.strip(), 'record_count': n,
        'all_original_terminal_words_equal': bool((changed == 0).all()),
        'terminal_word_count': 12*n, 'terminal_word_differences': int(changed.sum()),
        'header_equal': bool(np.array_equal(ar[:4], br[:4])),
        'launch_and_reserved_tail_words_equal': bool((launch_changed == 0).all()),
        'launch_xyz_word_count': 3*n, 'launch_tail_word_differences': int(launch_changed.sum()),
        'changed_record_count': len(bad_rows), 'changed_record_indices_first64': bad_rows[:64].tolist(),
        'all_five_uploads_exact': uploads, 'source_reversal_functions': retained,
        'fixed_original_assembly_and_corruption_checks': assembly,
        'route_counts': route_counts, 'status_counts': status_counts,
        'route_status_counts': {str(r): {str(int(x)): int(y) for x, y in zip(
            *np.unique(at[route == r, 7], return_counts=True))} for r in range(3)},
        'progress_recomputed': progress, 'all_state_diagnostic_associations_valid': True,
        'positive_fast_and_fallback_populations': coverage,
        'attempt_work': {'fast_sum': int(state[:, 1].sum(dtype=np.uint64)),
                         'fallback_sum': int(state[:, 2].sum(dtype=np.uint64)),
                         'total_sum': int(work.sum()), 'maximum': int(work.max())},
        'source_and_output_provenance': provenance,
        'baseline_provenance_scope': 'Historical frozen manifests and retained output hashes checked. Current historical source/cache-executable identity is not asserted.',
        'pipeline_creation': a['pipeline_creation'],
        'parity_and_coverage_gate_passed': exact and coverage,
        'performance_gate': None, 'production_adoption': False,
        'records_are_complete': bool(a['selected']),
        'records': [dict(record_index=i, state=state[i].tolist(),
                         terminal=at[i].tolist(), original_terminal=bt[i].tolist(),
                         diagnostic=ad[i].tolist()) for i in indices],
    }
    if a['timed'] and exact and coverage:
        assert not a['selected'] and b['timed']
        ct, btms = timestamps(a), timestamps(b)
        cm, bm = sorted(ct)[2], sorted(btms)[2]
        material, separated = cm <= Decimal('.8')*bm, max(ct) < min(btms)
        result['performance_gate'] = {
            'candidate_retained_ms': list(map(str, ct)), 'baseline_retained_ms': list(map(str, btms)),
            'candidate_median_ms': str(cm), 'baseline_median_ms': str(bm),
            'median_reduction_percent': str(100*(bm-cm)/bm),
            'twenty_percent_gate_passed': material, 'ranges_separated': separated,
            'passed': exact and coverage and material and separated,
            'scope': 'Whole four-dispatch chain including initialization/discarded prefixes/fallback/validation. Excludes compilation, readback, shading and native presentation.'}
    elif a['timed']:
        result['performance_gate'] = {
            'passed': False,
            'interpretation_withheld': 'Terminal/launch parity or positive fast/fallback coverage failed. Raw timestamps remain in the retained reports; no conversion, reduction or range comparison is performed.'}
    else:
        assert a['iterations'] == []
    result['passed'] = exact and coverage and (
        result['performance_gate'] is None or result['performance_gate']['passed'])
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('candidate')
    parser.add_argument('baseline')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    answer = review(args.candidate, args.baseline)
    with Path(args.output).open('x', encoding='utf-8') as stream:
        json.dump(answer, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps({k: answer[k] for k in ('passed', 'record_count', 'terminal_word_differences',
        'launch_tail_word_differences', 'route_counts', 'progress_recomputed', 'performance_gate')}, indent=2))
