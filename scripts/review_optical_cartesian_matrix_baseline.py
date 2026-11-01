"""Freeze original branch packets and per-record references before candidates.

No candidate readback is accepted by this program. Duplicate launch solves may
be memoized, but all 23 original terminal records retain separate error gates.
The old atmosphere absolute limits and the prospective same-ray rule are both
retained. Measured DOP853 convergence is not an exact-ODE error enclosure.
"""
from collections import Counter
from datetime import datetime, timezone
from fractions import Fraction as Q
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

import numpy as np
import scipy

from review_optical_cartesian_suffix_source import function, replace_once
from verify_optical_dop853 import geometry, integrate, differences

ROOT = Path(__file__).resolve().parents[1]
POLICY = 'docs/science/cartesian-suffix-matrix-policy.md'
CONTROL = 'artifacts/optical-cartesian-suffix-tight-selected-first'
METRICS = ('position_m', 'path_m', 'direction')
LIMITS = dict(zip(METRICS, (5., 5., 3e-6)))
CASES = ('readiness', 'fisheye', 'vacuum-outward', 'vacuum-inward',
         'atmosphere-minus20-vacuum', 'atmosphere-minus20-gradient',
         'atmosphere-100-vacuum', 'atmosphere-100-gradient',
         'atmosphere-5000-vacuum', 'atmosphere-5000-gradient')
HANDOFF = '''        if can_handoff {
            let owner=cs_initial_owner(state,g);
            if owner>=-1 && g.w!=0.0 && cs_point_scene_clear(state.p,max(0.002,4e-7*length(state.p))){
                cs_handoff(id,state,path,entry,g,owner,u32(i));return;
            }
            can_handoff=false;
        }
'''
LAUNCH = 'let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);'


def text(path):
    return (ROOT/path).read_text(encoding='utf-8-sig')


def load(path):
    return json.loads(text(path))


def digest(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write_new(path, value):
    with (ROOT/path).open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(value, indent=2, allow_nan=False)+'\n')


def replace_body(source, name, body):
    old = function(source, name)
    new = old[:old.index('{')+1]+body+'}'
    return replace_once(source, old, new)


def source_gate(prefix, case):
    """Whole assembled modules, reversed to the retained tight control."""
    assert case in CASES
    report = load(prefix+('.json' if (ROOT/(prefix+'.json')).exists() else '-validation.json'))
    metadata = report.get('fixture', {})
    assert report['branch_reference'] and report['cartesian_suffix_branch'] == case
    if metadata:
        assert metadata['cartesian_suffix_branch']['reference'] is True
    rows = []
    for name in ('prefix', 'suffix', 'fallback'):
        old = text(CONTROL+'-'+name+'.wgsl')
        expected = old
        if name == 'prefix':
            expected = replace_once(expected, HANDOFF, '')
        if case.startswith('atmosphere-') and name in ('prefix', 'fallback'):
            expected = replace_body(expected, 'scene_event',
                'return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);')
            expected = replace_body(expected, 'terrain_coverage', 'return 1u;')
            if name == 'prefix':
                # Original exact equatorial E/up/-N directions, rounded once
                # on upload. This does not infer them from candidate outputs.
                rays = np.array([[0., 1., 0.], [.5, 1., 0.], [0., 1., -1.]])
                rays /= np.linalg.norm(rays, axis=1)[:, None]
                rays[:2, 2] = -0.0  # LocalFrame explicitly negates north dot.
                words = rays.astype('<f4').view('<u4').tolist()
                if metadata:
                    assert words == metadata['cartesian_suffix_atmosphere']['raw_local_launch_words']
                assert len(words) == 3 and all(len(w) == 3 for w in words)
                vectors = ','.join('bitcast<vec3<f32>>(vec3<u32>('+','.join(str(x)+'u' for x in w)+'))' for w in words)
                replacement = 'let atmosphere_rays=array<vec3<f32>,3>('+vectors+');let direction=normalize(atmosphere_rays[id.x%3u]);'
                expected = replace_once(expected, LAUNCH, replacement)
        actual = text(prefix+'-'+name+'.wgsl')
        assert actual == expected, (case, name, 'whole assembled module differs')
        rows.append({'module': name, 'actual_sha256': digest(prefix+'-'+name+'.wgsl'),
                     'control_sha256': digest(CONTROL+'-'+name+'.wgsl'),
                     'exact_after_only_declared_reference_and_atmosphere_edits': True})
    # Handoff removal is checked on the callable prefix, not by assuming an
    # unused handoff helper was removed by a later compiler pass.
    assert 'cs_handoff(' not in function(text(prefix+'-prefix.wgsl'), 'cs_prefix_trace')
    return rows


def provenance(prefix):
    source = load(prefix+'-sources.json')
    compiled = load(prefix+'-compile-inputs.json')
    stable = load(prefix+'-input-stability.json')
    assert source['compiler_inputs_unchanged'] and stable['inputs_unchanged_through_run']
    assert source['files'][:-1] == compiled['inputs'] == stable['after_run']
    assert source['files'][-1]['path'].endswith('.exe')
    assert any(x['path'].replace('\\', '/') == POLICY for x in compiled['inputs'])
    # Freeze while the source batch is still held. Later candidate audits use
    # these retained hashes, not an assertion that historical mutable sources
    # still equal whatever happens to be in the workspace.
    for row in source['files'][:-1]+load(prefix+'-outputs.json'):
        assert digest(row['path']) == row['sha256'], row['path']
    life = load(prefix+'-lifecycle.json')
    assert life['exit_code'] == 0 and not life['timed_out'] and not life['terminated_by_wrapper']
    paths = [prefix+x for x in ('.json', '-sources.json', '-compile-inputs.json',
             '-outputs.json', '-input-stability.json', '-lifecycle.json')]
    paths += [x['path'] for x in load(prefix+'-outputs.json')]
    paths += [POLICY, 'scripts/review_optical_cartesian_matrix_baseline.py', 'scripts/verify_optical_dop853.py']
    return {p: digest(p) for p in sorted(set(paths))}


def raw_records(prefix, report):
    n = report['record_count']
    assert n == 23 and report['selected'] and not report['timed'] and not report['iterations']
    assert report['state_words_per_record'] == 24 and report['diagnostic_bytes_per_record'] == 32
    assert report['branch_reference'] and report['cartesian_suffix_tight'] and report['cartesian_suffix_trace']
    raw = np.fromfile(ROOT/(prefix+'-readback.bin'), dtype='<u4')
    state = np.fromfile(ROOT/(prefix+'-states.bin'), dtype='<u4').reshape(n, 24)
    aux = np.fromfile(ROOT/(prefix+'-auxiliary.bin'), dtype='<u4').reshape(n, 4097, 4)
    progress = np.fromfile(ROOT/(prefix+'-progress.bin'), dtype='<u4')
    assert raw.nbytes == report['output_bytes'] == 1856
    assert state.nbytes == report['state_bytes'] == 2208
    assert aux.nbytes == report['auxiliary_layout']['bytes'] == 1507696
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    terminal, detail = raw[4:280].reshape(n, 12), raw[280:].reshape(n, 8)
    assert np.isfinite(terminal.copy().view('<f4')[:, :7]).all()
    assert np.isfinite(detail.copy().view('<f4')[:, 4:7]).all()
    assert np.all(state[:, 11] == 0) and np.all(state[:, 23] == 0)
    assert np.all(state[:, 13:15] == 0) and np.all(state[:, 12] <= 4096)
    assert np.all(state[:, 15] == 0) and np.all(state[:, 22] == np.where(terminal[:, 7] == 3, 3, 0))
    assert np.array_equal(state[:, 21], np.arange(n))
    assert np.array_equal(state[:, 8:11], detail[:, 4:7])
    assert np.array_equal(state[:, 12:15], detail[:, :3])
    assert np.array_equal(state[:, 22] | (state[:, 15] << 8), detail[:, 3])
    assert np.all(detail[:, 7] == 0)
    assert progress.tolist() == report['progress_words'] == [0, n, int(state[:, 12].max()), 0]
    assert np.all(aux[:, 0, :2] == 0) and np.all(aux[:, 1:] == 0)
    assert np.array_equal(aux[:, 0, 2], np.arange(n))
    assert np.all(aux[:, 0, 3] == raw[2])
    assert len(report['samples']) == n
    for i, sample in enumerate(report['samples']):
        assert sample['record_index'] == i
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == detail[i].tolist()
        assert sample['state_words'] == state[i].tolist()
    return terminal, detail, state


def packet_gate(prefix, report, case):
    primary = np.fromfile(ROOT/(prefix+'-primary.bin'), dtype='<u4')
    old = np.fromfile(ROOT/(CONTROL+'-primary.bin'), dtype='<u4')
    assert primary.size == old.size == 560
    changes = np.flatnonzero(primary != old).tolist()
    assert changes == report['fixture']['cartesian_suffix_branch']['primary_changed_word_indices']
    assert np.array_equal(primary[44:48], old[44:48])  # Current control token.
    for name in ('field', 'certificates'):
        assert (ROOT/(prefix+'-'+name+'.bin')).read_bytes() == (ROOT/(CONTROL+'-'+name+'.bin')).read_bytes()
    if case in ('readiness', 'fisheye'):
        expected = old.copy()
        if case == 'readiness':
            expected[39] = 0
        else:
            expected[34] = np.float32(1).view(np.uint32)
        assert np.array_equal(primary, expected)
        for name in ('geometry', 'frames'):
            assert (ROOT/(prefix+'-'+name+'.bin')).read_bytes() == (ROOT/(CONTROL+'-'+name+'.bin')).read_bytes()
    else:
        frames = np.fromfile(ROOT/(prefix+'-frames.bin'), dtype='<u4').reshape(-1, 4)
        geom = np.fromfile(ROOT/(prefix+'-geometry.bin'), dtype='<u4')
        assert geom.size == 4 and frames.shape == (6, 4)
        assert np.all(frames[0] == 0) and frames[1, 0] == 6 and frames[2, 3] == 0
        floats = primary.copy().view('<f4').reshape(140, 4)
        if case.startswith('atmosphere-'):
            canonical = canonical_input(report, case)
            layers = canonical['layers']
            height = canonical['origin_ecef_m'][0]-6378137.
            assert np.array_equal(primary[20:36], old[20:36])
            expected_layers = np.zeros((128, 4), dtype='<f4')
            for i, layer in enumerate(layers):
                slope = 0. if i+1 == len(layers) else (layers[i+1]['refractivity']-layer['refractivity'])/(layers[i+1]['height_m']-layer['height_m'])
                expected_layers[i] = [layer['height_m'], layer['refractivity'], slope, 0.]
            assert np.array_equal(primary[48:].reshape(128, 4), expected_layers.view('<u4'))
            assert floats[9, 0] == len(layers) and floats[9, 1] == 120000 and floats[9, 3] == 1
            assert floats[10, 1] == 0 and floats[10, 2] == -10000
        else:
            height = 2000.
            assert floats[9, 0] == 2 and floats[9, 1] == 1000 and floats[9, 3] == 1
            layers = np.zeros((128, 4), dtype='<u4')
            layers[1, 0] = np.float32(1000).view(np.uint32)
            assert np.array_equal(primary[48:].reshape(128, 4), layers)
        # Independent exact equatorial eye; signed zeros are retained in the
        # raw input identity, but have no distinct canonical position here.
        assert floats[4].tolist() == [0., -6378137.-height, 0., height]
        assert floats[3, 1] == np.float32((6378137.+height)/6378137.)
        assert floats[3, 3] == np.float32(2*height+height*height/6378137.)
    return {'primary_changed_word_indices': changes,
            'five_uploads': {name: digest(prefix+'-'+name+'.bin') for name in ('primary', 'field', 'geometry', 'frames', 'certificates')}}


def canonical_input(report, case):
    if case.startswith('atmosphere-'):
        meta = report['fixture']['cartesian_suffix_atmosphere']
        height = {'minus20': -20., '100': 100., '5000': 5000.}[case.split('-')[1]]
        layers = [(0., .0003), (2000., .00023), (120000., 0.)] if case.endswith('-gradient') else [(0., 0.), (120000., 0.)]
        assert meta['eye_geodetic'] == [0., 0., height]
        assert meta['origin_ecef_m'] == [6378137.+height, 0., 0.]
        assert meta['layers'] == [list(x) for x in layers] and meta['base_height_m'] == 0.
        assert meta['unique_rays'] == 3 and meta['duplicate_records'] == 20
        return {'origin_ecef_m': meta['origin_ecef_m'], 'profile_base_height_m': 0.,
                'layers': [{'height_m': h, 'refractivity': n} for h, n in layers]}
    assert case == 'fisheye'
    result = dict(report['fixture']['clear_fixture']['cases'][0]['canonical_inputs'])
    result.pop('target_path_m', None)
    return result


def freeze(prefix, output):
    report = load(prefix+'.json')
    case = report['cartesian_suffix_branch']
    assert case in CASES
    assert not (ROOT/(output+'.json')).exists() and not (ROOT/(output+'-contract.json')).exists()
    hashes = provenance(prefix)
    modules = source_gate(prefix, case)
    packet = packet_gate(prefix, report, case)
    terminal, detail, state = raw_records(prefix, report)
    statuses = terminal[:, 7]
    if case == 'readiness':
        assert np.all(statuses == 2) and np.all(state[:, 12] == 0)
    elif case == 'vacuum-outward':
        assert np.all(statuses == 0) and np.all(terminal[:, 3] == 0) and np.all(state[:, 12] == 0)
    elif case == 'vacuum-inward':
        assert np.all(statuses == 1) and np.all(state[:, 12] > 0)
        assert np.all(terminal.copy().view('<f4')[:, 3] > 0)
        # Owner0/material0 is uncovered reference h0, not terrain or marker.
        assert np.all(terminal[:, 11] == 0)
    elif case == 'fisheye':
        assert np.any(statuses == 3) and np.any(statuses != 3)
        assert np.all(state[statuses == 3, 12] == 0)
    else:
        assert np.all(statuses == 0), 'Original atmosphere clear case did not exit'
    needs_reference = case == 'fisheye' or case.startswith('atmosphere-')
    sky = np.flatnonzero(statuses == 0).tolist() if needs_reference else []
    rows = [{'record_index': i, 'pixel_label': s['pixel'], 'original_status': int(statuses[i]),
             'original_terminal_words': terminal[i].tolist(), 'launch_words': detail[i, 4:7].tolist(),
             'original_attempt_count': int(state[i, 12])} for i, s in enumerate(report['samples'])]
    contract = {
        'schema': 'cartesian-suffix-matrix-baseline-contract-v1', 'created_utc': datetime.now(timezone.utc).isoformat(),
        'case': case, 'baseline': prefix, 'candidate_readback_used': False,
        'input_sha256': hashes, 'assembled_module_checks': modules, 'packet_checks': packet, 'records': rows,
        'executable_provenance': {
            'recorded_executable': load(prefix+'-sources.json')['files'][-1],
            'current_reused_path_sha256': digest(load(prefix+'-sources.json')['files'][-1]['path']),
            'historical_binary_archived_by_this_audit': False,
            'scope': 'Serial wrapper compilations reuse the executable path. Historical hash/compiler arguments and frozen-run lifecycle are retained; current binary equality is not asserted for an earlier run.'},
        'prospective_policy': POLICY, 'prospective_policy_sha256': digest(POLICY),
        'atmosphere_absolute_limits_strict': LIMITS if case.startswith('atmosphere-') else None,
        'original_exact_terminal_required': 'Every unhanded/fallback/masked record; every readiness/vacuum record regardless of candidate route.',
        'accepted_suffix_rule': 'Original and candidate sky; each metric <= SAME original ray error plus SAME measured coarse/tight convergence, using exact rational sums of retained binary64 values. Atmosphere additionally requires each metric strictly below its original absolute limit.',
        'sky_reference_record_indices': sky,
        'source_aware_reference': {'coarse_rtol': 2e-12, 'coarse_max_step_m': 2000., 'tight_rtol': 5e-14, 'tight_max_step_m': 500.},
        'runtime_versions': {'python': sys.version, 'numpy': np.__version__, 'scipy': scipy.__version__},
        'limitations': __doc__.strip()}
    # This is the prospective freeze, emitted before any reference result. A
    # failed solve leaves it intact and never creates an apparently passed file.
    write_new(output+'-contract.json', contract)
    solved = {}
    reference_rows = []
    if sky:
        canonical = canonical_input(report, case)
        origin = np.asarray(canonical['origin_ecef_m'], dtype=float)
        _, up = geometry(origin)
        east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
        basis = np.column_stack((east, up, -np.cross(up, east)))
        for i in sky:
            launch_words = tuple(map(int, detail[i, 4:7]))
            launch = detail[i, 4:7].copy().view('<f4').astype(float)
            inputs = dict(canonical, initial_direction=(basis@launch).tolist())
            if launch_words not in solved:
                coarse, tight = integrate(inputs, False), integrate(inputs, True)
                d = differences(coarse, tight)
                convergence = dict(zip(METRICS, (d['endpoint_m'], d['path_m'], d['direction'])))
                solved[launch_words] = (coarse, tight, convergence)
            coarse, tight, convergence = solved[launch_words]
            f = terminal[i].copy().view('<f4').astype(float)
            error = {'position_m': float(np.linalg.norm(origin+basis@f[:3]-tight['endpoint_ecef_m'])),
                     'path_m': abs(float(f[3])-tight['geometric_path_m']),
                     'direction': float(np.linalg.norm(basis@f[4:7]-tight['exit_direction']))}
            row = dict(rows[i], inputs=inputs, coarse_reference=coarse, tight_reference=tight,
                       original_error=error, reference_convergence=convergence,
                       per_metric_ceiling_exact={m: str(Q(error[m])+Q(convergence[m])) for m in METRICS})
            row['original_absolute_checks'] = {m: Q(error[m]) < Q(LIMITS[m]) for m in METRICS} if case.startswith('atmosphere-') else None
            reference_rows.append(row)
            print(json.dumps({'case': case, 'record': i, 'error': error, 'convergence': convergence}), flush=True)
    assert all(digest(p) == value for p, value in hashes.items())
    absolute_ok = all(all(r['original_absolute_checks'].values()) for r in reference_rows) if case.startswith('atmosphere-') else None
    result = {'schema': 'cartesian-suffix-matrix-baseline-v1', 'created_utc': datetime.now(timezone.utc).isoformat(),
              'case': case, 'baseline': prefix, 'contract': output+'-contract.json', 'contract_sha256': digest(output+'-contract.json'),
              'candidate_readback_used': False, 'baseline_source_and_runtime_checks_passed': True,
              'original_status_counts': dict(Counter(map(int, statuses))), 'record_count': 23,
              'reference_record_count': len(reference_rows), 'unique_reference_launches': len(solved),
              'original_atmosphere_absolute_gate_passed': absolute_ok,
              'eligible_for_same_packet_candidate': absolute_ok is not False,
              'input_hashes_unchanged_through_reference': True, 'references': reference_rows,
              'decision_scope': 'Baseline freeze only. Candidate gates, actual fallback/turning coverage and all performance/production gates remain separate.'}
    write_new(output+'.json', result)
    print(json.dumps({k: v for k, v in result.items() if k != 'references'}, indent=2), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    parser.add_argument('--source-only', action='store_true')
    args = parser.parse_args()
    if args.source_only:
        p = load(args.prefix+('.json' if (ROOT/(args.prefix+'.json')).exists() else '-validation.json'))
        result = {'case': p['cartesian_suffix_branch'], 'baseline': args.prefix,
                  'assembled_module_checks': source_gate(args.prefix, p['cartesian_suffix_branch']),
                  'scope': 'Exact assembled source substitutions only; no GPU or numerical reference acceptance.'}
        write_new(args.output, result)
        print(json.dumps(result, indent=2))
    else:
        freeze(args.prefix, args.output)
