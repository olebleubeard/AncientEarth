"""Source/packet identity for the returning candidate, with no GPU or ODE run."""
import hashlib
import json
from pathlib import Path

import review_optical_cartesian_returning_baseline as original

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = 'artifacts/optical-composite-f64-returning-reference-validation-second'
CANDIDATE = 'artifacts/optical-composite-f64-returning-validation-first'
OUTPUT = 'artifacts/optical-composite-f64-returning-candidate-source-review-first.json'
PARSER_REVIEW = 'artifacts/optical-returning-parser-source-review-first.json'
ORIGINAL_REVIEW = ORIGINAL+'-independent-review.json'
SCRIPT = 'scripts/review_optical_cartesian_composite_returning_source.py'
UPLOADS = ('-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin')


def digest(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def review():
    paths = set(original.input_paths()) | {SCRIPT, PARSER_REVIEW, ORIGINAL_REVIEW}
    parser = load(PARSER_REVIEW)
    assert parser['passed'] and parser['ode_executions'] == parser['gpu_executions'] == 0
    for path, sha in parser['sha256'].items():
        assert digest(path) == sha, path
        paths.add(path)
    reference_checks = original.source_gate(ORIGINAL)
    packet_checks = original.packet_gate(ORIGINAL)
    retained = load(ORIGINAL_REVIEW)
    assert retained['passed'] and retained['assembled_module_checks'] == reference_checks
    assert retained['packet_checks'] == packet_checks
    modules = []
    for prefix, reference in ((ORIGINAL, True), (CANDIDATE, False)):
        path = prefix+'-validation.json'
        paths.add(path)
        report = load(path)
        assert report['cartesian_composite_f64'] == 'returning-gradient'
        assert report['cartesian_composite_reference'] is reference
        assert report['selected'] and report['state_words_per_record'] == 24
        assert report['all_modules_valid'] and report['module_count'] == 3
    for name in ('prefix', 'suffix', 'fallback'):
        old_path, new_path = ORIGINAL+'-'+name+'.wgsl', CANDIDATE+'-'+name+'.wgsl'
        paths.update((old_path, new_path))
        old, new = (ROOT/old_path).read_bytes(), (ROOT/new_path).read_bytes()
        if name == 'prefix':
            block = original.HANDOFF.encode('utf-8')
            assert new.count(block) == 1 and old.count(block) == 0
            assert new.replace(block, b'', 1) == old
        else:
            assert new == old
        modules.append({'name': name, 'sha256': digest(new_path),
                        'original_sha256': digest(old_path),
                        'whole_module_exact_declared_transform': True})
    uploaded = {}
    for suffix in UPLOADS:
        old_path, new_path = ORIGINAL+suffix, CANDIDATE+suffix
        paths.update((old_path, new_path))
        assert (ROOT/old_path).read_bytes() == (ROOT/new_path).read_bytes(), suffix
        uploaded[suffix] = digest(new_path)
    assert original.packet_gate(CANDIDATE) == packet_checks
    return {'schema': 'cartesian-composite-returning-candidate-source-v1', 'passed': True,
            'candidate_prefix': CANDIDATE, 'original_prefix': ORIGINAL,
            'modules': modules, 'five_uploads_exact': True, 'uploaded_sha256': uploaded,
            'original_source_review': ORIGINAL_REVIEW, 'parser_review': PARSER_REVIEW,
            'reference_module_checks': reference_checks, 'packet_checks': packet_checks,
            'sha256': {path: digest(path) for path in sorted(paths)},
            'gpu_executions': 0, 'ode_executions': 0,
            'scope': 'Complete candidate modules equal the corrected original after removing exactly the optional handoff block. All five uploads are identical. No observed handoff, runtime result, reference convergence or candidate fallback is inferred.'}


if __name__ == '__main__':
    result = review()
    with (ROOT/OUTPUT).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps({'passed': result['passed'], 'modules': len(result['modules']),
                      'uploads': len(result['uploaded_sha256']), 'output': OUTPUT}))
