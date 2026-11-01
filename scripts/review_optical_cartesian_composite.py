"""Separate composite gate: retained selected outputs and first atmosphere case.

Historical required-route rejection is preserved. Only final route population
is descriptive here; unchanged physical handoffs, original fallback, all final
numerical ceilings, source identity and finite terminal checks remain gates.
No other matrix case, reference solve, timing or controller change is enabled.
"""
import argparse
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path
import re
import zipfile

import numpy as np
import review_optical_cartesian_suffix_f64 as fixed

ROOT = fixed.ROOT
load, digest = fixed.load, fixed.digest
POLICY = 'docs/science/cartesian-composite-f64-policy.md'
SOURCE = 'artifacts/optical-composite-f64-initial-source-review-first.json'
VALIDATION = 'artifacts/optical-composite-f64-atmosphere-validation-first'
SELECTED = 'artifacts/optical-cartesian-f64-selected-first'
ATMOSPHERE = 'atmosphere-100-gradient'
PHYSICAL_COLUMNS = list(range(13))+list(range(16, 22))+[23]


def source_gate():
    report = load(SOURCE)
    assert report['schema'] == 'cartesian-composite-initial-source-review-v1'
    assert report['passed'] and report['case'] == ATMOSPHERE
    for path, sha in report['sha256'].items():
        assert digest(path) == sha, path
    for row in report['assembled_module_and_upload_checks']:
        assert row['exact_bytes']
        assert digest(row['candidate']) == digest(row['reviewed']) == row['sha256']
    return {'source_review': SOURCE, 'source_review_sha256': digest(SOURCE),
            'unchanged_promoted_shader_sha256': digest(VALIDATION+'-suffix.wgsl')}


def selected_provenance():
    """Use archived sources, not mutable current host files, for the old run."""
    closure = load(SELECTED+'-closure-review.json')
    assert closure['review_completed'] and not closure['experiment_passed']
    assert closure['frozen_auditor_replay_exact']
    for path, sha in closure['sha256'].items():
        assert digest(path) == sha, path
    old_review = load(SELECTED+'-independent-review.json')
    assert old_review['passed'] is False and old_review['failed_records'] == [3]
    assert old_review['decision'] == 'reject-controller-without-retuning'
    fixed.checked_hashes(load(SELECTED+'-outputs.json'))
    archive = load(SELECTED+'-source-archive.json')
    assert digest(SELECTED+'-source.zip') == archive['archive_sha256']
    assert digest(SELECTED+'-sources.json') == archive['source_manifest_sha256']
    original = {r['path'].replace('\\', '/'): r['sha256'] for r in load(SELECTED+'-sources.json')['files']}
    seen, blobs = set(), {}
    with zipfile.ZipFile(ROOT/(SELECTED+'-source.zip')) as bundle:
        assert json.loads(bundle.read('paths.json')) == archive['entries']
        for row in archive['entries']:
            path = row['path'].replace('\\', '/')
            assert path not in seen and original[path] == row['sha256']
            seen.add(path)
            assert bundle.getinfo(row['archive_entry']).file_size == row['bytes']
            if row['archive_entry'] not in blobs:
                with bundle.open(row['archive_entry']) as stream:
                    assert hashlib.file_digest(stream, 'sha256').hexdigest() == row['sha256']
                blobs[row['archive_entry']] = row['sha256']
            assert blobs[row['archive_entry']] == row['sha256'], 'Aliased archive entry has inconsistent hashes'
    excluded = set()
    for row in archive['not_in_archive']:
        path = row['path'].replace('\\', '/')
        assert path not in seen and path not in excluded and original[path] == row['sha256']
        excluded.add(path)
    assert seen | excluded == original.keys(), 'Archive and explicit exclusions do not cover the original manifest'
    return {'historical_experiment_passed': False, 'historical_failed_records': [3],
            'closure_sha256': digest(SELECTED+'-closure-review.json'),
            'archive_sha256': archive['archive_sha256'], 'archived_paths_verified': len(seen),
            'unique_blobs_verified': len(blobs), 'unarchived_fingerprints': len(excluded), 'selected_rerun': False}


def evaluate_outputs(prefix, case):
    config, policy, baseline, contract, refs, old_raw, old_state = fixed.frozen_data(case)
    control_hashes = fixed.checked_hashes(load(config['control']+'-outputs.json'))
    report = load(prefix+'.json')
    assert report['selected'] and not report['timed'] and not report['iterations']
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['auxiliary_layout'] is None and report['diagnostic_bytes_per_record'] == 32
    assert report['steps'] == load(config['control']+'.json')['steps']
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    states = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    assert raw.nbytes == report['output_bytes'] == 1856 and states.nbytes == report['state_bytes'] == 2208
    assert progress.nbytes == report['progress_bytes'] == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    assert report['header_matches'] and np.array_equal(raw[:4], old_raw[:4])
    terminals, details = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    old_details = old_raw[280:].reshape(23, 8)
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
    identities = np.all(states[:, PHYSICAL_COLUMNS] == old_state[:, PHYSICAL_COLUMNS], axis=1)
    assert len(report['samples']) == 23
    rows = []
    for i, sample in enumerate(report['samples']):
        original = contract['records'][i]
        pixel = original.get('pixel', original.get('pixel_label'))
        assert sample['record_index'] == i and sample['pixel'] == pixel
        assert sample['terminal_words'] == terminals[i].tolist()
        assert sample['diagnostic_words'] == details[i].tolist() and sample['state_words'] == states[i].tolist()
        route, status = int(states[i, 22]), int(terminals[i, 7])
        assert route in fixed.NAMES
        if route in (0, 3): assert states[i, 13] == states[i, 14] == 0
        if route in (1, 2): assert states[i, 12] > 0 and states[i, 13] > 0
        if route == 1: assert status == 0 and states[i, 14] == states[i, 15] == 0
        if route == 2: assert states[i, 14] > 0 and states[i, 15] in (2, 3, 4, 5)
        if route == 3: assert status == 3
        reasons = []
        if not identities[i]: reasons.append('physical-handoff-or-prefix-differs')
        if details[i, 4:7].tolist() != original['launch_words']: reasons.append('launch-differs')
        exact = terminals[i].tolist() == original['original_terminal_words']
        if route != 1 and not exact: reasons.append('original-terminal-differs')
        if route == 1 and original['original_status'] != 0: reasons.append('original-sky-class-differs')
        if case == 'selected' and i not in config['required']:
            if not np.array_equal(states[i], old_state[i]) or not np.array_equal(details[i], old_details[i]):
                reasons.append('unhanded-original-state-or-details-differ')
        errors = fixed.exact_errors(terminals[i], refs[i]) if i in refs else None
        checks, absolute = None, None
        if case == 'selected':
            result = fixed.evaluate_selected_record(policy, i, fixed.NAMES[route], terminals[i].tolist(), details[i, 4:7].tolist(), errors)
            reasons.extend(result['reasons'])
            if route == 1:
                checks = {m: Q(errors[m]) <= Q(policy['records'][i]['per_metric_error_ceiling_exact'][m]) for m in fixed.METRICS}
        else:
            assert errors is not None
            if route == 1:
                checks = {m: Q(errors[m]) <= Q(refs[i]['per_metric_ceiling_exact'][m]) for m in fixed.METRICS}
                reasons.extend(m+'-worse-than-same-original' for m, ok in checks.items() if not ok)
            absolute = {m: Q(errors[m]) < Q(fixed.LIMITS[m]) for m in fixed.METRICS}
            reasons.extend(m+'-absolute-limit' for m, ok in absolute.items() if not ok)
        rows.append({'record_index': i, 'pixel': pixel, 'route': fixed.NAMES[route], 'status': status,
                     'reason': int(states[i, 15]), 'attempts': states[i, 12:15].tolist(),
                     'physical_handoff_and_prefix_exact': bool(identities[i]), 'original_terminal_exact': exact,
                     'candidate_error': errors, 'accepted_suffix_metric_checks': checks, 'absolute_checks': absolute,
                     'passed': not reasons, 'failure_reasons': reasons})
    return {'case': case, 'passed': all(r['passed'] for r in rows),
            'historical_control_output_hashes': control_hashes,
            'failed_records': [r['record_index'] for r in rows if not r['passed']],
            'suffix_records': np.flatnonzero(states[:, 22] == 1).tolist(),
            'fallback_records': np.flatnonzero(states[:, 22] == 2).tolist(),
            'route_counts': dict(Counter(r['route'] for r in rows)),
            'physical_handoffs_and_prefixes_exact': bool(np.all(identities)), 'records': rows}


def selected_reuse():
    sources = source_gate()
    provenance = selected_provenance()
    # Recheck the unchanged selected modules/uploads using the original source
    # artifact; this does not assert mutable historical host paths are current.
    fixed.source_gate(SELECTED, 'selected')
    result = evaluate_outputs(SELECTED, 'selected')
    result['positive_suffix_demonstrated'] = bool(result['suffix_records'])
    result['passed'] = result['passed'] and result['positive_suffix_demonstrated']
    return {'schema': 'cartesian-composite-selected-reuse-v1', 'policy_sha256': digest(POLICY),
            'source_gate': sources, 'provenance': provenance, 'selected': result,
            'passed': result['passed'], 'historical_required_route_verdict': 'REJECT',
            'scope': 'Separately authorized final-output assessment of retained data. No rerun or rewritten historical verdict; not complete matrix or performance acceptance.'}


def review(prefix):
    source = source_gate()
    reuse = selected_reuse()
    assert reuse['passed'], 'Starting selected composite final-output prerequisites failed'
    report = load(prefix+'.json')
    assert report['cartesian_composite_f64'] == ATMOSPHERE and not report['cartesian_composite_reference']
    assert report['cartesian_suffix_f64'] is None and not report['reference']
    assert not report['cartesian_suffix_trace'] and not report['cartesian_suffix_tight']
    assert set(re.findall(r'\b[A-Z][A-Z0-9_]*\b', report['required_features'])) == {'SHADER_F64', 'SHADER_INT64'}
    manifest, compiled, stability = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert manifest['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    assert manifest['files'][:-1] == compiled['inputs'] == stability['after_run']
    input_hashes = fixed.checked_hashes(manifest['files'])
    assert set(input_paths()) <= input_hashes.keys()
    output_hashes = fixed.checked_hashes(load(prefix+'-outputs.json'))
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and not life['timed_out'] and not life['terminated_by_wrapper'] and life['failure'] is None
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-composite-f64', ATMOSPHERE]
    assert manifest['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    exe = manifest['files'][-1]
    assert exe['path'].replace('\\', '/') == prefix+'-executable.exe' and exe['sha256'] == life['executable_sha256']
    for suffix in ('-prefix.wgsl', '-suffix.wgsl', '-fallback.wgsl', '-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin'):
        assert (ROOT/(prefix+suffix)).read_bytes() == (ROOT/(VALIDATION+suffix)).read_bytes(), suffix
    result = evaluate_outputs(prefix, ATMOSPHERE)
    return {'schema': 'cartesian-composite-first-atmosphere-v1', 'candidate': prefix,
            'policy_sha256': digest(POLICY), 'source_gate': source,
            'candidate_input_hashes': input_hashes, 'candidate_output_hashes': output_hashes,
            'selected_reuse': reuse, 'atmosphere': result, 'passed': result['passed'],
            'decision': 'first-atmosphere-final-output-gate-pass-only' if result['passed'] else 'stop-composite-without-tuning',
            'full_frame_timing_or_production_authorized': False,
            'limitations': ['No prescribed accelerated record set in this separate policy; all original final numerical gates remain.',
                            'A fallback is exact-original evidence, not f64 accuracy evidence.',
                            'Remaining matrix and prospectively frozen returning/fallback control must close before any full-chain timing.'],
            'reviewer_source_sha256': digest('scripts/review_optical_cartesian_composite.py')}


def input_paths():
    paths = set(fixed.input_paths()) | {POLICY, SOURCE, 'scripts/review_optical_cartesian_composite.py',
        'docs/science/cartesian-f64-coverage-assessment.md',
        SELECTED+'-source.zip', SELECTED+'-source-archive.json', SELECTED+'-closure-review.json',
        SELECTED+'-publication-assessment.json', SELECTED+'-independent-review.json',
        SELECTED+'-sources.json', SELECTED+'-outputs.json', SELECTED+'.json',
        SELECTED+'-readback.bin', SELECTED+'-states.bin', SELECTED+'-progress.bin'}
    paths.update(p.replace('\\', '/') for p in load(SOURCE)['sha256'])
    for row in load(SOURCE)['assembled_module_and_upload_checks']:
        paths.update([row['candidate'].replace('\\', '/'), row['reviewed'].replace('\\', '/')])
    for path in load(SELECTED+'-closure-review.json')['sha256']:
        p = Path(path)
        paths.add((p.relative_to(ROOT).as_posix() if p.is_absolute() else str(p).replace('\\', '/')))
    paths.update(row['path'].replace('\\', '/') for row in load(SELECTED+'-outputs.json'))
    assert all((ROOT/path).is_file() for path in paths)
    return sorted(paths)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='?')
    parser.add_argument('--selected-reuse', action='store_true')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    assert bool(args.prefix) != args.selected_reuse
    result = selected_reuse() if args.selected_reuse else review(args.prefix)
    with (ROOT/args.output).open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps({'passed': result['passed'], 'scope': result['schema']}))
