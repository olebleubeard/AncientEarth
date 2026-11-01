"""Independent closure of the first composite atmosphere GPU output.

No new GPU dispatch, reference integration, gate fitting or timing analysis.
Replays the frozen auditor, then separately decodes the raw state and metrics.
"""
from collections import Counter
from fractions import Fraction as Q
import hashlib
import json
from pathlib import Path

import numpy as np

import review_optical_cartesian_composite as frozen
from verify_optical_dop853 import geometry

ROOT = Path(__file__).resolve().parents[1]
PREFIX = 'artifacts/optical-composite-f64-atmosphere-first'
CONTROL = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-candidate-first'
BASELINE = 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-baseline-freeze-first.json'
VALIDATION = 'artifacts/optical-composite-f64-atmosphere-validation-first'
METRICS = ('position_m', 'path_m', 'direction')
LIMITS = dict(zip(METRICS, (5., 5., 3e-6)))


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def sha(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify_rows(rows):
    hashes = {}
    for row in rows:
        path = row['path'].replace('\\', '/')
        assert sha(path) == row['sha256'], path
        assert path not in hashes or hashes[path] == row['sha256']
        hashes[path] = row['sha256']
    return hashes


def errors(words, ref):
    # Preserve the prospectively frozen binary64 metric convention. This is
    # an independent recomputation from raw words, not a new physical oracle.
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


def run():
    retained = load(PREFIX+'-independent-review.json')
    assert frozen.review(PREFIX) == retained, 'Frozen auditor replay differs'
    assert retained['passed'] and not retained['full_frame_timing_or_production_authorized']
    sources = load(PREFIX+'-sources.json')
    compiled = load(PREFIX+'-compile-inputs.json')
    stable = load(PREFIX+'-input-stability.json')
    assert sources['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert sources['files'][:-1] == compiled['inputs'] == stable['after_run']
    inputs = verify_rows(sources['files'])
    outputs = verify_rows(load(PREFIX+'-outputs.json'))
    controls = verify_rows(load(CONTROL+'-outputs.json'))
    assert inputs == retained['candidate_input_hashes'] and outputs == retained['candidate_output_hashes']
    assert controls == retained['atmosphere']['historical_control_output_hashes']
    life = load(PREFIX+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None
    assert not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['arguments'] == ['--name', Path(PREFIX).name, '--cartesian-composite-f64', 'atmosphere-100-gradient']
    assert sources['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    assert sha(PREFIX+'-executable.exe') == life['executable_sha256'] == sources['files'][-1]['sha256']
    equality = []
    for suffix in ('-prefix.wgsl', '-suffix.wgsl', '-fallback.wgsl', '-primary.bin', '-field.bin',
                   '-geometry.bin', '-frames.bin', '-certificates.bin'):
        assert (ROOT/(PREFIX+suffix)).read_bytes() == (ROOT/(VALIDATION+suffix)).read_bytes()
        equality.append({'suffix': suffix, 'sha256': sha(PREFIX+suffix), 'exact': True})
    report = load(PREFIX+'.json')
    assert report['selected'] and not report['timed'] and report['iterations'] == []
    assert report['auxiliary_layout'] is None and report['record_count'] == 23
    assert report['state_words_per_record'] == 24 and report['diagnostic_bytes_per_record'] == 32
    raw = np.fromfile(ROOT/(PREFIX+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(PREFIX+'-states.bin'), dtype='<u4').reshape(23, 24)
    old = np.fromfile(ROOT/(CONTROL+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(PREFIX+'-progress.bin'), dtype='<u4')
    primary = np.fromfile(ROOT/(PREFIX+'-primary.bin'), dtype='<u4')
    assert raw.nbytes == 1856 and state.nbytes == 2208 and progress.nbytes == 16
    assert np.array_equal(raw[:4], primary[44:48])
    assert raw[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(terminal.copy().view('<f4')[:, 8:11]).all()
    assert np.isfinite(state.copy().view('<f4')[:, :11]).all()
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.array_equal(state[:, 21], np.arange(23))
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.all(state[:, 22] == 1) and np.all(state[:, 15] == 0)
    assert np.all(state[:, 12:15] == np.array([1, 3, 0]))
    assert np.all(state[:, 12].astype(np.uint64)+state[:, 13] <= 4096)
    assert progress.tolist() == report['progress_words'] == [0, 23, 4, 0]
    physical = list(range(13))+list(range(16, 22))+[23]
    assert np.array_equal(state[:, physical], old[:, physical])
    baseline = load(BASELINE)
    contract = load(baseline['contract'])
    assert sha(baseline['contract']) == baseline['contract_sha256']
    assert baseline['candidate_readback_used'] is False and baseline['eligible_for_same_packet_candidate']
    for path, digest in contract['input_sha256'].items():
        assert sha(path) == digest
    references = {r['record_index']: r for r in baseline['references']}
    assert sorted(references) == list(range(23))
    metrics, launches = [], Counter()
    for i in range(23):
        ref, original = references[i], contract['records'][i]
        assert detail[i, 4:7].tolist() == ref['launch_words'] == original['launch_words']
        launches[tuple(original['launch_words'])] += 1
        assert terminal[i, 7] == original['original_status'] == 0
        measured = errors(terminal[i], ref)
        original_error = errors(original['original_terminal_words'], ref)
        assert original_error == ref['original_error']
        ceilings = {m: Q(original_error[m])+Q(ref['reference_convergence'][m]) for m in METRICS}
        assert all(ceilings[m] == Q(ref['per_metric_ceiling_exact'][m]) for m in METRICS)
        same_ray = {m: Q(measured[m]) <= ceilings[m] for m in METRICS}
        absolute = {m: Q(measured[m]) < Q(LIMITS[m]) for m in METRICS}
        assert all(same_ray.values()) and all(absolute.values())
        authored = retained['atmosphere']['records'][i]
        assert measured == authored['candidate_error']
        assert same_ray == authored['accepted_suffix_metric_checks'] and absolute == authored['absolute_checks']
        assert authored['passed'] and authored['physical_handoff_and_prefix_exact']
        sample = report['samples'][i]
        assert sample['record_index'] == i and sample['pixel'] == original.get('pixel', original.get('pixel_label'))
        assert sample['terminal_words'] == terminal[i].tolist() and sample['state_words'] == state[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist()
        metrics.append({'record_index': i, 'launch_words': original['launch_words'], 'errors': measured,
                        'frozen_ceilings_exact': {m: str(q) for m, q in ceilings.items()},
                        'same_ray_checks': same_ray, 'absolute_checks': absolute})
    assert len(launches) == 3 and sorted(launches.values()) == [7, 8, 8]
    selected = retained['selected_reuse']
    assert selected['historical_required_route_verdict'] == 'REJECT' and selected['passed']
    assert selected['selected']['route_counts'] == {'unhanded-original': 13, 'original-fallback': 1, 'accepted-suffix': 9}
    old_rejection = load('artifacts/optical-cartesian-f64-selected-first-independent-review.json')
    assert old_rejection['passed'] is False and old_rejection['failed_records'] == [3]
    paths = [__file__, 'scripts/review_optical_cartesian_composite.py', BASELINE, baseline['contract'],
             'docs/science/cartesian-composite-f64-policy.md', PREFIX+'-independent-review.json',
             PREFIX+'.json', PREFIX+'-sources.json', PREFIX+'-outputs.json', PREFIX+'-lifecycle.json',
             PREFIX+'-readback.bin', PREFIX+'-states.bin', PREFIX+'-progress.bin']
    return {'schema': 'cartesian-composite-atmosphere-independent-closure-v1', 'review_completed': True,
            'one_atmosphere_case_passed': True, 'decision': 'PASS: first atmosphere final-output gate only',
            'frozen_auditor_replay_exact': True, 'sha256': {p: sha(p) for p in paths},
            'provenance': {'input_hashes_verified': len(inputs), 'output_hashes_verified': len(outputs),
                           'control_output_hashes_verified': len(controls), 'retained_executable_success': True,
                           'unchanged_sources_through_run': True},
            'offline_module_and_upload_equality': equality,
            'route_counts': {'accepted-suffix': 23, 'original-fallback': 0, 'unhanded-original': 0, 'masked': 0},
            'status_counts': {'sky': 23}, 'all_physical_handoffs_and_prefix_counts_exact': True,
            'physical_identity_columns': physical, 'progress_words': progress.tolist(),
            'per_record_prefix_suffix_fallback_counts': [1, 3, 0],
            'unique_launch_populations': [{'launch_words': list(k), 'records': v} for k, v in launches.items()],
            'same_ray_checks_passed': 69, 'strict_absolute_checks_passed': 69,
            'maximum_errors': {m: max(row['errors'][m] for row in metrics) for m in METRICS},
            'metric_rows': metrics, 'historical_selected_required_route_verdict': 'REJECT',
            'selected_reuse_scope': 'Separate retained final-output evidence: nine suffix completions and fourteen exact original terminals; old required-route rejection unchanged.',
            'gpu_executions': 0, 'ode_executions': 0, 'timing_interpreted': False,
            'limitations': ['Three unique launches repeated across 23 records, not 23 independent physical cases.',
                            'Atmosphere-only no-contact/covered-h0 stubs; no terrain or returning-ray acceptance.',
                            'Frozen finite reference metrics, not a global ODE-error enclosure.',
                            'Remaining matrix, prospectively frozen returning fallback, full-chain speed and native checks remain open.']}


if __name__ == '__main__':
    result = run()
    with (ROOT/(PREFIX+'-closure-review.json')).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps({k: result[k] for k in ('review_completed', 'one_atmosphere_case_passed', 'route_counts',
                                           'same_ray_checks_passed', 'strict_absolute_checks_passed', 'maximum_errors')}, indent=2))
