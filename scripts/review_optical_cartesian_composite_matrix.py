"""Frozen final-output gates for the nine remaining original matrix packets.

No ODE solves, tolerance fitting, GPU dispatch or timing. Historical handoff
captures are used only where they actually exist. The original full-trace
freeze supplies terminal and canonical metric gates for every packet.
Returning-gradient has its own prospective original/reference gate.
"""
import argparse
import ast
from collections import Counter
from fractions import Fraction as Q
from pathlib import Path
import re

import numpy as np

import review_optical_cartesian_composite as initial
import review_optical_cartesian_matrix_candidate as historical
from verify_optical_dop853 import differences

fixed = initial.fixed
ROOT, load, digest = fixed.ROOT, fixed.load, fixed.digest
POLICY = initial.POLICY
EVIDENCE = 'docs/science/cartesian-composite-matrix-evidence.md'
SOURCE = 'artifacts/optical-composite-f64-matrix-source-review-first.json'
CLOSURE = 'artifacts/optical-composite-f64-atmosphere-first-closure-review.json'
CASES = ('readiness', 'fisheye', 'vacuum-outward', 'vacuum-inward',
         'atmosphere-minus20-vacuum', 'atmosphere-minus20-gradient',
         'atmosphere-100-vacuum', 'atmosphere-5000-vacuum', 'atmosphere-5000-gradient')
NO_HANDOFF_CAPTURE = ('atmosphere-5000-vacuum', 'atmosphere-5000-gradient')
CONTROL = {case: 'artifacts/optical-cartesian-matrix-'+case+'-candidate-first'
           for case in CASES if case not in NO_HANDOFF_CAPTURE}


def initial_closure():
    """Check retained initial closure, without rerunning a historical host."""
    closure = load(CLOSURE)
    assert closure['review_completed'] and closure['one_atmosphere_case_passed']
    assert closure['frozen_auditor_replay_exact'] and not closure['timing_interpreted']
    assert closure['same_ray_checks_passed'] == closure['strict_absolute_checks_passed'] == 69
    for path, sha in closure['sha256'].items():
        assert digest(path) == sha, path
    first = load('artifacts/optical-composite-f64-atmosphere-first-independent-review.json')
    assert first['passed'] and first['selected_reuse']['historical_required_route_verdict'] == 'REJECT'
    assert first['atmosphere']['suffix_records'] == list(range(23))
    return {'closure': CLOSURE, 'sha256': digest(CLOSURE),
            'initial_atmosphere_pass': True, 'historical_selected_route_verdict': 'REJECT'}


def source_gate(case):
    proof = load(SOURCE)
    assert proof['schema'] == 'cartesian-composite-nine-packet-source-review-v1' and proof['passed']
    for path, sha in proof['sha256'].items():
        assert digest(path) == sha, path
    assert sorted(r['case'] for r in proof['cases']) == sorted(CASES)
    row = next(r for r in proof['cases'] if r['case'] == case)
    assert row['five_uploads_exact'] and row['validation_passed']
    assert sorted(m['name'] for m in row['modules']) == ['fallback', 'prefix', 'suffix']
    for module in row['modules']:
        assert digest(row['candidate_prefix']+'-'+module['name']+'.wgsl') == module['sha256']
    for suffix, sha in row['uploaded_sha256'].items():
        assert suffix in ('-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin')
        assert digest(row['candidate_prefix']+suffix) == sha
    assert set(row['uploaded_sha256']) == {'-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin'}
    return row


def baseline(case, source):
    path = 'artifacts/optical-cartesian-matrix-'+case+'-baseline-freeze-first.json'
    frozen = load(path)
    assert frozen['case'] == case and frozen['candidate_readback_used'] is False
    assert frozen['eligible_for_same_packet_candidate'] and frozen['baseline_source_and_runtime_checks_passed']
    assert digest(frozen['contract']) == frozen['contract_sha256']
    contract = load(frozen['contract'])
    assert contract['case'] == case and contract['candidate_readback_used'] is False
    assert source['original_baseline'] == contract['baseline']
    assert source['baseline_contract'] == frozen['contract']
    assert source['baseline_contract_sha256'] == frozen['contract_sha256']
    for p, sha in contract['input_sha256'].items():
        assert digest(p) == sha, p
    assert len(contract['records']) == 23
    refs = {r['record_index']: r for r in frozen['references']}
    assert len(refs) == frozen['reference_record_count']
    expected = [i for i, r in enumerate(contract['records']) if r['original_status'] == 0]
    assert sorted(refs) == (expected if case == 'fisheye' or case.startswith('atmosphere-') else [])
    for i, r in refs.items():
        assert r['launch_words'] == contract['records'][i]['launch_words']
        assert fixed.exact_errors(contract['records'][i]['original_terminal_words'], r) == r['original_error']
        diff = differences(r['coarse_reference'], r['tight_reference'])
        assert r['reference_convergence'] == dict(zip(fixed.METRICS, (diff['endpoint_m'], diff['path_m'], diff['direction'])))
        for m in fixed.METRICS:
            assert Q(r['per_metric_ceiling_exact'][m]) == Q(r['original_error'][m])+Q(r['reference_convergence'][m])
            if case.startswith('atmosphere-'):
                assert Q(r['original_error'][m]) < Q(fixed.LIMITS[m])
    if case.startswith('atmosphere-'):
        assert frozen['original_atmosphere_absolute_gate_passed']
    archive = historical.old_source_archive(contract['baseline'])
    return frozen, contract, refs, archive


def handoff_structure(words, primary):
    """Necessary saved-field checks; deliberately not a GPU arithmetic replay."""
    values = words.copy().view('<f4').astype(float)
    assert np.isfinite(values[:11]).all() and np.isfinite(values[16:20]).all()
    owner = int(words[20].view('<i4'))
    count = float(primary[9, 0])
    assert count.is_integer() and 2 <= count <= 128
    count = int(count)
    layers = primary[12:12+count]
    assert np.isfinite(layers).all() and np.all(np.diff(layers[:, 0]) > 0)
    assert primary[10, 1] == layers[0, 0] and primary[9, 1] == layers[-1, 0]
    assert -1 <= owner < count-1
    low = primary[10, 2] if owner == -1 else layers[owner, 0]
    high = primary[10, 1] if owner == -1 else layers[owner+1, 0]
    assert low < values[19] < high
    assert values[3] >= 0 and values[7] >= 0
    assert np.linalg.norm(values[4:7]) > 0 and np.linalg.norm(values[16:19]) > 0
    return {'saved_owner': owner, 'saved_height': values[19], 'strict_uploaded_interval': [float(low), float(high)],
            'scope': 'Finite strict interior and valid counters, not a reconstruction of packed geometry or the prefix arithmetic.'}


def evaluate(prefix, case, contract, refs):
    report = load(prefix+'.json')
    assert report['selected'] and not report['timed'] and report['iterations'] == []
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['auxiliary_layout'] is None and report['diagnostic_bytes_per_record'] == 32
    original = contract['baseline']
    assert report['steps'] == load(original+'.json')['steps']
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    states = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    primary_words = np.fromfile(ROOT/(prefix+'-primary.bin'), dtype='<u4')
    primary = primary_words.copy().view('<f4').reshape(140, 4).astype(float)
    original_raw = np.fromfile(ROOT/(original+'-readback.bin'), dtype='<u4')
    original_states = np.fromfile(ROOT/(original+'-states.bin'), dtype='<u4').reshape(23, 24)
    assert raw.nbytes == report['output_bytes'] == 1856 and states.nbytes == report['state_bytes'] == 2208
    assert progress.nbytes == report['progress_bytes'] == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    assert np.array_equal(raw[:4], primary_words[44:48]) and np.array_equal(raw[:4], original_raw[:4])
    terminals, details = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    assert np.isfinite(terminals.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(terminals.copy().view('<f4')[:, 8:11]).all()
    assert np.isfinite(states.copy().view('<f4')[:, :11]).all()
    assert np.all(states[:, 11] == 0) and np.all(states[:, 23] == 0)
    assert np.array_equal(states[:, 21], np.arange(23))
    assert np.array_equal(states[:, 8:11], details[:, 4:7]) and np.all(details[:, 7] == 0)
    assert np.array_equal(states[:, 12:15], details[:, :3])
    assert np.array_equal(states[:, 22] | (states[:, 15] << 8), details[:, 3])
    assert np.all(states[:, 12:15] <= 4096) and np.all(states[:, 12].astype(np.uint64)+states[:, 13] <= 4096)
    work = states[:, 12:15].astype(np.uint64).sum(axis=1)
    assert progress.tolist() == report['progress_words'] == [0, 23, int(work.max()), 0]
    control = None
    control_hashes = None
    if case in CONTROL:
        control_hashes = fixed.checked_hashes(load(CONTROL[case]+'-outputs.json'))
        control = np.fromfile(ROOT/(CONTROL[case]+'-states.bin'), dtype='<u4').reshape(23, 24)
    assert len(report['samples']) == 23
    rows = []
    for i, sample in enumerate(report['samples']):
        old = contract['records'][i]
        assert sample['record_index'] == i and sample['pixel'] == old['pixel_label']
        assert sample['terminal_words'] == terminals[i].tolist()
        assert sample['diagnostic_words'] == details[i].tolist() and sample['state_words'] == states[i].tolist()
        assert details[i, 4:7].tolist() == old['launch_words']
        route, status = int(states[i, 22]), int(terminals[i, 7])
        assert route in fixed.NAMES
        if route in (0, 3):
            assert states[i, 13] == states[i, 14] == states[i, 15] == 0
            noncount = [j for j in range(24) if j not in (12, 13, 14)]
            assert np.array_equal(states[i, noncount], original_states[i, noncount])
        if route in (1, 2):
            assert states[i, 12] > 0 and states[i, 13] > 0
        if route == 1: assert status == 0 and states[i, 14] == states[i, 15] == 0
        if route == 2: assert states[i, 14] > 0 and states[i, 15] in (2, 3, 4, 5)
        if route == 3: assert status == 3 and np.all(states[i, 12:15] == 0)
        if case == 'readiness': assert route == 0 and status == 2 and np.all(states[i, 12:15] == 0)
        structural = handoff_structure(states[i], primary) if route in (1, 2) else None
        identity = bool(np.array_equal(states[i, initial.PHYSICAL_COLUMNS], control[i, initial.PHYSICAL_COLUMNS])) if control is not None else None
        reasons = []
        if identity is False: reasons.append('retained-physical-handoff-or-prefix-differs')
        exact = terminals[i].tolist() == old['original_terminal_words']
        exact_required = route != 1 or case in ('readiness', 'vacuum-outward', 'vacuum-inward')
        if exact_required and not exact: reasons.append('required-original-terminal-differs')
        if route == 1 and old['original_status'] != 0: reasons.append('original-sky-class-differs')
        if route == 1 and terminals[i, 8:12].tolist() != old['original_terminal_words'][8:12]:
            reasons.append('original-sky-normal-or-owner-differs')
        error, same, absolute = None, None, None
        if i in refs:
            error = fixed.exact_errors(terminals[i], refs[i])
            if route == 1:
                same = {m: Q(error[m]) <= Q(refs[i]['per_metric_ceiling_exact'][m]) for m in fixed.METRICS}
                reasons.extend(m+'-same-ray-limit' for m, ok in same.items() if not ok)
            if case.startswith('atmosphere-'):
                absolute = {m: Q(error[m]) < Q(fixed.LIMITS[m]) for m in fixed.METRICS}
                reasons.extend(m+'-absolute-limit' for m, ok in absolute.items() if not ok)
        elif route == 1 and not exact_required: reasons.append('missing-frozen-sky-reference')
        rows.append({'record_index': i, 'pixel': old['pixel_label'], 'route': fixed.NAMES[route], 'status': status,
                     'reason': int(states[i, 15]), 'attempts': states[i, 12:15].tolist(),
                     'retained_handoff_comparison_available': control is not None,
                     'retained_physical_handoff_and_prefix_exact': identity, 'saved_handoff_structure': structural,
                     'original_terminal_exact': exact, 'exact_original_required': exact_required,
                     'candidate_error': error, 'accepted_suffix_metric_checks': same, 'absolute_checks': absolute,
                     'passed': not reasons, 'failure_reasons': reasons})
    if case == 'fisheye':
        assert any(r['route'] == 'masked' for r in rows) and any(r['route'] != 'masked' for r in rows)
    return {'case': case, 'passed': all(r['passed'] for r in rows),
            'failed_records': [r['record_index'] for r in rows if not r['passed']],
            'route_counts': dict(Counter(r['route'] for r in rows)), 'status_counts': dict(Counter(r['status'] for r in rows)),
            'suffix_records': [r['record_index'] for r in rows if r['route'] == 'accepted-suffix'],
            'fallback_records': [r['record_index'] for r in rows if r['route'] == 'original-fallback'],
            'retained_handoff_comparison_available': control is not None,
            'historical_control_output_hashes': control_hashes, 'records': rows}


def review(prefix, case):
    assert case in CASES
    first, source = initial_closure(), source_gate(case)
    frozen, contract, refs, archived = baseline(case, source)
    report = load(prefix+'.json')
    assert report['cartesian_composite_f64'] == case and not report['cartesian_composite_reference']
    assert report['cartesian_suffix_f64'] is None and not report['reference']
    assert not report['cartesian_suffix_trace'] and not report['cartesian_suffix_tight']
    assert set(re.findall(r'\b[A-Z][A-Z0-9_]*\b', report['required_features'])) == {'SHADER_F64', 'SHADER_INT64'}
    manifest, compiled, stability = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert manifest['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    assert manifest['files'][:-1] == compiled['inputs'] == stability['after_run']
    hashes = fixed.checked_hashes(manifest['files'])
    assert set(input_paths()) <= hashes.keys()
    outputs = fixed.checked_hashes(load(prefix+'-outputs.json'))
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None and not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-composite-f64', case]
    assert manifest['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    exe = manifest['files'][-1]
    assert exe['path'].replace('\\', '/') == prefix+'-executable.exe' and exe['sha256'] == life['executable_sha256']
    for suffix in ('-prefix.wgsl', '-suffix.wgsl', '-fallback.wgsl', '-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin'):
        assert (ROOT/(prefix+suffix)).read_bytes() == (ROOT/(source['candidate_prefix']+suffix)).read_bytes(), suffix
    result = evaluate(prefix, case, contract, refs)
    return {'schema': 'cartesian-composite-matrix-final-output-v1', 'candidate': prefix, 'case': case,
            'policy_sha256': digest(POLICY), 'evidence_scope_sha256': digest(EVIDENCE), 'initial_closure': first,
            'source_review': SOURCE, 'source_review_sha256': digest(SOURCE), 'source_case': source,
            'baseline_source_archive': archived, 'baseline_contract_sha256': frozen['contract_sha256'],
            'candidate_input_hashes': hashes, 'candidate_output_hashes': outputs, 'result': result,
            'passed': result['passed'], 'decision': 'packet-final-output-pass-only' if result['passed'] else 'stop-composite-without-tuning',
            'returning_coverage_or_full_timing_authorized': False,
            'limitations': ['No observed handoff bit identity is claimed where an earlier capture does not exist.',
                           'Exact original fallback is whole-output parity, not promoted-solver accuracy evidence.',
                           'No ODE solve, threshold change, full-chain timing or production adoption.'],
            'reviewer_source_sha256': digest('scripts/review_optical_cartesian_composite_matrix.py')}


def input_paths():
    paths = {POLICY, EVIDENCE, SOURCE, CLOSURE, 'scripts/review_optical_cartesian_composite_matrix.py',
             'artifacts/optical-composite-f64-atmosphere-first-independent-review.json'}
    paths.update(load(CLOSURE)['sha256'])
    proof = load(SOURCE)
    paths.update(proof['sha256'])
    for source in proof['cases']:
        case = source['case']
        path = 'artifacts/optical-cartesian-matrix-'+case+'-baseline-freeze-first.json'
        frozen = load(path)
        paths.update([path, frozen['contract']])
        paths.update(load(frozen['contract'])['input_sha256'])
        for name in ('prefix.wgsl', 'suffix.wgsl', 'fallback.wgsl', 'primary.bin', 'field.bin', 'geometry.bin', 'frames.bin', 'certificates.bin'):
            paths.add(source['candidate_prefix']+'-'+name)
        original = load(frozen['contract'])['baseline']
        paths.update([original+'.json', original+'-sources.json', original+'-readback.bin', original+'-states.bin'])
        if case in CONTROL:
            paths.add(CONTROL[case]+'-outputs.json')
            paths.update(r['path'] for r in load(CONTROL[case]+'-outputs.json'))
    archive_path = 'artifacts/optical-cartesian-matrix-reference-first-source-archive.json'
    archive = load(archive_path)
    paths.update([archive_path, archive['archive'], archive['baseline_manifest']])
    archived = {r['original_path'].replace('\\', '/') for r in archive['files']}
    paths.update(r['path'] for r in load(archive['baseline_manifest'])['files'][:-1] if r['path'].replace('\\', '/') not in archived)
    # Some archived original compiler inputs are external libraries. They
    # remain explicit absolute-path fingerprints, not workspace archives.
    normalized = set()
    for path in paths:
        p = Path(path)
        normalized.add(p.relative_to(ROOT).as_posix() if p.is_relative_to(ROOT) else p.as_posix())
    paths = normalized
    queue = [p for p in paths if p.startswith('scripts/') and p.endswith('.py')]
    visited = set()
    while queue:
        path = queue.pop()
        if path in visited: continue
        visited.add(path)
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
    parser.add_argument('--case', choices=CASES, required=True)
    parser.add_argument('--check-inputs', action='store_true')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    assert bool(args.prefix) != args.check_inputs
    if args.check_inputs:
        source = source_gate(args.case)
        frozen, contract, refs, _ = baseline(args.case, source)
        result = {'case': args.case, 'initial_closure': initial_closure(), 'frozen_references_checked': len(refs),
                  'retained_handoff_comparison_available': args.case in CONTROL, 'input_sha256': {p: digest(p) for p in input_paths()},
                  'passed': True, 'candidate_readback_used': False}
    else:
        result = review(args.prefix, args.case)
    historical.base.write_new(args.output, result)
    print({'passed': result['passed'], 'case': args.case})
