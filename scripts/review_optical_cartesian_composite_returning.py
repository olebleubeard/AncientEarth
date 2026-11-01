"""Returning composite gate; no reference integration or GPU execution.

The original/reference freeze is immutable. A candidate must actually hand
off and fall back on all 23 records, then reproduce every original terminal
word. There is no retained earlier candidate handoff to compare against.
"""
import argparse
import ast
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import zipfile

import numpy as np

import review_optical_cartesian_returning_baseline as original
from review_optical_cartesian_composite_matrix import handoff_structure

ROOT = Path(__file__).resolve().parents[1]
CASE = 'returning-gradient'
ORIGINAL = 'artifacts/optical-composite-f64-returning-reference-first'
FREEZE = 'artifacts/optical-composite-f64-returning-baseline-freeze-first'
SOURCE = 'artifacts/optical-composite-f64-returning-candidate-source-review-first.json'
POLICY = 'docs/science/cartesian-composite-f64-policy.md'
EVIDENCE = 'docs/science/cartesian-composite-matrix-evidence.md'
SCRIPT = 'scripts/review_optical_cartesian_composite_returning.py'
PARTS = ('-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin')


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def sha(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def key(path):
    return str(path).replace('\\', '/')


def checked(rows):
    hashes = {}
    for row in rows:
        path = key(row['path'])
        assert sha(path) == row['sha256'], path
        assert path not in hashes or hashes[path] == row['sha256']
        hashes[path] = row['sha256']
    return hashes


def source_proof():
    proof = load(SOURCE)
    assert proof['schema'] == 'cartesian-composite-returning-candidate-source-v1' and proof['passed']
    assert proof['five_uploads_exact'] and len(proof['modules']) == 3
    assert {r['name'] for r in proof['modules']} == {'prefix', 'suffix', 'fallback'}
    assert set(proof['uploaded_sha256']) == set(PARTS)
    for path, digest in proof['sha256'].items():
        assert sha(path) == digest, path
    for row in proof['modules']:
        assert row['whole_module_exact_declared_transform']
        assert sha(proof['candidate_prefix']+'-'+row['name']+'.wgsl') == row['sha256']
        assert sha(ORIGINAL+'-'+row['name']+'.wgsl') == row['original_sha256']
    for suffix, digest in proof['uploaded_sha256'].items():
        assert sha(proof['candidate_prefix']+suffix) == sha(ORIGINAL+suffix) == digest
    return proof


def original_archive():
    manifest = load(ORIGINAL+'-sources.json')
    archive = load(ORIGINAL+'-source-archive.json')
    assert sha(ORIGINAL+'-sources.json') == archive['source_manifest_sha256']
    assert sha(ORIGINAL+'-source.zip') == archive['archive_sha256']
    assert archive['all_entry_hashes_verified']
    expected = {}
    expected_rows = Counter()
    for row in manifest['files']:
        path = key(row['path'])
        assert path not in expected or expected[path] == row['sha256']
        expected[path] = row['sha256']
        expected_rows[(path, row['sha256'])] += 1
    retained_rows = Counter()
    seen, excluded, blobs = set(), set(), {}
    with zipfile.ZipFile(ROOT/(ORIGINAL+'-source.zip')) as bundle:
        assert json.loads(bundle.read('paths.json')) == archive['entries']
        for row in archive['entries']:
            path, blob = key(row['path']), row['archive_entry']
            assert path not in seen and expected[path] == row['sha256']
            seen.add(path)
            retained_rows[(path, row['sha256'])] += 1
            assert bundle.getinfo(blob).file_size == row['bytes']
            if blob not in blobs:
                with bundle.open(blob) as stream:
                    blobs[blob] = hashlib.file_digest(stream, 'sha256').hexdigest()
            assert blobs[blob] == row['sha256']
    for row in archive['not_in_archive']:
        path = key(row['path'])
        assert path not in seen and expected[path] == row['sha256']
        assert row['retention'] and sha(path) == row['sha256'], path
        excluded.add(path)
        retained_rows[(path, row['sha256'])] += 1
    assert seen | excluded == set(expected), 'Incomplete original archive coverage'
    assert retained_rows == expected_rows, 'Archive must preserve every manifest row, including identical repeated dependencies'
    compiled = load(ORIGINAL+'-compile-inputs.json')
    stable = load(ORIGINAL+'-input-stability.json')
    assert manifest['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert manifest['files'][:-1] == compiled['inputs'] == stable['after_run']
    life = load(ORIGINAL+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None and not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['arguments'] == ['--name', Path(ORIGINAL).name, '--cartesian-composite-f64', CASE, '--cartesian-composite-reference']
    assert manifest['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    assert key(manifest['files'][-1]['path']) == ORIGINAL+'-executable.exe'
    assert sha(ORIGINAL+'-executable.exe') == life['executable_sha256'] == manifest['files'][-1]['sha256']
    return expected, {'source_manifest_sha256': archive['source_manifest_sha256'],
        'archive_sha256': archive['archive_sha256'], 'archived_paths_verified': len(seen),
        'unique_payloads_verified': len(blobs), 'separately_retained_hashes_verified': len(excluded),
        'manifest_rows_verified': sum(expected_rows.values()), 'unique_manifest_paths': len(expected),
        'complete_manifest_coverage': True, 'historical_mutable_source_read_from_archive': True}


def frozen_original():
    proof = source_proof()
    expected, archive = original_archive()
    frozen, contract = load(FREEZE+'.json'), load(FREEZE+'-contract.json')
    assert frozen['schema'] == 'cartesian-composite-returning-original-freeze-v1'
    assert contract['schema'] == 'cartesian-composite-returning-original-contract-v1'
    assert frozen['case'] == contract['case'] == CASE and frozen['baseline'] == contract['baseline'] == ORIGINAL
    assert frozen['contract'] == FREEZE+'-contract.json' and sha(frozen['contract']) == frozen['contract_sha256']
    assert frozen['passed'] and frozen['eligible_for_returning_candidate'] and frozen['failure_reasons'] == []
    assert not frozen['candidate_readback_used'] and not contract['candidate_readback_used']
    assert frozen['input_hashes_unchanged_through_reference'] and not contract['old_fixed_path_endpoint_used']
    assert contract['prospective_policy'] == POLICY and contract['prospective_policy_sha256'] == sha(POLICY)
    assert contract['reference_source_sha256'] == original.ORACLE_SHA == sha(original.ORACLE)
    assert frozen['strict_reference_convergence_limits'] == contract['strict_reference_convergence_limits'] == original.CONVERGENCE_LIMITS
    for path, digest in contract['input_sha256'].items():
        if key(path) in expected:
            assert expected[key(path)] == digest
        else:
            assert sha(path) == digest, path
    output_hashes = checked(load(ORIGINAL+'-outputs.json'))
    life = load(FREEZE+'-lifecycle.json')
    assert life['exit_code'] == 0 and not life['timed_out'] and not life['terminated_by_wrapper'] and life.get('failure') is None
    assert life['arguments'] == ['-B', 'scripts/review_optical_cartesian_returning_baseline.py', ORIGINAL, '--output', FREEZE]
    report = load(ORIGINAL+'.json')
    terminal, detail, rows = original.raw_records(ORIGINAL, report)
    assert rows == contract['records'] and len(rows) == len(frozen['references']) == 23
    assert frozen['record_count'] == 23 and frozen['unique_reference_launches'] == 1
    local_launch = detail[0, 4:7].copy().view('<f4').astype(float)
    launch = original.BASIS@local_launch
    norm = float(np.linalg.norm(launch))
    assert np.isfinite(norm) and norm > 0
    inputs = dict(contract['canonical_inputs'])
    assert inputs == dict(original.declared_input(), initial_direction=launch.tolist())
    normalization = contract['reference_initial_normalization']
    assert normalization['actual_gpu_launch_norm'] == norm
    assert normalization['direction_correction_norm'] == float(np.linalg.norm(launch/norm-launch))
    coarse, tight = frozen['coarse_reference'], frozen['tight_reference']
    convergence, failures = original.assess_references(coarse, tight)
    assert not failures and convergence == frozen['reference_convergence']
    assert coarse['maximum_step_m'] == 2000. and tight['maximum_step_m'] == 500.
    assert coarse['relative_tolerance'] == 2e-12 and tight['relative_tolerance'] == 5e-14
    for ref in (coarse, tight):
        endpoint = np.asarray(ref['endpoint_ecef_m'])
        assert abs(float(np.linalg.norm(endpoint))-6378137.) < 1e-6
        assert float(endpoint@ref['exit_direction']) < 0
    errors = []
    for i, ref in enumerate(frozen['references']):
        assert all(ref[k] == rows[i][k] for k in rows[i])
        values = terminal[i].copy().view('<f4').astype(float)
        error = {'position_m': float(np.linalg.norm(original.ORIGIN+original.BASIS@values[:3]-tight['endpoint_ecef_m'])),
            'path_m': abs(float(values[3])-tight['geometric_path_m']),
            'direction': float(np.linalg.norm(original.BASIS@values[4:7]-tight['exit_direction']))}
        assert all(np.isfinite(x) for x in error.values()) and error == ref['original_error']
        errors.append(error)
    summary = {'schema': 'cartesian-composite-returning-original-independent-review-v1',
        'passed': True, 'original_reference_only': True, 'baseline': ORIGINAL,
        'freeze_sha256': sha(FREEZE+'.json'), 'contract_sha256': sha(FREEZE+'-contract.json'),
        'source_proof_sha256': sha(SOURCE), 'archive': archive,
        'original_output_hashes_verified': len(output_hashes), 'original_records': 23,
        'original_status': 'uncovered-h0 surface', 'original_routes': {'unhanded-original': 23},
        'original_attempt_histogram': dict(Counter(r['original_attempt_count'] for r in rows)),
        'actual_launch_words': detail[0, 4:7].tolist(), 'initial_normalization': normalization,
        'canonical_first_downward_h0_and_directed_turn': True, 'reference_convergence': convergence,
        'turn_path_m': tight['radial_turns'][0]['path_m'],
        'downward_1000m_source_path_m': tight['source_contacts'][0]['path_m'],
        'first_h0_path_m': tight['geometric_path_m'],
        'maximum_original_errors_descriptive': {m: max(r[m] for r in errors) for m in errors[0]},
        'all23_raw_reference_error_associations_exact': True,
        'gpu_executions': 0, 'ode_executions': 0,
        'scope': 'Original raw/reference closure only. One repeated physical orientation; normalization and canonical/packed model distinctions retained. No observed internal GPU turn, candidate fallback, global ODE enclosure or timing acceptance.'}
    return frozen, contract, proof, summary


def review(prefix):
    frozen, contract, proof, baseline = frozen_original()
    report = load(prefix+'.json')
    assert report['cartesian_composite_f64'] == CASE and not report['cartesian_composite_reference']
    assert report['cartesian_suffix_f64'] is None and not report['reference']
    assert not report['cartesian_suffix_trace'] and not report['cartesian_suffix_tight']
    assert report['selected'] and not report['timed'] and report['iterations'] == []
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['diagnostic_bytes_per_record'] == 32 and report['auxiliary_layout'] is None
    assert report['steps'] == load(ORIGINAL+'.json')['steps']
    assert set(re.findall(r'\b[A-Z][A-Z0-9_]*\b', report['required_features'])) == {'SHADER_F64', 'SHADER_INT64'}
    manifest, compiled, stable = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert manifest['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert manifest['files'][:-1] == compiled['inputs'] == stable['after_run']
    hashes = checked(manifest['files'])
    assert set(input_paths()) <= hashes.keys(), 'Original/reference evidence missing from pre-run inventory'
    outputs = checked(load(prefix+'-outputs.json'))
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None and not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-composite-f64', CASE]
    assert manifest['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    assert key(manifest['files'][-1]['path']) == prefix+'-executable.exe'
    assert manifest['files'][-1]['sha256'] == life['executable_sha256']
    for name in ('prefix', 'suffix', 'fallback'):
        assert (ROOT/(prefix+'-'+name+'.wgsl')).read_bytes() == (ROOT/(proof['candidate_prefix']+'-'+name+'.wgsl')).read_bytes()
    for part in PARTS:
        assert (ROOT/(prefix+part)).read_bytes() == (ROOT/(ORIGINAL+part)).read_bytes()
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    primary_words = np.fromfile(ROOT/(prefix+'-primary.bin'), dtype='<u4')
    assert raw.nbytes == report['output_bytes'] == 1856 and state.nbytes == report['state_bytes'] == 2208
    assert progress.nbytes == report['progress_bytes'] == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    assert np.array_equal(raw[:4], primary_words[44:48])
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(terminal.copy().view('<f4')[:, 8:11]).all()
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.array_equal(state[:, 21], np.arange(23))
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.all(state[:, 12:15] <= 4096) and np.all(state[:, 12].astype(np.uint64)+state[:, 13] <= 4096)
    work = state[:, 12:15].astype(np.uint64).sum(axis=1)
    assert progress.tolist() == report['progress_words'] == [0, 23, int(work.max()), 0]
    primary = primary_words.copy().view('<f4').reshape(140, 4).astype(float)
    assert len(report['samples']) == 23
    rows = []
    for i, sample in enumerate(report['samples']):
        old = contract['records'][i]
        assert sample['record_index'] == i and sample['pixel'] == old['pixel_label']
        assert sample['terminal_words'] == terminal[i].tolist() and sample['state_words'] == state[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist()
        route, reason = int(state[i, 22]), int(state[i, 15])
        assert route in (0, 1, 2, 3)
        structure = handoff_structure(state[i], primary) if route in (1, 2) else None
        failure = []
        if route != 2: failure.append('required-actual-handoff-fallback-missing')
        if route == 2 and (not np.all(state[i, 12:15] > 0) or reason not in (2, 3, 4, 5)):
            failure.append('invalid-fallback-count-or-reason')
        if detail[i, 4:7].tolist() != old['launch_words']: failure.append('actual-launch-differs')
        if terminal[i].tolist() != old['original_terminal_words']: failure.append('original-terminal-words-differ')
        rows.append({'record_index': i, 'pixel': old['pixel_label'], 'route': route, 'reason': reason,
                     'attempts': state[i, 12:15].tolist(), 'saved_handoff_structure': structure,
                     'retained_handoff_comparison_available': False,
                     'original_terminal_exact': terminal[i].tolist() == old['original_terminal_words'],
                     'passed': not failure, 'failure_reasons': failure})
    passed = all(r['passed'] for r in rows)
    return {'schema': 'cartesian-composite-returning-candidate-v1', 'candidate': prefix, 'case': CASE,
        'passed': passed, 'decision': 'returning-exact-original-fallback-pass-only' if passed else 'stop-composite-without-tuning',
        'original_reference_review': baseline, 'source_proof_sha256': sha(SOURCE),
        'candidate_input_hashes': hashes, 'candidate_output_hashes': outputs,
        'route_counts': dict(Counter(r['route'] for r in rows)), 'reason_counts': dict(Counter(r['reason'] for r in rows)),
        'records': rows, 'failed_records': [r['record_index'] for r in rows if not r['passed']],
        'retained_handoff_comparison_available': False, 'full_timing_or_production_authorized': False,
        'reviewer_source_sha256': sha(SCRIPT),
        'limitations': ['Canonical coarse/tight reference demonstrates the turn; no internal GPU suffix turn is observed.',
                       'Any valid unchanged fallback reason2/3/4/5 is permitted; no reason4-only claim.',
                       'Exact original fallback establishes published output parity, not promoted-solver accuracy.',
                       'No old candidate handoff capture exists; source and strict state checks do not invent raw identity.',
                       'Other matrix, complete-chain performance and native gates remain separate.']}


def input_paths():
    paths = {SCRIPT, POLICY, EVIDENCE, SOURCE, FREEZE+'.json', FREEZE+'-contract.json', FREEZE+'-lifecycle.json',
             ORIGINAL+'-source.zip', ORIGINAL+'-source-archive.json', ORIGINAL+'-outputs.json'}
    proof = load(SOURCE)
    paths.update(proof['sha256'])
    paths.update(key(r['path']) for r in load(ORIGINAL+'-outputs.json'))
    paths.update(key(r['path']) for r in load(ORIGINAL+'-source-archive.json')['not_in_archive'])
    source_paths = {key(r['path']) for r in load(ORIGINAL+'-sources.json')['files']}
    paths.update(p for p in load(FREEZE+'-contract.json')['input_sha256'] if key(p) not in source_paths)
    normalized = set()
    for path in paths:
        p = Path(path)
        normalized.add(p.relative_to(ROOT).as_posix() if p.is_relative_to(ROOT) else p.as_posix())
    paths = normalized
    queue = [p for p in paths if p.startswith('scripts/') and p.endswith('.py')]
    seen = set()
    while queue:
        path = queue.pop()
        if path in seen: continue
        seen.add(path)
        for node in ast.walk(ast.parse((ROOT/path).read_text(encoding='utf-8-sig'))):
            modules = [a.name for a in node.names] if isinstance(node, ast.Import) else [node.module] if isinstance(node, ast.ImportFrom) else []
            for module in modules:
                local = 'scripts/'+module.split('.')[0]+'.py' if module else ''
                if local and (ROOT/local).is_file():
                    paths.add(local)
                    queue.append(local)
    assert all((ROOT/path).is_file() for path in paths)
    return sorted(paths)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='?')
    parser.add_argument('--check-original', action='store_true')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    assert bool(args.prefix) != args.check_original
    result = frozen_original()[3] if args.check_original else review(args.prefix)
    with (ROOT/args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps({'passed': result['passed'], 'scope': result['schema']}))
