"""One frozen full-frame correctness and complete-chain effort decision.

All records participate. Existing independent matrix closures and selected
canonical references are reused; no reference solve or new numerical limit.
Timestamp conversion is unreachable until every correctness predicate passes.
"""
import argparse
import ast
from collections import Counter
from decimal import Decimal
from fractions import Fraction as Q
import json
from pathlib import Path
import re

import numpy as np
import review_optical_cartesian_suffix_f64 as fixed
from review_optical_original_clearance import raw_records, same_bytes, timestamps

ROOT, load, digest = fixed.ROOT, fixed.load, fixed.digest
POLICY = 'docs/science/cartesian-composite-full-decision.md'
SOURCE = 'artifacts/optical-composite-f64-full-source-review-first.json'
VALIDATION = 'artifacts/optical-composite-f64-full-validation-second'
N = 1920*1080
UPLOADS = ('primary', 'field', 'geometry', 'frames', 'certificates')
CLOSURES = {
    'artifacts/optical-composite-f64-atmosphere-first-closure-review.json': '049b8381235b8efa416566d1aa50351dc96bba8e4f9bcb152adf16c84c1705f1',
    'artifacts/optical-composite-f64-matrix-first-closure-review.json': '7aa94c8d7422ac97c9bcb0e2022975414de53a649429d1eede4b650fbc44851b',
    'artifacts/optical-composite-f64-returning-first-closure-review.json': '715ebb3d12090274f700b019944f8b2d416138e9079b62b3743d88ece96044f0',
}


def prerequisites():
    for path, sha in CLOSURES.items():
        assert digest(path) == sha, path
    first, matrix, returning = map(load, CLOSURES)
    assert first['review_completed'] and first['one_atmosphere_case_passed']
    assert first['frozen_auditor_replay_exact'] and not first['timing_interpreted']
    assert matrix['passed'] and matrix['review_completed'] and matrix['case_count'] == 9
    assert matrix['record_count'] == 207 and not matrix['timing_interpreted']
    assert returning['passed'] and returning['all_records_actual_handoff_then_original_fallback']
    assert returning['exact_terminal_words'] == 276 and returning['exact_launch_words'] == 69
    assert not returning['timing_interpreted']
    return CLOSURES.copy()


def source_gate(candidate, baseline):
    proof = load(SOURCE)
    assert proof['schema'] == 'cartesian-composite-full-source-review-v1' and proof['passed']
    assert proof['candidate_prefix'] == VALIDATION
    assert proof['all_three_modules_approved'] and proof['five_uploads_exact']
    for path, sha in proof['sha256'].items():
        assert digest(path) == sha, path
    assert sorted(m['name'] for m in proof['modules']) == ['fallback', 'prefix', 'suffix']
    for row in proof['modules']:
        suffix = '-'+row['name']+'.wgsl'
        assert digest(candidate+suffix) == digest(VALIDATION+suffix) == row['sha256']
    assert same_bytes(candidate+'.wgsl', candidate+'-prefix.wgsl')
    assert same_bytes(baseline+'.wgsl', proof['original_wgsl']), 'Fresh baseline is not the approved true original'
    assert set(proof['uploaded_sha256']) == {'-'+x+'.bin' for x in UPLOADS}
    for name in UPLOADS:
        suffix = '-'+name+'.bin'
        assert same_bytes(candidate+suffix, baseline+suffix)
        assert digest(candidate+suffix) == digest(VALIDATION+suffix) == proof['uploaded_sha256'][suffix]
    valid = load(candidate+'-validation.json')
    assert valid['all_modules_valid'] and valid['module_count'] == 3
    assert valid['cartesian_composite_full'] and not valid['selected']
    assert valid['state_words_per_record'] == 24
    return {'path': SOURCE, 'sha256': digest(SOURCE), 'all_modules_and_five_uploads_exact': True}


def provenance(prefix, candidate):
    source, compiled, stability = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert source['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    assert source['files'][:-1] == compiled['inputs'] == stability['after_run']
    # The baseline's shared cache executable may be overwritten by the next
    # compiler; require its immediately retained exact copy instead.
    hashes = fixed.checked_hashes(source['files'][:-1])
    executable = source['files'][-1]
    assert digest(prefix+'-executable.exe') == executable['sha256']
    outputs = fixed.checked_hashes(load(prefix+'-outputs.json'))
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None
    assert not life['timed_out'] and not life['terminated_by_wrapper']
    mode = '--cartesian-composite-full' if candidate else '--reference'
    assert life['arguments'] == ['--name', Path(prefix).name, '--full', '--time', mode]
    if candidate:
        assert source['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
        assert executable['path'].replace('\\', '/') == prefix+'-executable.exe'
        assert life['executable_sha256'] == executable['sha256']
        assert set(input_paths()) <= hashes.keys(), 'Unfrozen audit input'
    return {'sources_sha256': digest(prefix+'-sources.json'),
            'outputs_manifest_sha256': digest(prefix+'-outputs.json'),
            'lifecycle_sha256': digest(prefix+'-lifecycle.json'),
            'input_stability_sha256': digest(prefix+'-input-stability.json'),
            'retained_executable_sha256': executable['sha256'],
            'input_hashes_verified': len(hashes), 'output_hashes_verified': len(outputs)}


def structure(state, terminal, detail, primary):
    """Vectorized necessary state/owner conditions, not a field replay."""
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.array_equal(state[:, 21], np.arange(N, dtype=np.uint32))
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.all(state[:, 12:15] <= 4096)
    assert np.all(state[:, 12].astype(np.uint64)+state[:, 13] <= 4096)
    values = state.view('<f4')
    assert np.isfinite(values[:, :11]).all() and np.isfinite(values[:, 16:20]).all()
    route = state[:, 22]
    assert np.all(route <= 3)
    handed = (route == 1) | (route == 2)
    plain = ~handed
    assert np.all(state[plain, 13:16] == 0)
    assert np.all(state[plain, :8] == 0) and np.all(state[plain, 16:20] == 0)
    assert np.all(state[plain, 20] == np.uint32(0xfffffffe))
    assert np.all(state[handed, 12:14] > 0)
    assert np.all(terminal[route == 1, 7] == 0) and np.all(state[route == 1, 14:16] == 0)
    assert np.all(state[route == 2, 14] > 0) and np.isin(state[route == 2, 15], [2, 3, 4, 5]).all()
    assert np.all(terminal[route == 3, 7] == 3) and np.all(state[route == 3, 12:15] == 0)
    assert np.all(terminal[route == 0, 7] <= 2)
    count = float(primary[9, 0])
    assert count.is_integer() and 2 <= count <= 128
    layers = primary[12:12+int(count)]
    assert np.isfinite(layers).all() and np.all(np.diff(layers[:, 0]) > 0)
    assert primary[10, 1] == layers[0, 0] and primary[9, 1] == layers[-1, 0]
    owner = state[handed, 20].view('<i4').astype(np.int64)
    assert np.all((-1 <= owner) & (owner < int(count)-1))
    low = np.where(owner == -1, primary[10, 2], layers[np.maximum(owner, 0), 0])
    high = np.where(owner == -1, primary[10, 1], layers[owner+1, 0])
    height = values[handed, 19]
    assert np.all((low < height) & (height < high))
    assert np.all(values[handed, 3] >= 0) and np.all(values[handed, 7] >= 0)
    assert np.all(np.any(values[handed, 4:7] != 0, axis=1))
    assert np.all(np.any(values[handed, 16:19] != 0, axis=1))
    return state[:, 12:15].astype(np.uint64).sum(axis=1)


def selected_samples(terminal, detail, state, original_terminal):
    config, policy, _, contract, refs, _, old_state = fixed.frozen_data('selected')
    fixed.checked_hashes(load(config['control']+'-outputs.json'))
    # Logical record number is explicitly remapped. All physical components,
    # saved owner, prefix attempts and reserved word retain exact identity.
    columns = list(range(13))+list(range(16, 21))+[23]
    rows = []
    for i, old in enumerate(contract['records']):
        pixel = old.get('pixel', old.get('pixel_label'))
        index = pixel[1]*1920+pixel[0]
        assert 0 <= index < N and int(old_state[i, 21]) == i and int(state[index, 21]) == index
        assert original_terminal[index].tolist() == old['original_terminal_words']
        assert detail[index, 4:7].tolist() == old['launch_words']
        assert np.array_equal(state[index, columns], old_state[i, columns]), ('Physical handoff or prefix drift', i)
        route = int(state[index, 22])
        errors = fixed.exact_errors(terminal[index], refs[i]) if i in refs else None
        result = fixed.evaluate_selected_record(policy, i, fixed.NAMES[route], terminal[index].tolist(), detail[index, 4:7].tolist(), errors)
        checks = None
        if route == 1:
            assert i in refs
            checks = {m: Q(errors[m]) <= Q(policy['records'][i]['per_metric_error_ceiling_exact'][m]) for m in fixed.METRICS}
            assert all(checks.values()), ('Frozen metric failed', i, checks)
        assert result['passed'], (i, result)
        rows.append({'selected_index': i, 'full_index': index, 'pixel': pixel,
                     'route': fixed.NAMES[route], 'physical_handoff_and_prefix_exact': True,
                     'candidate_error': errors, 'frozen_metric_checks': checks, 'passed': True})
    return rows


def review(candidate, baseline):
    prerequisite_hashes = prerequisites()
    sources = source_gate(candidate, baseline)
    provenance_checks = {p: provenance(p, p == candidate) for p in (baseline, candidate)}
    a, ar, at, ad = raw_records(candidate)
    b, br, bt, bd = raw_records(baseline)
    assert a['cartesian_composite_full'] and not a['reference'] and a['timed'] and b['timed']
    assert not a['selected'] and not b['selected'] and a['record_count'] == b['record_count'] == N
    assert a['dimensions'] == b['dimensions'] == [1920, 1080] and a['adapter'] == b['adapter']
    assert b['reference']
    for flag in ('finite_loops', 'reference_reuse', 'reference_carry', 'uniform_loops', 'prune_unused'):
        assert not b.get(flag, False), flag
    for flag in ('original_clearance', 'reduce_bookkeeping', 'cartesian_suffix', 'cartesian_suffix_trace', 'cartesian_suffix_tight', 'cartesian_composite_reference', 'branch_reference'):
        assert not a[flag], flag
    assert a['cartesian_composite_f64'] is None and a['cartesian_suffix_f64'] is None and a['cartesian_suffix_branch'] is None
    assert a['auxiliary_layout'] is None and a['partial_layout'] is None and a['slice_attempts'] is None
    assert a['modules'] == ['prefix', 'suffix', 'fallback'] and a['state_words_per_record'] == 24
    features = set(re.findall(r'\b[A-Z][A-Z0-9_]*\b', a['required_features']))
    assert features == {'SHADER_F64', 'SHADER_INT64', 'TIMESTAMP_QUERY'}
    expected = [(0, 'cs_prefix', [(N+31)//32, 1, 1]), (1, 'cs_suffix', [(N+31)//32, 1, 1]),
                (2, 'cs_fallback', [(N+31)//32, 1, 1]), (2, 'cs_validate', [((N+255)//256+31)//32, 1, 1]),
                (2, 'cs_finalize', [1, 1, 1])]
    assert a['steps'] == [dict(module_index=m, entry=e, dispatch=d, repetitions=1) for m, e, d in expected]
    assert 'No pre-pass progress clear.' in a['timing_scope']
    assert a['fixture']['cartesian_composite_full']['progress_reset_inside_timed_sequence']
    assert not a['fixture']['cartesian_composite_full']['host_progress_clear_required']
    assert a['state_bytes'] == 96*N == (ROOT/(candidate+'-states.bin')).stat().st_size
    assert a['progress_bytes'] == 16 == (ROOT/(candidate+'-progress.bin')).stat().st_size
    state = np.memmap(ROOT/(candidate+'-states.bin'), dtype='<u4', mode='r').reshape(N, 24)
    primary_words = np.fromfile(ROOT/(candidate+'-primary.bin'), dtype='<u4')
    assert np.array_equal(ar[:4], br[:4]) and np.array_equal(ar[:4], primary_words[44:48])
    work = structure(state, at, ad, primary_words.view('<f4').reshape(140, 4).astype(float))
    progress = np.fromfile(ROOT/(candidate+'-progress.bin'), dtype='<u4').tolist()
    assert progress == a['progress_words'] == [0, N, int(work.max()), 0]
    assert np.array_equal(ad[:, 4:7], bd[:, 4:7]), 'Full launch mismatch'
    assert np.array_equal(at[:, 7], bt[:, 7]), 'Full terminal class mismatch'
    route = state[:, 22]
    original = route != 1
    assert np.array_equal(at[original], bt[original]), 'Original route terminal mismatch'
    assert np.array_equal(at[~original, 8:12], bt[~original, 8:12]), 'Sky normal or owner mismatch'
    assert bool(original.any() and (~original).any()), 'No positive composite/original populations'
    assert np.all(at[:, 7] <= 3)
    statuses = {str(int(k)): int(v) for k, v in zip(*np.unique(at[:, 7], return_counts=True))}
    assert statuses == a['raw_status_counts']
    for sample in a['samples']:
        assert sample['state_words'] == state[sample['record_index']].tolist()
        assert sample['pixel'] == [sample['record_index'] % 1920, sample['record_index']//1920]
    samples = selected_samples(at, ad, state, bt)
    assert sorted(s['record_index'] for s in a['samples']) == sorted(r['full_index'] for r in samples)
    # No timestamp values are converted, compared or summarized above here.
    ct, ot = timestamps(a), timestamps(b)
    cm, om = sorted(ct)[2], sorted(ot)[2]
    material, separated = cm <= Decimal('.8')*om, max(ct) < min(ot)
    cost = {'candidate_retained_ms': list(map(str, ct)), 'original_retained_ms': list(map(str, ot)),
            'candidate_median_ms': str(cm), 'original_median_ms': str(om),
            'median_reduction_percent': str(100*(om-cm)/om),
            'twenty_percent_gate_passed': material, 'ranges_separated': separated, 'passed': material and separated}
    return {'schema': 'cartesian-composite-full-decision-v1', 'candidate': candidate, 'baseline': baseline,
            'passed': cost['passed'], 'correctness_passed': True,
            'decision': 'complete-chain-effort-pass-only' if cost['passed'] else 'reject-without-tuning',
            'policy_sha256': digest(POLICY), 'source_gate': sources, 'prerequisite_closures': prerequisite_hashes,
            'provenance': provenance_checks, 'record_count': N,
            'all_launch_words_exact': True, 'all_final_classes_exact': True,
            'original_route_terminal_words_exact': 12*int(original.sum()),
            'accepted_sky_normal_and_owner_words_exact': 4*int((~original).sum()),
            'route_counts': {fixed.NAMES[int(k)]: int(v) for k, v in zip(*np.unique(route, return_counts=True))},
            'status_counts': statuses, 'progress': progress,
            'attempt_sums': state[:, 12:15].sum(axis=0, dtype=np.uint64).tolist(),
            'all_state_and_uploaded_owner_checks_passed': True, 'samples': samples,
            'performance_gate': cost, 'production_or_smooth_flight_claim': False,
            'scope': 'Complete compute sequence including initialization and all discarded/fallback work. Excludes compilation, readback, shading and native presentation. Canonical error gates cover retained finite samples/matrix, not every sky pixel.',
            'auditor_sha256': digest('scripts/review_optical_cartesian_composite_full.py')}


def input_paths():
    paths = {POLICY, SOURCE, 'scripts/review_optical_cartesian_composite_full.py', *CLOSURES}
    proof = load(SOURCE)
    paths.update(p.replace('\\', '/') for p in proof['sha256'])
    paths.add(proof['original_wgsl'])
    paths.update(VALIDATION+'-'+x+'.wgsl' for x in ('prefix', 'suffix', 'fallback'))
    paths.update(VALIDATION+'-'+x+'.bin' for x in UPLOADS)
    config, policy, _, contract, _, _, _ = fixed.frozen_data('selected')
    paths.update([fixed.POLICY, policy['baseline_report'], policy['initial_contract'], policy['policy_source']])
    paths.update(p.replace('\\', '/') for p in contract['input_sha256'])
    paths.update([config['control']+'-outputs.json', config['control']+'-states.bin', config['control']+'-readback.bin'])
    paths.update(r['path'].replace('\\', '/') for r in load(config['control']+'-outputs.json'))
    queue, visited = ['scripts/review_optical_cartesian_composite_full.py'], set()
    while queue:
        path = queue.pop()
        if path in visited: continue
        visited.add(path); paths.add(path)
        for node in ast.walk(ast.parse((ROOT/path).read_text(encoding='utf-8-sig'))):
            names = [a.name for a in node.names] if isinstance(node, ast.Import) else [node.module] if isinstance(node, ast.ImportFrom) else []
            for name in names:
                local = 'scripts/'+name.split('.')[0]+'.py' if name else ''
                if local and (ROOT/local).is_file(): queue.append(local)
    assert all((ROOT/p).is_file() for p in paths)
    return sorted(paths)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('candidate', nargs='?')
    parser.add_argument('baseline', nargs='?')
    parser.add_argument('--output')
    parser.add_argument('--check-inputs', action='store_true')
    args = parser.parse_args()
    if args.check_inputs:
        prerequisites()
        print(json.dumps({'prerequisites_passed': True, 'input_paths': input_paths()}))
    else:
        assert args.candidate and args.baseline and args.output
        try:
            answer = review(args.candidate, args.baseline)
        except Exception as error:
            answer = {'schema': 'cartesian-composite-full-decision-v1', 'candidate': args.candidate,
                      'baseline': args.baseline, 'passed': False, 'correctness_passed': False,
                      'decision': 'reject-without-tuning', 'performance_gate': None,
                      'failure': type(error).__name__+': '+str(error),
                      'auditor_sha256': digest('scripts/review_optical_cartesian_composite_full.py')}
        with (ROOT/args.output).open('x', encoding='utf-8') as stream:
            json.dump(answer, stream, indent=2, allow_nan=False); stream.write('\n')
        print(json.dumps({k: answer.get(k) for k in ('passed', 'correctness_passed', 'decision', 'route_counts', 'performance_gate', 'failure')}))
