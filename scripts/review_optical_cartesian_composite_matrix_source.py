"""Exact assembled-source/upload gate for nine remaining frozen matrix packets.

No candidate readback, GPU dispatch, ODE integration or new numerical policy.
Returning source/packet and numerical-reference gates are separate.
"""
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
BASE = 'artifacts/optical-cartesian-f64-selected-first'
SOURCE_REVIEW = 'artifacts/optical-cartesian-f64-source-review-first.json'
DESTINATION = 'artifacts/optical-composite-f64-matrix-source-review-first.json'
CASES = ('readiness', 'fisheye', 'vacuum-outward', 'vacuum-inward',
         'atmosphere-minus20-vacuum', 'atmosphere-minus20-gradient',
         'atmosphere-100-vacuum', 'atmosphere-5000-vacuum', 'atmosphere-5000-gradient')
PARTS = ('-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin')
LAUNCH = 'let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);'


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def sha(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def text(path):
    # Preserve generated newlines: comparison below is complete encoded bytes.
    return (ROOT/path).read_bytes().decode('utf-8')


def stub(source, name, replacement):
    matches = list(re.finditer(r'\bfn '+name+r'\(', source))
    assert len(matches) == 1, (name, len(matches))
    opening = source.index('{', matches[0].start())
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[:opening+1]+replacement+source[end-1:]


def run():
    review = load(SOURCE_REVIEW)
    assert review['passed']
    base = {name: text(BASE+'-'+name+'.wgsl') for name in ('prefix', 'suffix', 'fallback')}
    paths = {Path(__file__).relative_to(ROOT).as_posix(), SOURCE_REVIEW,
             'scripts/optical_cartesian_composite_fixture.rs',
             'scripts/optical_cartesian_suffix_f64_fixture.rs',
             'scripts/optical_cartesian_suffix_branch_fixture.rs',
             'scripts/optical_cartesian_suffix_atmosphere_fixture.rs',
             'docs/science/cartesian-composite-f64-policy.md'}
    for name in base:
        p = BASE+'-'+name+'.wgsl'
        reviewed = 'artifacts/optical-cartesian-f64-selected-validation-first-'+name+'.wgsl'
        assert (ROOT/p).read_bytes() == (ROOT/reviewed).read_bytes()
        assert sha(reviewed) == review['sha256'][reviewed]
        paths.update((p, reviewed))
    rows = []
    for case in CASES:
        prefix = 'artifacts/optical-composite-f64-matrix-'+case+'-validation-first'
        frozen_path = 'artifacts/optical-cartesian-matrix-'+case+'-baseline-freeze-first.json'
        frozen = load(frozen_path)
        assert frozen['case'] == case and frozen['eligible_for_same_packet_candidate']
        assert not frozen['candidate_readback_used'] and frozen['baseline_source_and_runtime_checks_passed']
        contract_path = frozen['contract']
        assert sha(contract_path) == frozen['contract_sha256']
        contract = load(contract_path)
        assert contract['case'] == case and not contract['candidate_readback_used']
        original = frozen['baseline']
        assert original == contract['baseline'] == 'artifacts/optical-cartesian-matrix-'+case+'-reference-first'
        hashes = {p.replace('\\', '/'): h for p, h in contract['input_sha256'].items()}
        expected = dict(base)
        transform = 'Exact selected f64 modules; uploaded packet only.'
        if case.startswith('atmosphere-'):
            old_prefix = original+'-prefix.wgsl'
            assert sha(old_prefix) == hashes[old_prefix]
            launches = re.findall(r'let atmosphere_rays=.*?;let direction=normalize\(atmosphere_rays\[id.x%3u\]\);', text(old_prefix))
            assert len(launches) == 1
            assert expected['prefix'].count(LAUNCH) == 1
            expected['prefix'] = expected['prefix'].replace(LAUNCH, launches[0])
            for name in ('prefix', 'fallback'):
                expected[name] = stub(expected[name], 'scene_event', 'return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);')
                expected[name] = stub(expected[name], 'terrain_coverage', 'return 1u;')
            paths.add(old_prefix)
            transform = 'Frozen original three-launch statement plus existing no-contact/covered-h0 stubs; all remaining bytes exact.'
        modules = []
        for name, source in expected.items():
            p = prefix+'-'+name+'.wgsl'
            assert (ROOT/p).read_bytes() == source.encode('utf-8'), (case, name)
            paths.add(p)
            modules.append({'name': name, 'sha256': sha(p),
                            'expected_transform': 'Exact unchanged complete f64 suffix.' if name == 'suffix' else transform})
        uploaded = {}
        for part in PARTS:
            p, old = prefix+part, original+part
            assert sha(old) == hashes[old], old
            assert (ROOT/p).read_bytes() == (ROOT/old).read_bytes(), (case, part)
            paths.update((p, old)); uploaded[part] = sha(p)
        validation_path = prefix+'-validation.json'
        validation = load(validation_path)
        assert validation['all_modules_valid'] and validation['module_count'] == 3
        assert validation['modules'] == ['prefix', 'suffix', 'fallback']
        assert validation['cartesian_composite_f64'] == case and not validation['cartesian_composite_reference']
        assert validation['selected'] and validation['state_words_per_record'] == 24
        assert validation['auxiliary_layout'] is None
        paths.update((frozen_path, contract_path, validation_path))
        rows.append({'case': case, 'candidate_prefix': prefix, 'original_baseline': original,
                     'baseline_contract': contract_path, 'baseline_contract_sha256': sha(contract_path),
                     'modules': modules, 'five_uploads_exact': True,
                     'uploaded_sha256': uploaded, 'validation_passed': True})
    return {'schema': 'cartesian-composite-nine-packet-source-review-v1', 'passed': True,
            'cases': rows, 'sha256': {p: sha(p) for p in sorted(paths)},
            'complete_module_comparisons': 27, 'exact_original_upload_comparisons': 45,
            'gpu_executions': 0, 'ode_executions': 0,
            'scope': 'Source and packet identity for nine selected untimed controls only. No candidate output or timing result. Returning reference/candidate source, packet, physical reference and observed fallback remain separate gates.',
            'handoff_scope': 'Source identity does not invent an unrecorded handoff. Raw physical-state equality requires a retained matching control; otherwise it is explicitly unavailable and unchanged full-ray final metrics still apply.'}


if __name__ == '__main__':
    result = run()
    with (ROOT/DESTINATION).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({'passed': result['passed'], 'cases': len(result['cases']),
                      'modules': result['complete_module_comparisons'],
                      'uploads': result['exact_original_upload_comparisons']}))
