"""Compare fixed-control branch candidates with their prospective freezes.

No references or thresholds are fitted to candidate results. Entire uploads
and declared source differences are prerequisites. All records are retained,
including repeated launches, exact-original routes and failed metric gates.
"""
from collections import Counter
from fractions import Fraction as Q
import argparse
import hashlib
import json
import math
import zipfile

import numpy as np

import review_optical_cartesian_matrix_baseline as base
from review_optical_cartesian_suffix_trace import number, positive_addition_enclosure
from verify_optical_dop853 import geometry


def old_source_archive(original):
    manifest_path = 'artifacts/optical-cartesian-matrix-reference-first-source-archive.json'
    archive = base.load(manifest_path)
    assert base.digest(archive['archive']) == archive['archive_sha256']
    assert base.digest(archive['baseline_manifest']) == archive['baseline_manifest_sha256']
    old = base.load(original+'-sources.json')['files'][:-1]
    anchor = base.load(archive['baseline_manifest'])['files'][:-1]
    assert old == anchor, 'The ten baseline compiler inputs must share the archived manifest'
    expected = {r['path'].replace('\\', '/'): r['sha256'] for r in old}
    archived = {}
    with zipfile.ZipFile(base.ROOT/archive['archive']) as bundle:
        for row in archive['files']:
            path = row['original_path'].replace('\\', '/')
            assert path not in archived and expected[path] == row['sha256']
            assert hashlib.sha256(bundle.read(row['archive_entry'])).hexdigest() == row['sha256']
            archived[path] = row['sha256']
    remaining = []
    for row in old:
        if row['path'].replace('\\', '/') not in archived:
            assert base.digest(row['path']) == row['sha256'], row['path']
            remaining.append(row['path'])
    return {'manifest': manifest_path, 'manifest_sha256': base.digest(manifest_path),
            'archive_sha256': archive['archive_sha256'], 'verified_archived_files': len(archived),
            'unarchived_dependencies_verified_at_original_hash': remaining,
            'historical_baseline_executables_archived': False}


def audit_logs(words, state, terminal):
    count, overflow, identity, token = map(int, words[0])
    assert 0 <= count <= 4096 and overflow == 0
    route, attempts = int(state[22]), int(state[13])
    if route in (0, 3):
        assert count == 0
    active = words[1:count+1]
    total = Q(number(state[3]))
    previous_attempt = 0
    for i, row in enumerate(active):
        step_word, before_word, attempt, kind = map(int, row)
        step, before = number(step_word), number(before_word)
        assert math.isfinite(step) and math.isfinite(before) and step >= 0 and before >= 0
        assert previous_attempt < attempt <= attempts
        previous_attempt = attempt
        assert kind == 0 or (kind == 1 and i == count-1)
        if i == 0:
            assert before_word == state[3]
        total += Q(step)
        after_word = int(active[i+1, 1]) if i+1 < count else int(terminal[3]) if route == 1 else None
        if after_word is not None:
            low, high = positive_addition_enclosure(before, step)
            assert low <= Q(number(after_word)) <= high
    if route == 1 and count == 0:
        assert state[3] == terminal[3]
    if route == 2 and count:
        assert int(active[-1, 3]) == 0, 'A logged successful top return cannot also request fallback'
    return {'accepted_steps': count, 'final_kind': int(active[-1, 3]) if count else None,
            'handoff_plus_steps_exact': str(total),
            'terminal_minus_sum_m': float(Q(number(terminal[3]))-total) if route == 1 else None,
            'scope': 'Interior stored additions checked. A fallback terminal belongs to the restarted original trace; it is not a suffix accumulation endpoint.'}


def review(prefix, frozen_prefix):
    report = base.load(prefix+'.json')
    frozen = base.load(frozen_prefix+'.json')
    contract = base.load(frozen['contract'])
    assert frozen['contract_sha256'] == base.digest(frozen['contract'])
    assert frozen['candidate_readback_used'] is False and frozen['eligible_for_same_packet_candidate']
    for path, expected in contract['input_sha256'].items():
        assert base.digest(path) == expected, path
    case, original = contract['case'], contract['baseline']
    assert report['cartesian_suffix_branch'] == case and not report['branch_reference']
    assert report['selected'] and not report['timed'] and not report['iterations']
    assert report['cartesian_suffix_tight'] and report['cartesian_suffix_trace'] and report['record_count'] == 23
    hashes = base.provenance(prefix)
    archived_source = old_source_archive(original)
    source, life = base.load(prefix+'-sources.json'), base.load(prefix+'-lifecycle.json')
    executable = source['files'][-1]
    assert source['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    assert executable['path'].replace('\\', '/') == prefix+'-executable.exe'
    assert base.digest(executable['path']) == executable['sha256'] == life['executable_sha256']
    original_report = base.load(original+'.json')
    assert report['steps'] == original_report['steps'] and report['auxiliary_layout'] == original_report['auxiliary_layout']
    modules = []
    for name in ('prefix', 'suffix', 'fallback'):
        new = base.text(prefix+'-'+name+'.wgsl')
        old = base.text(original+'-'+name+'.wgsl')
        restored = base.replace_once(new, base.HANDOFF, '') if name == 'prefix' else new
        assert restored == old, (case, name)
        modules.append({'name': name, 'candidate_sha256': base.digest(prefix+'-'+name+'.wgsl'),
                        'baseline_sha256': base.digest(original+'-'+name+'.wgsl'), 'declared_handoff_difference_only': True})
    for name in ('primary', 'field', 'geometry', 'frames', 'certificates'):
        assert (base.ROOT/(prefix+'-'+name+'.bin')).read_bytes() == (base.ROOT/(original+'-'+name+'.bin')).read_bytes()
    raw = np.fromfile(base.ROOT/(prefix+'-readback.bin'), dtype='<u4')
    states = np.fromfile(base.ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    logs = np.fromfile(base.ROOT/(prefix+'-auxiliary.bin'), dtype='<u4').reshape(23, 4097, 4)
    progress = np.fromfile(base.ROOT/(prefix+'-progress.bin'), dtype='<u4')
    old_states = np.fromfile(base.ROOT/(original+'-states.bin'), dtype='<u4').reshape(23, 24)
    assert raw.nbytes == report['output_bytes'] == 1856 and states.nbytes == report['state_bytes'] == 2208
    assert logs.nbytes == report['auxiliary_layout']['bytes'] == 1507696
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(states.copy().view('<f4')[:, :11]).all()
    assert np.all(states[:, 11] == 0) and np.all(states[:, 23] == 0)
    assert np.array_equal(states[:, 21], np.arange(23))
    assert np.array_equal(states[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.array_equal(states[:, 12:15], detail[:, :3])
    assert np.array_equal(states[:, 22] | (states[:, 15] << 8), detail[:, 3])
    assert np.all(states[:, 12:15] <= 4096) and np.all(states[:, 12].astype(np.uint64)+states[:, 13] <= 4096)
    work = states[:, 12:15].astype(np.uint64).sum(axis=1)
    assert progress.tolist() == report['progress_words'] == [0, 23, int(work.max()), 0]
    assert np.array_equal(logs[:, 0, 2], np.arange(23)) and np.all(logs[:, 0, 3] == raw[2])
    assert len(report['samples']) == 23
    references = {r['record_index']: r for r in frozen['references']}
    rows = []
    for i, sample in enumerate(report['samples']):
        original_row = contract['records'][i]
        assert sample['record_index'] == i and sample['pixel'] == original_row['pixel_label']
        assert sample['terminal_words'] == terminal[i].tolist() and sample['diagnostic_words'] == detail[i].tolist()
        assert sample['state_words'] == states[i].tolist()
        assert original_row['launch_words'] == detail[i, 4:7].tolist()
        route, status = int(states[i, 22]), int(terminal[i, 7])
        assert route in (0, 1, 2, 3)
        if route in (0, 3):
            # The prospective policy pins terminal words and valid counters;
            # it does not invent adaptive-count bit parity as an extra gate.
            noncount = [j for j in range(24) if j not in (12, 13, 14)]
            assert np.array_equal(states[i, noncount], old_states[i, noncount])
            assert states[i, 13] == states[i, 14] == 0
        if route == 3 or case == 'readiness':
            assert np.all(states[i, 12:15] == 0)
        if route in (1, 2):
            assert states[i, 12] > 0 and states[i, 13] > 0
        if route == 1:
            assert status == 0 and states[i, 14] == 0 and states[i, 15] == 0
        if route == 2:
            assert states[i, 14] > 0 and states[i, 15] in (2, 3, 4, 5)
        if route == 3:
            assert status == 3
        exact = terminal[i].tolist() == original_row['original_terminal_words']
        failures = []
        exact_required = route in (0, 2, 3) or case in ('readiness', 'vacuum-outward', 'vacuum-inward')
        if exact_required and not exact:
            failures.append('required-original-terminal-parity')
        if route == 1 and original_row['original_status'] != 0:
            failures.append('suffix-original-class-differs')
        errors, checks, absolute = None, None, None
        if i in references:
            reference = references[i]
            origin = np.asarray(reference['inputs']['origin_ecef_m'], dtype=float)
            _, up = geometry(origin)
            east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
            basis = np.column_stack((east, up, -np.cross(up, east)))
            f = terminal[i].copy().view('<f4').astype(float)
            truth = reference['tight_reference']
            errors = {'position_m': float(np.linalg.norm(origin+basis@f[:3]-truth['endpoint_ecef_m'])),
                      'path_m': abs(float(f[3])-truth['geometric_path_m']),
                      'direction': float(np.linalg.norm(basis@f[4:7]-truth['exit_direction']))}
            if route == 1:
                checks = {m: Q(errors[m]) <= Q(reference['per_metric_ceiling_exact'][m]) for m in base.METRICS}
                failures += [m+'-worse-than-same-original' for m, passed in checks.items() if not passed]
            if case.startswith('atmosphere-'):
                absolute = {m: Q(errors[m]) < Q(base.LIMITS[m]) for m in base.METRICS}
                failures += [m+'-original-absolute-limit' for m, passed in absolute.items() if not passed]
        elif route == 1 and not exact_required:
            failures.append('missing-frozen-sky-reference')
        logs_result = audit_logs(logs[i], states[i], terminal[i])
        rows.append({'record_index': i, 'pixel_label': sample['pixel'], 'route': route, 'status': status,
                     'reason': int(states[i, 15]), 'counts': states[i, 12:15].tolist(),
                     'counts_equal_original': bool(np.array_equal(states[i, 12:15], old_states[i, 12:15])),
                     'exact_original_terminal': exact, 'exact_original_required': exact_required,
                     'candidate_error': errors, 'per_ray_checks': checks, 'absolute_checks': absolute,
                     'accepted_log': logs_result, 'passed': not failures, 'failures': failures})
    assert all(base.digest(p) == expected for p, expected in hashes.items())
    return {'schema': 'cartesian-suffix-matrix-candidate-review-v1', 'case': case,
            'candidate': prefix, 'frozen_baseline_review': frozen_prefix+'.json', 'baseline_review_sha256': base.digest(frozen_prefix+'.json'),
            'candidate_provenance': hashes, 'source_checks': modules, 'baseline_source_archive': archived_source,
            'candidate_retained_executable': executable, 'five_uploads_exact': True,
            'route_counts': dict(Counter(r['route'] for r in rows)), 'status_counts': dict(Counter(r['status'] for r in rows)),
            'passed': all(r['passed'] for r in rows), 'failed_records': [r['record_index'] for r in rows if not r['passed']],
            'actual_fallback_records': [r['record_index'] for r in rows if r['route'] == 2], 'records': rows,
            'scope': 'Fixed prospective finite matrix gate. A passing packet does not close other packets, turning coverage, full-frame performance or production adoption. Historical executable provenance remains as qualified in the baseline.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--baseline-freeze', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    result = review(args.prefix, args.baseline_freeze)
    base.write_new(args.output, result)
    print(json.dumps({k: v for k, v in result.items() if k != 'records'}, indent=2))
