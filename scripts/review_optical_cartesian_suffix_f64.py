"""Fixed two-case, unlogged f64 controller acceptance; never computes new gates.

Uses the previously frozen same-ray references and exact saved handoffs. The
old f32 accepted-step logging predicates do not describe an f64 controller and
are deliberately outside this ABI. All required routes, terminal metrics,
prefix identity and original fallback parity remain mandatory. No timing.
"""
import argparse
import ast
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path
import re

import numpy as np

from review_optical_cartesian_suffix_policy import evaluate_selected_record, METRICS
from verify_optical_dop853 import geometry

ROOT = Path(__file__).resolve().parents[1]
POLICY = 'artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json'
ATMOSPHERE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-baseline-freeze-first.json'
SOURCE_REVIEW = 'artifacts/optical-cartesian-f64-source-review-first.json'
CASES = {
    'selected': {
        'control': 'artifacts/optical-cartesian-suffix-tight-selected-first',
        'validation': 'artifacts/optical-cartesian-f64-selected-validation-first',
        'required': [3, 9, 10, 11, 12, 14, 15, 17, 18, 20],
    },
    'atmosphere-100-gradient': {
        'control': 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first',
        'validation': 'artifacts/optical-cartesian-f64-atmosphere-validation-first',
        'required': list(range(23)),
    },
}
LIMITS = {'position_m': 5., 'path_m': 5., 'direction': 3e-6}
NAMES = {0: 'unhanded-original', 1: 'accepted-suffix', 2: 'original-fallback', 3: 'masked'}
IDENTITY_COLUMNS = list(range(13)) + list(range(16, 24))


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def digest(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def checked_hashes(rows):
    result = {}
    for row in rows:
        path = row['path'].replace('\\', '/')
        assert digest(path) == row['sha256'], path
        assert path not in result or result[path] == row['sha256']
        result[path] = row['sha256']
    return result


def input_paths():
    """Exact retained-read and local-import closure for the pre-run inventory."""
    paths = {POLICY, ATMOSPHERE, SOURCE_REVIEW,
             'scripts/review_optical_cartesian_suffix_f64.py',
             'scripts/review_optical_cartesian_f64_source.py'}
    for path in load(SOURCE_REVIEW)['sha256']:
        paths.add(path.replace('\\', '/'))
    policy = load(POLICY)
    paths.update([policy['baseline_report'], policy['initial_contract'], policy['policy_source']])
    atmosphere = load(ATMOSPHERE)
    paths.add(atmosphere['contract'])
    for contract in (load(policy['initial_contract']), load(atmosphere['contract'])):
        paths.update(path.replace('\\', '/') for path in contract['input_sha256'])
    for config in CASES.values():
        control, validation = config['control'], config['validation']
        paths.update([control+'.json', control+'-outputs.json', control+'-states.bin', control+'-readback.bin',
                      validation+'-validation.json'])
        paths.update(row['path'].replace('\\', '/') for row in load(control+'-outputs.json'))
        for suffix in ('-prefix.wgsl', '-suffix.wgsl', '-fallback.wgsl',
                       '-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin'):
            paths.add(validation+suffix)
            paths.add(control+suffix)
    queue = [p for p in paths if p.startswith('scripts/') and p.endswith('.py')]
    visited = set()
    while queue:
        path = queue.pop()
        if path in visited: continue
        visited.add(path)
        for node in ast.walk(ast.parse((ROOT/path).read_text(encoding='utf-8-sig'))):
            modules = [a.name for a in node.names] if isinstance(node, ast.Import) else [node.module] if isinstance(node, ast.ImportFrom) else []
            for module in modules:
                if module:
                    local = 'scripts/'+module.split('.')[0]+'.py'
                    if (ROOT/local).is_file():
                        paths.add(local)
                        queue.append(local)
    assert all((ROOT/path).is_file() for path in paths)
    return sorted(paths)


def exact_errors(words, reference):
    """Same frozen local-output -> canonical metric calculation as old audits."""
    origin = np.asarray(reference['inputs']['origin_ecef_m'], dtype=float)
    _, up = geometry(origin)
    east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
    basis = np.column_stack((east, up, -np.cross(up, east)))
    value = np.asarray(words, dtype='<u4').view('<f4').astype(float)
    assert np.isfinite(value[:7]).all()
    truth = reference['tight_reference']
    return {'position_m': float(np.linalg.norm(origin+basis@value[:3]-truth['endpoint_ecef_m'])),
            'path_m': abs(float(value[3])-truth['geometric_path_m']),
            'direction': float(np.linalg.norm(basis@value[4:7]-truth['exit_direction']))}


def frozen_data(case):
    config = CASES[case]
    if case == 'selected':
        policy = load(POLICY)
        baseline = load(policy['baseline_report'])
        contract = load(policy['initial_contract'])
        assert digest(policy['baseline_report']) == policy['baseline_sha256']
        assert digest(policy['initial_contract']) == policy['initial_contract_sha256']
        assert digest(policy['policy_source']) == policy['policy_source_sha256']
        refs = {r['record_index']: r for r in baseline['cases']}
        assert sorted(refs) == config['required']
    else:
        policy = None
        baseline = load(ATMOSPHERE)
        assert baseline['case'] == case and baseline['candidate_readback_used'] is False
        assert baseline['eligible_for_same_packet_candidate'] and baseline['original_atmosphere_absolute_gate_passed']
        contract = load(baseline['contract'])
        assert digest(baseline['contract']) == baseline['contract_sha256']
        refs = {r['record_index']: r for r in baseline['references']}
        assert sorted(refs) == list(range(23))
    for path, sha in contract['input_sha256'].items():
        assert digest(path) == sha, path
    assert len(contract['records']) == 23
    # These recomputations are on ORIGINAL terminal words, before candidate
    # data. No reference solve, ceiling fitting or candidate-dependent policy.
    for index, ref in refs.items():
        assert ref['launch_words'] == contract['records'][index]['launch_words']
        original = contract['records'][index]['original_terminal_words']
        assert exact_errors(original, ref) == ref['original_error']
        if case != 'selected':
            for metric in METRICS:
                ceiling = Q(ref['original_error'][metric])+Q(ref['reference_convergence'][metric])
                assert ceiling == Q(ref['per_metric_ceiling_exact'][metric])
                assert Q(ref['original_error'][metric]) < Q(LIMITS[metric])
    old_raw = np.fromfile(ROOT/(config['control']+'-readback.bin'), dtype='<u4')
    old_state = np.fromfile(ROOT/(config['control']+'-states.bin'), dtype='<u4').reshape(23, 24)
    assert old_raw.size == 464 and np.flatnonzero(old_state[:, 22] == 1).tolist() == config['required']
    return config, policy, baseline, contract, refs, old_raw, old_state


def source_gate(prefix, case):
    config = CASES[case]
    validation = config['validation']
    checked = load(validation+'-validation.json')
    assert checked['all_modules_valid'] and checked['cartesian_suffix_f64'] == case
    # The independent proof artifact is part of the frozen run inventory. Its
    # source/artifact hashes are checked by its author; the entire runtime
    # module must additionally be exactly the reviewed offline module here.
    source_review = load(SOURCE_REVIEW)
    assert source_review['schema'] == 'cartesian-f64-suffix-independent-source-review-v1'
    assert source_review['passed'] is True
    for path, sha in source_review['sha256'].items():
        assert digest(path) == sha, path
    modules = []
    for name in ('prefix', 'suffix', 'fallback'):
        path, old = prefix+'-'+name+'.wgsl', validation+'-'+name+'.wgsl'
        assert (ROOT/path).read_bytes() == (ROOT/old).read_bytes(), name
        modules.append({'name': name, 'sha256': digest(path), 'offline_sha256': digest(old)})
    for name in ('primary', 'field', 'geometry', 'frames', 'certificates'):
        value = (ROOT/(prefix+'-'+name+'.bin')).read_bytes()
        assert value == (ROOT/(validation+'-'+name+'.bin')).read_bytes(), name
        assert value == (ROOT/(config['control']+'-'+name+'.bin')).read_bytes(), name
    return {'modules': modules, 'five_uploads_exact': True,
            'source_review': SOURCE_REVIEW, 'source_review_sha256': digest(SOURCE_REVIEW)}


def review(prefix, case):
    config, policy, baseline, contract, refs, old_raw, old_state = frozen_data(case)
    report = load(prefix+'.json')
    assert report['cartesian_suffix_f64'] == case and report['selected'] and report['record_count'] == 23
    assert not report['timed'] and not report['iterations'] and not report['reference']
    assert not report['cartesian_suffix_trace'] and not report['cartesian_suffix_tight']
    assert report['auxiliary_layout'] is None
    assert report['state_words_per_record'] == 24 and report['diagnostic_bytes_per_record'] == 32
    assert report['steps'] == load(config['control']+'.json')['steps']
    assert isinstance(report['required_features'], str)
    assert set(re.findall(r'\b[A-Z][A-Z0-9_]*\b', report['required_features'])) == {'SHADER_F64', 'SHADER_INT64'}
    source, compiled, stable = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert source['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert source['files'][:-1] == compiled['inputs'] == stable['after_run']
    hashes = checked_hashes(source['files'])
    assert set(input_paths()) <= hashes.keys(), 'Retained read/import is absent from the frozen run inventory'
    output_hashes = checked_hashes(load(prefix+'-outputs.json'))
    # The selected historical control's original full-path result can fail its
    # own accuracy gate; it supplies only frozen inputs/handoffs here.
    control_hashes = checked_hashes(load(config['control']+'-outputs.json'))
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['failure'] is None
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-suffix-f64', case]
    assert source['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    exe = source['files'][-1]
    assert exe['path'].replace('\\', '/') == prefix+'-executable.exe'
    assert exe['sha256'] == life['executable_sha256']
    assert SOURCE_REVIEW in hashes
    sources = source_gate(prefix, case)
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    assert raw.nbytes == report['output_bytes'] == 1856 and state.nbytes == report['state_bytes'] == 2208
    assert progress.nbytes == report['progress_bytes'] == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    assert np.array_equal(raw[:4], old_raw[:4]), 'Header token/dimensions differ from the exact-input control'
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    old_detail = old_raw[280:].reshape(23, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(terminal.copy().view('<f4')[:, 8:11]).all()
    assert np.isfinite(state.copy().view('<f4')[:, :11]).all()
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.array_equal(state[:, 21], np.arange(23))
    assert np.all(state[:, 12:15] <= 4096) and np.all(state[:, 12].astype(np.uint64)+state[:, 13] <= 4096)
    work = state[:, 12:15].astype(np.uint64).sum(axis=1)
    assert progress.tolist() == report['progress_words'] == [0, 23, int(work.max()), 0]
    identities = np.all(state[:, IDENTITY_COLUMNS] == old_state[:, IDENTITY_COLUMNS], axis=1)
    assert len(report['samples']) == 23
    rows = []
    for i, sample in enumerate(report['samples']):
        original = contract['records'][i]
        expected_pixel = original.get('pixel', original.get('pixel_label'))
        assert sample['record_index'] == i and sample['pixel'] == expected_pixel
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist() and sample['state_words'] == state[i].tolist()
        route, status = int(state[i, 22]), int(terminal[i, 7])
        assert route in NAMES
        if route in (1, 2):
            assert state[i, 12] > 0 and state[i, 13] > 0
        if route == 1:
            assert status == 0 and state[i, 14] == state[i, 15] == 0
        if route == 2:
            assert state[i, 14] > 0 and state[i, 15] in (2, 3, 4, 5)
        reasons = []
        if not identities[i]: reasons.append('handoff-or-prefix-identity-differs')
        if route != old_state[i, 22]: reasons.append('required-route-differs')
        if detail[i, 4:7].tolist() != original['launch_words']: reasons.append('launch-differs')
        same_original = terminal[i].tolist() == original['original_terminal_words']
        if route != 1 and not same_original: reasons.append('original-terminal-differs')
        if i not in config['required']:
            if not np.array_equal(state[i], old_state[i]) or not np.array_equal(detail[i], old_detail[i]):
                reasons.append('original-route-state-or-details-differ')
        errors, metric_checks, absolute = None, None, None
        if i in refs:
            errors = exact_errors(terminal[i], refs[i])
        if case == 'selected':
            evaluation = evaluate_selected_record(policy, i, NAMES[route], terminal[i].tolist(), detail[i, 4:7].tolist(), errors)
            reasons.extend(evaluation['reasons'])
            if route == 1 and errors is not None:
                metric_checks = {m: Q(errors[m]) <= Q(policy['records'][i]['per_metric_error_ceiling_exact'][m]) for m in METRICS}
        else:
            assert errors is not None
            if route == 1:
                if original['original_status'] != 0: reasons.append('original-class-differs')
                metric_checks = {m: Q(errors[m]) <= Q(refs[i]['per_metric_ceiling_exact'][m]) for m in METRICS}
                reasons.extend(m+'-worse-than-same-original' for m, ok in metric_checks.items() if not ok)
            absolute = {m: Q(errors[m]) < Q(LIMITS[m]) for m in METRICS}
            reasons.extend(m+'-absolute-limit' for m, ok in absolute.items() if not ok)
        rows.append({'record_index': i, 'pixel': expected_pixel, 'route': NAMES[route], 'status': status,
                     'attempts': state[i, 12:15].tolist(), 'same_handoff_and_prefix': bool(identities[i]),
                     'terminal_words': terminal[i].tolist(), 'exact_original_terminal': same_original,
                     'candidate_error': errors, 'per_ray_checks': metric_checks, 'absolute_checks': absolute,
                     'passed': not reasons, 'failure_reasons': reasons})
    suffixes = np.flatnonzero(state[:, 22] == 1).tolist()
    failed = [r['record_index'] for r in rows if not r['passed']]
    passed = not failed and suffixes == config['required']
    return {'schema': 'cartesian-f64-suffix-two-case-review-v1', 'candidate': prefix, 'case': case,
            'passed': passed, 'decision': 'one-case-pass-only' if passed else 'reject-controller-without-retuning',
            'source_gate': sources, 'candidate_input_hashes': hashes, 'candidate_output_hashes': output_hashes,
            'historical_control_output_hashes': control_hashes,
            'frozen_policy': POLICY if case == 'selected' else ATMOSPHERE,
            'frozen_policy_sha256': digest(POLICY if case == 'selected' else ATMOSPHERE),
            'required_suffix_records': config['required'], 'observed_suffix_records': suffixes,
            'route_counts': dict(Counter(r['route'] for r in rows)), 'failed_records': failed,
            'all_handoffs_and_prefixes_exact': bool(np.all(identities)), 'records': rows,
            'full_frame_timing_or_production_authorized': False,
            'limitations': ['Unlogged f64 controller: no assertion of f32 accepted-step logs or internal step identity.',
                            'Fixed finite output/route gates, not a global ODE-error enclosure or image-wide proof.',
                            'Both initial packets, remaining matrix/turning and complete-chain speed gates remain separate.'],
            'reviewer_source_sha256': digest('scripts/review_optical_cartesian_suffix_f64.py')}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='?')
    parser.add_argument('--case', required=True, choices=CASES)
    parser.add_argument('--output')
    parser.add_argument('--check-inputs', action='store_true')
    args = parser.parse_args()
    if args.check_inputs:
        data = frozen_data(args.case)
        print(json.dumps({'case': args.case, 'frozen_references': len(data[4]), 'required_suffix_records': data[0]['required']}))
    else:
        assert args.prefix and args.output
        result = review(args.prefix, args.case)
        with (ROOT/args.output).open('x', encoding='utf-8') as stream:
            stream.write(json.dumps(result, indent=2, allow_nan=False)+'\n')
        print(json.dumps({k: result[k] for k in ('case', 'passed', 'decision', 'observed_suffix_records', 'failed_records')}))
