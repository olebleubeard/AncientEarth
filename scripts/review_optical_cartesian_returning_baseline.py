"""Freeze the returning ORIGINAL and its first-downward-h0 reference.

Only an original-reference GPU report is accepted. The one actual launch is
memoized for the two reference integrations, while all 23 raw records remain
separate baseline contracts. This is not the old fixed-path Bouguer endpoint.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import sys

import numpy as np
import scipy

from review_optical_cartesian_suffix_source import function, replace_once
from verify_optical_cartesian_turning import reference

ROOT = Path(__file__).resolve().parents[1]
CASE = 'returning-gradient'
POLICY = 'docs/science/cartesian-composite-f64-policy.md'
ORACLE = 'scripts/verify_optical_cartesian_turning.py'
ORACLE_SHA = '98d6bcd10be5f6c0aaeb9c9a380439fe168968471d007b1871801857a5955261'
INPUT = 'artifacts/optical-bouguer-reference.json'
SOURCE_REVIEW = 'artifacts/optical-cartesian-f64-source-review-first.json'
CONTROL = 'artifacts/optical-cartesian-f64-selected-validation-first'
NAME = 'outward turn and return to initial height'
LAYERS = [(0., .0005), (1000., .0003), (2000., .0001), (3000., 0.), (120000., 0.)]
ORIGIN = np.array([6379137., 0., 0.])
BASIS = np.array([[0., 1., 0.], [1., 0., 0.], [0., 0., -1.]])
SEED_WORDS = [1065353114, 996459370, 2147483648]
# Prospective finite convergence criteria, reviewed before the original run.
# They are not error allowances for the GPU and do not enclose the exact ODE.
CONVERGENCE_LIMITS = {'position_m': 1e-3, 'path_m': 1e-3, 'direction': 1e-9}
HANDOFF = '''        if can_handoff {
            let owner=cs_initial_owner(state,g);
            if owner>=-1 && g.w!=0.0 && cs_point_scene_clear(state.p,max(0.002,4e-7*length(state.p))){
                cs_handoff(id,state,path,entry,g,owner,u32(i));return;
            }
            can_handoff=false;
        }
'''
LAUNCH = 'let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);'


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def text(path):
    return (ROOT/path).read_text(encoding='utf-8-sig')


def sha(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write_new(path, value):
    with (ROOT/path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write('\n')


def input_paths():
    paths = {POLICY, ORACLE, INPUT, SOURCE_REVIEW,
             'scripts/review_optical_cartesian_returning_baseline.py',
             'scripts/review_optical_cartesian_suffix_source.py',
             'scripts/optical_cartesian_returning_reference_inputs.txt',
             'scripts/optical_cartesian_suffix_turning_fixture.rs',
             'scripts/optical_cartesian_composite_fixture.rs'}
    paths.update(p.replace('\\', '/') for p in load(SOURCE_REVIEW)['sha256'])
    for tag in ('prefix.wgsl', 'suffix.wgsl', 'fallback.wgsl', 'primary.bin', 'field.bin', 'certificates.bin'):
        paths.add(CONTROL+'-'+tag)
    assert all((ROOT/p).is_file() for p in paths)
    return sorted(paths)


def declared_input():
    assert sha(ORACLE) == ORACLE_SHA, 'The independently reviewed oracle changed'
    case = [r for r in load(INPUT)['cases'] if r['name'] == NAME]
    assert len(case) == 1
    case = case[0]
    assert case['origin_ecef_m'] == ORIGIN.tolist() and case['profile_base_height_m'] == 0.
    assert [(r['height_m'], r['refractivity']) for r in case['layers']] == LAYERS
    # The original decimal seed is only an upload/source check. The reference
    # later takes the actual normalized GPU launch, not these decimal values.
    seed = np.array(case['initial_direction'], dtype=float)
    assert seed.tolist() == [.0034906514152237326, .9999939076577904, 0.]
    local = np.array([seed[1], seed[0], -seed[2]], dtype='<f4')
    assert local.view('<u4').tolist() == SEED_WORDS
    return {'origin_ecef_m': ORIGIN.tolist(), 'profile_base_height_m': 0.,
            'layers': [{'height_m': h, 'refractivity': n} for h, n in LAYERS]}


def source_gate(prefix):
    declared_input()
    report_path = prefix+('.json' if (ROOT/(prefix+'.json')).exists() else '-validation.json')
    report = load(report_path)
    assert report['cartesian_composite_f64'] == CASE and report['cartesian_composite_reference']
    assert report['selected'] and report['state_words_per_record'] == 24
    checked = load(SOURCE_REVIEW)
    assert checked['passed'] and checked['schema'] == 'cartesian-f64-suffix-independent-source-review-v1'
    for path, digest in checked['sha256'].items():
        assert sha(path) == digest, path
    replacement = 'let direction=normalize(bitcast<vec3<f32>>(vec3<u32>('+','.join(str(w)+'u' for w in SEED_WORDS)+')));'
    rows = []
    for name in ('prefix', 'suffix', 'fallback'):
        expected = text(CONTROL+'-'+name+'.wgsl')
        if name == 'prefix':
            expected = replace_once(expected, LAUNCH, replacement)
            expected = replace_once(expected, HANDOFF, '')
        assert text(prefix+'-'+name+'.wgsl') == expected, 'Unexpected '+name+' transformation'
        rows.append({'module': name, 'sha256': sha(prefix+'-'+name+'.wgsl'),
                     'control_sha256': sha(CONTROL+'-'+name+'.wgsl'), 'whole_module_exact_declared_transform': True})
    prefix_source = text(prefix+'-prefix.wgsl')
    assert 'cs_handoff(' not in function(prefix_source, 'cs_prefix_trace')
    for name in ('scene_event', 'terrain_coverage'):
        assert function(prefix_source, name) == function(text(CONTROL+'-prefix.wgsl'), name)
    return rows


def packet_gate(prefix, report=None):
    declared_input()
    words = np.fromfile(ROOT/(prefix+'-primary.bin'), dtype='<u4')
    old = np.fromfile(ROOT/(CONTROL+'-primary.bin'), dtype='<u4')
    assert words.size == old.size == 560
    floats = words.copy().view('<f4').reshape(140, 4)
    assert np.isfinite(floats).all()
    permitted = set(range(20)) | set(range(36, 44)) | set(range(48, 560))
    changed = np.flatnonzero(words != old).tolist()
    assert set(changed) <= permitted
    assert np.array_equal(words[20:36], old[20:36]) and np.array_equal(words[44:48], old[44:48])
    assert floats[8, 2] == 0., 'Returning packet must not mask pixels by fisheye'
    a, flattening, height = 6378137., 1./298.257223563, 1000.
    b = a*(1.-flattening)
    expected_q = np.zeros((3, 4), dtype='<f4')
    expected_q[0, 0] = expected_q[1, 1] = 1./a
    expected_q[2, 2] = a/(b*b)
    assert np.array_equal(floats[:3], expected_q)
    assert np.array_equal(floats[3], np.asarray([0., (a+height)/a, 0., 2.*height+height*height/a], dtype='<f4'))
    assert np.array_equal(floats[4], np.asarray([0., -a-height, 0., height], dtype='<f4'))
    layer_values = np.zeros((128, 4), dtype='<f4')
    slopes = [(LAYERS[i+1][1]-n)/(LAYERS[i+1][0]-h) if i+1<len(LAYERS) else 0.
              for i, (h, n) in enumerate(LAYERS)]
    gradient = max(abs(x) for x in slopes)
    for i, ((h, n), slope) in enumerate(zip(LAYERS, slopes)):
        layer_values[i] = [h, n, slope, 0.]
    assert np.array_equal(words[48:].reshape(128, 4), layer_values.view('<u4'))
    e2 = flattening*(2.-flattening)
    assert np.array_equal(floats[9], np.asarray([5., 120000., gradient+1./(a*(1.-e2)-10000.), 1.], dtype='<f4'))
    assert np.array_equal(floats[10], np.asarray([gradient, 0., -10000., 0.], dtype='<f4'))
    geometry = np.fromfile(ROOT/(prefix+'-geometry.bin'), dtype='<u4')
    frames = np.fromfile(ROOT/(prefix+'-frames.bin'), dtype='<u4').reshape(-1, 4)
    assert geometry.size == 4 and np.all(geometry == 0) and frames.shape == (6, 4)
    expected_frames = np.zeros((6, 4), dtype='<f4')
    expected_frames[2, 1] = 1000.
    expected_frames[3, 0] = expected_frames[4, 1] = expected_frames[5, 2] = 1.
    expected_frames.view('<u4')[1, 0] = 6
    # Signed zero has no geometric distinction in this analytic packet check;
    # every actual bit remains pinned by the retained upload hashes.
    assert np.array_equal(frames.view('<f4'), expected_frames)
    for name in ('field', 'certificates'):
        assert (ROOT/(prefix+'-'+name+'.bin')).read_bytes() == (ROOT/(CONTROL+'-'+name+'.bin')).read_bytes()
    if report is not None:
        metadata = report['fixture']['cartesian_suffix_turning']
        assert metadata['case'] == CASE and metadata['retained_input'] == INPUT and metadata['retained_name'] == NAME
        assert metadata['origin_ecef_m'] == ORIGIN.tolist() and metadata['eye_geodetic'] == [0., 0., 1000.]
        assert metadata['layers'] == [list(x) for x in LAYERS] and metadata['base_height_m'] == 0.
        assert metadata['raw_local_launch_words'] == SEED_WORDS
        assert metadata['unique_rays'] == 1 and metadata['duplicate_records'] == 22
        composite = report['fixture']['cartesian_composite_f64']
        assert composite['case'] == CASE and composite['reference'] and composite['common_suffix_byte_exact']
        assert composite['primary_changed_word_indices'] == changed
        assert 'cartesian_suffix_atmosphere' not in report['fixture']
    return {'primary_changed_word_indices': changed, 'raw_local_seed_words': SEED_WORDS,
            'coherent_empty_geometry_and_six_vec4_frame': True,
            'five_upload_sha256': {name: sha(prefix+'-'+name+'.bin') for name in ('primary', 'field', 'geometry', 'frames', 'certificates')}}


def provenance(prefix):
    source, compiled, stable = [load(prefix+'-'+tag+'.json') for tag in ('sources', 'compile-inputs', 'input-stability')]
    assert source['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert source['files'][:-1] == compiled['inputs'] == stable['after_run']
    hashes = {}
    for row in source['files']+load(prefix+'-outputs.json'):
        path = row['path'].replace('\\', '/')
        assert sha(path) == row['sha256'], path
        assert path not in hashes or hashes[path] == row['sha256']
        hashes[path] = row['sha256']
    assert set(input_paths()) <= hashes.keys(), 'Reference dependency missing from the original run inventory'
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and life['failure'] is None and not life['timed_out'] and not life['terminated_by_wrapper']
    assert life['arguments'] == ['--name', Path(prefix).name, '--cartesian-composite-f64', CASE, '--cartesian-composite-reference']
    assert source['executable_retained'] and life['executable_retained'] and life['executable_unchanged_through_run']
    assert source['files'][-1]['path'].replace('\\', '/') == prefix+'-executable.exe'
    assert sha(prefix+'-executable.exe') == life['executable_sha256'] == source['files'][-1]['sha256']
    hashes[prefix+'-outputs.json'] = sha(prefix+'-outputs.json')
    return hashes


def raw_records(prefix, report):
    assert report['cartesian_composite_f64'] == CASE and report['cartesian_composite_reference']
    assert report['selected'] and not report['timed'] and report['iterations'] == []
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['diagnostic_bytes_per_record'] == 32 and report['auxiliary_layout'] is None
    assert report['steps'] == [{'dispatch': [1, 1, 1], 'entry': entry, 'module_index': module, 'repetitions': 1}
                               for entry, module in [('cs_prefix', 0), ('cs_suffix', 1), ('cs_fallback', 2),
                                                     ('cs_validate', 2), ('cs_finalize', 2)]]
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    primary = np.fromfile(ROOT/(prefix+'-primary.bin'), dtype='<u4')
    assert raw.nbytes == report['output_bytes'] == 1856 and state.nbytes == report['state_bytes'] == 2208
    assert progress.nbytes == report['progress_bytes'] == 16
    assert raw[:4].tolist() == report['header'] == report['expected_header'] and report['header_matches']
    assert np.array_equal(raw[:4], primary[44:48])
    terminal, detail = raw[4:280].reshape(23, 12), raw[280:].reshape(23, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.all(terminal[:, 7] == 1) and np.all(terminal[:, 8:12] == 0), 'Original must publish uncovered h0 surface'
    assert np.all(terminal.copy().view('<f4')[:, 3] > 0)
    assert np.all(state[:, :8] == 0) and np.all(state[:, 11] == 0)
    assert np.all((state[:, 12] > 0) & (state[:, 12] <= 4096))
    assert np.all(state[:, 13:20] == 0) and np.all(state[:, 20] == 0xfffffffe)
    assert np.array_equal(state[:, 21], np.arange(23)) and np.all(state[:, 22:24] == 0)
    assert np.array_equal(state[:, 8:11], detail[:, 4:7]) and np.all(detail[:, 7] == 0)
    assert np.array_equal(state[:, 12:16], detail[:, :4])
    assert progress.tolist() == report['progress_words'] == [0, 23, int(state[:, 12].max()), 0]
    launch = detail[:, 4:7].copy().view('<f4').astype(float)
    assert np.isfinite(launch).all() and np.all(launch[:, 0] > 0) and np.all(launch[:, 1] > 0) and np.all(launch[:, 2] == 0)
    # GPU normalization is recorded, not replaced by an assumed exact seed.
    assert len(set(map(tuple, detail[:, 4:7].tolist()))) == 1
    assert len(report['samples']) == 23
    rows = []
    for i, sample in enumerate(report['samples']):
        assert sample['record_index'] == i and sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist() and sample['state_words'] == state[i].tolist()
        rows.append({'record_index': i, 'pixel_label': sample['pixel'], 'original_status': 1,
                     'original_terminal_words': terminal[i].tolist(), 'launch_words': detail[i, 4:7].tolist(),
                     'original_attempt_count': int(state[i, 12]), 'original_route': 0})
    return terminal, detail, rows


def assess_references(coarse, tight):
    failed = []
    for label, ref in [('coarse', coarse), ('tight', tight)]:
        numeric = [*ref['endpoint_ecef_m'], *ref['exit_direction'], ref['geometric_path_m'],
                   ref['optical_path_m'], ref['maximum_bouguer_drift_m'], ref['maximum_direction_norm_drift']]
        if not all(math.isfinite(x) for x in numeric): failed.append(label+'-nonfinite-reference')
        if ref['terminal_kind'] != 'first-downward-h0' or not ref['observed_outward_and_inward']:
            failed.append(label+'-missing-return-to-h0')
        turns, contacts = ref['radial_turns'], ref['source_contacts']
        if len(turns) != 1 or len(contacts) != 2:
            failed.append(label+'-unexpected-event-count')
            continue
        if [(x['kind'], x['source_owner_before']) for x in contacts] != [('downward-source', 1), ('first-downward-h0', 0)]:
            failed.append(label+'-source-chronology')
        if not (0 < turns[0]['path_m'] < contacts[0]['path_m'] < contacts[1]['path_m'] == ref['geometric_path_m']):
            failed.append(label+'-path-chronology')
        if not (turns[0]['source_owner'] == 1 and 1000 < turns[0]['height_m'] < 2000 and
                ref['minimum_radial_velocity'] < 0 < ref['maximum_radial_velocity'] and
                all(x['radial_velocity'] < 0 for x in contacts)):
            failed.append(label+'-directed-turn-and-crossing')
        if abs(contacts[0]['height_m']-1000.) >= 1e-6 or abs(contacts[1]['height_m']) >= 1e-6:
            failed.append(label+'-event-height-residual')
    convergence = {'position_m': float(np.linalg.norm(np.asarray(coarse['endpoint_ecef_m'])-tight['endpoint_ecef_m'])),
                   'path_m': abs(coarse['geometric_path_m']-tight['geometric_path_m']),
                   'direction': float(np.linalg.norm(np.asarray(coarse['exit_direction'])-tight['exit_direction']))}
    for metric, value in convergence.items():
        if not math.isfinite(value) or not value < CONVERGENCE_LIMITS[metric]:
            failed.append(metric+'-reference-convergence')
    return convergence, failed


def freeze(prefix, output):
    assert not (ROOT/(output+'.json')).exists() and not (ROOT/(output+'-contract.json')).exists()
    report = load(prefix+'.json')
    hashes = provenance(prefix)
    sources = source_gate(prefix)
    packet = packet_gate(prefix, report)
    terminal, detail, rows = raw_records(prefix, report)
    inputs = declared_input()
    local_launch = detail[0, 4:7].copy().view('<f4').astype(float)
    ecef_launch = BASIS@local_launch
    norm = float(np.linalg.norm(ecef_launch))
    assert math.isfinite(norm) and norm > 0
    inputs['initial_direction'] = ecef_launch.tolist()
    contract = {'schema': 'cartesian-composite-returning-original-contract-v1',
        'created_utc': datetime.now(timezone.utc).isoformat(), 'case': CASE, 'baseline': prefix,
        'candidate_readback_used': False, 'input_sha256': hashes, 'assembled_module_checks': sources,
        'packet_checks': packet, 'records': rows, 'canonical_inputs': inputs,
        'prospective_policy': POLICY, 'prospective_policy_sha256': sha(POLICY),
        'reference_source_sha256': ORACLE_SHA, 'strict_reference_convergence_limits': CONVERGENCE_LIMITS,
        'event_height_residual_limit_m': 1e-6,
        'reference_initial_normalization': {'actual_gpu_launch_norm': norm,
            'direction_correction_norm': float(np.linalg.norm(ecef_launch/norm-ecef_launch)),
            'scope': 'Existing oracle normalizes actual GPU orientation once, then normalizes final direction. This is the declared physical-orientation hypothesis, not an off-unit exact-state continuation.'},
        'candidate_rule': 'Each of 23 records must actually hand off, take original-eye fallback and match all 12 original terminal words. Source/packet identity and valid counts/progress remain required. No canonical tolerance replaces exact fallback.',
        'original_error_scope': 'Reference-relative original errors are retained descriptively. The atmosphere-only absolute limits are not assigned to this returning control.',
        'old_fixed_path_endpoint_used': False,
        'runtime_versions': {'python': sys.version, 'numpy': np.__version__, 'scipy': scipy.__version__}}
    write_new(output+'-contract.json', contract)
    print(json.dumps({'contract': output+'-contract.json', 'reference_integrations': 2, 'unique_actual_launches': 1}), flush=True)
    coarse, tight = reference(dict(inputs), False), reference(dict(inputs), True)
    convergence, failed = assess_references(coarse, tight)
    references = []
    for i, row in enumerate(rows):
        value = terminal[i].copy().view('<f4').astype(float)
        error = {'position_m': float(np.linalg.norm(ORIGIN+BASIS@value[:3]-tight['endpoint_ecef_m'])),
                 'path_m': abs(float(value[3])-tight['geometric_path_m']),
                 'direction': float(np.linalg.norm(BASIS@value[4:7]-tight['exit_direction']))}
        assert all(math.isfinite(v) for v in error.values())
        references.append(dict(row, original_error=error))
    assert all(sha(path) == digest for path, digest in hashes.items())
    result = {'schema': 'cartesian-composite-returning-original-freeze-v1', 'case': CASE, 'baseline': prefix,
              'contract': output+'-contract.json', 'contract_sha256': sha(output+'-contract.json'),
              'candidate_readback_used': False, 'passed': not failed, 'eligible_for_returning_candidate': not failed,
              'failure_reasons': failed, 'record_count': 23, 'unique_reference_launches': 1,
              'original_status_counts': {'surface': 23}, 'original_route_counts': {'unhanded-original': 23},
              'coarse_reference': coarse, 'tight_reference': tight, 'reference_convergence': convergence,
              'strict_reference_convergence_limits': CONVERGENCE_LIMITS, 'references': references,
              'input_hashes_unchanged_through_reference': True,
              'limitations': ['Finite DOP853 convergence and directed-event evidence, not a rigorous exact-ODE tube.',
                              'Observed radial turn belongs to the canonical reference; no GPU interior trajectory was retained.',
                              'One unique launch repeated23times; original endpoints remain separate exact future fallback gates.',
                              'Canonical layers and normalized orientation differ from exact uploaded f32 arithmetic; those model/rounding discrepancies are included in reported original error.',
                              'Candidate fallback execution, complete matrix and all performance/native gates remain open.']}
    write_new(output+'.json', result)
    print(json.dumps({'passed': result['passed'], 'reference_convergence': convergence, 'failure_reasons': failed}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    parser.add_argument('--source-only', action='store_true')
    args = parser.parse_args()
    if args.source_only:
        result = {'schema': 'cartesian-composite-returning-original-source-v1', 'passed': True,
                  'assembled_module_checks': source_gate(args.prefix), 'packet_checks': packet_gate(args.prefix),
                  'prospective_reference_convergence_limits': CONVERGENCE_LIMITS,
                  'scope': 'Source and packet prerequisites only; no GPU readback or reference integration.'}
        write_new(args.output, result)
        print(json.dumps(result, indent=2))
    else:
        freeze(args.prefix, args.output)
