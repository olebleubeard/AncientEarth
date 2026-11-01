"""Raw scheduling/parity audit for the selected Cartesian suffix diagnostic.

This checks recorded execution and exact original-terminal preservation. It
does not evaluate suffix accuracy, prove the executed intermediate curves, or
make a throughput claim. The independent prospective-reference audit is a
separate gate.
"""
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]
CANDIDATE = 'artifacts/optical-cartesian-suffix-selected-first'
ORIGINAL = 'artifacts/optical-normal-scene-reference-selected'
COUNTED = 'artifacts/optical-wavefront-counted-selected-first'


def read(path):
    return json.loads((ROOT / path).read_text(encoding='utf-8-sig'))


def raw(path):
    return (ROOT / path).read_bytes()


def digest(path):
    with (ROOT / path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def words(path):
    data = raw(path)
    assert len(data) % 4 == 0
    return list(struct.unpack('<' + 'I' * (len(data) // 4), data))


def floats(values):
    return list(struct.unpack('<' + 'f' * len(values), struct.pack('<' + 'I' * len(values), *values)))


def signed(value):
    return struct.unpack('<i', struct.pack('<I', value))[0]


def verify_hash_rows(rows):
    for row in rows:
        assert digest(row['path']) == row['sha256'], row['path']


def records(prefix):
    report = read(prefix + '.json')
    data = words(prefix + '-readback.bin')
    n = report['record_count']
    assert n == 23 and report['diagnostic_bytes_per_record'] == 32
    assert len(data) * 4 == report['output_bytes'] == 16 + n * 80
    assert data[:4] == report['header'] == report['expected_header']
    terminal = [data[4 + 12*i:4 + 12*(i+1)] for i in range(n)]
    detail = [data[4 + 12*n + 8*i:4 + 12*n + 8*(i+1)] for i in range(n)]
    for i, sample in enumerate(report['samples']):
        assert sample['record_index'] == i
        assert sample['terminal_words'] == terminal[i]
        assert sample['diagnostic_words'] == detail[i]
    return report, terminal, detail


def review():
    report, terminal, detail = records(CANDIDATE)
    original, old_terminal, old_detail = records(ORIGINAL)
    counted, counted_terminal, counted_detail = records(COUNTED)
    assert report['cartesian_suffix'] and report['selected'] and not report['timed']
    assert not report['reference'] and report['iterations'] == []
    assert report['state_words_per_record'] == 24
    assert counted_terminal == old_terminal
    assert [s['pixel'] for s in report['samples']] == [s['pixel'] for s in original['samples']] == [s['pixel'] for s in counted['samples']]
    for prefix in (CANDIDATE, ORIGINAL, COUNTED):
        assert read(prefix + '-input-stability.json')['inputs_unchanged_through_run']
        verify_hash_rows(read(prefix + '-outputs.json'))
    sources = read(CANDIDATE + '-sources.json')
    compilation = read(CANDIDATE + '-compile-inputs.json')
    stability = read(CANDIDATE + '-input-stability.json')
    assert sources['compiler_inputs_unchanged']
    assert sources['files'][:-1] == stability['after_run']
    assert sources['files'][-1]['path'].replace('\\', '/') == '.cache/optical-wavefront-probe.exe'
    assert sources['files'][:-1] == compilation['inputs']
    verify_hash_rows(sources['files'])
    lifecycle = read(CANDIDATE + '-lifecycle.json')
    assert lifecycle['exit_code'] == 0 and not lifecycle['timed_out']
    assert not lifecycle['terminated_by_wrapper'] and lifecycle['failure'] is None
    validation = read(CANDIDATE + '-validation.json')
    assert validation['all_modules_valid'] and validation['module_count'] == 3
    assert validation['modules'] == report['modules'] == ['prefix', 'suffix', 'fallback']
    assert report['steps'] == [
        {'module_index': 0, 'entry': 'cs_prefix', 'dispatch': [1, 1, 1], 'repetitions': 1},
        {'module_index': 1, 'entry': 'cs_suffix', 'dispatch': [1, 1, 1], 'repetitions': 1},
        {'module_index': 2, 'entry': 'cs_fallback', 'dispatch': [1, 1, 1], 'repetitions': 1},
        {'module_index': 2, 'entry': 'cs_validate', 'dispatch': [1, 1, 1], 'repetitions': 1},
        {'module_index': 2, 'entry': 'cs_finalize', 'dispatch': [1, 1, 1], 'repetitions': 1}]
    uploads = ('primary', 'geometry', 'frames', 'field', 'certificates')
    for kind in uploads:
        assert raw(CANDIDATE + '-' + kind + '.bin') == raw(ORIGINAL + '-' + kind + '.bin') == raw(COUNTED + '-' + kind + '.bin'), kind
    assert raw(CANDIDATE + '.wgsl') == raw(CANDIDATE + '-prefix.wgsl')
    state_words = words(CANDIDATE + '-states.bin')
    assert len(state_words) == 23 * 24 and report['state_bytes'] == len(state_words) * 4
    states = [state_words[24*i:24*(i+1)] for i in range(23)]
    progress = words(CANDIDATE + '-progress.bin')
    assert progress == report['progress_words']
    primary = floats(words(CANDIDATE + '-primary.bin'))
    count = int(primary[36])
    assert primary[36] == count and 2 <= count <= 128
    layers = [primary[48 + 4*i:52 + 4*i] for i in range(count)]
    assert all(a[0] < b[0] for a, b in zip(layers, layers[1:]))
    rows = []
    route_counts = Counter()
    status_counts = Counter()
    changed_words = 0
    for i, (t, old, d, od, state) in enumerate(zip(terminal, old_terminal, detail, old_detail, states)):
        assert state == report['samples'][i]['state_words']
        p_attempt, s_attempt, f_attempt, reason = state[12:16]
        phase, owner, record, route, reserved = state[11], signed(state[20]), state[21], state[22], state[23]
        assert phase == 0 and record == i and reserved == 0 and route in range(4)
        assert p_attempt + s_attempt <= 4096 and f_attempt <= 4096
        assert reason in (0, 2, 3, 4, 5)
        assert d[:4] == [p_attempt, s_attempt, f_attempt, route | (reason << 8)]
        assert d[4:8] == od[4:8] == counted_detail[i][4:8]
        assert state[8:11] == d[4:7]
        assert t[7] == old[7]
        assert all(math.isfinite(x) for x in floats(t[:7] + t[8:11]))
        changes = [j for j, (a, b) in enumerate(zip(t, old)) if a != b]
        changed_words += len(changes)
        if route in (0, 2, 3):
            assert t == old
        if route == 0:
            assert s_attempt == f_attempt == reason == 0 and owner == -2
            assert p_attempt == counted_detail[i][0]
            assert state[:8] == [0]*8 and state[16:20] == [0]*4
        elif route == 1:
            assert t[7] == 0 and s_attempt > 0 and f_attempt == reason == 0
        elif route == 2:
            assert s_attempt > 0 and reason != 0 and f_attempt == counted_detail[i][0]
        else:
            assert t[7] == 3 and state[12:16] == [0]*4
        handoff = None
        if route in (1, 2):
            assert p_attempt > 0 and -1 <= owner < count-1
            pf, uf, gf = floats(state[:4]), floats(state[4:8]), floats(state[16:20])
            assert all(math.isfinite(x) for x in pf + uf + gf)
            assert 0 <= uf[3] <= pf[3]
            if t[7] == 0:
                assert pf[3] <= floats([t[3]])[0]
            # Exact stored height/packed-knot association, not a geodetic proof
            # and not a replay of the shader's finite selection-band evaluation.
            lower = primary[42] if owner == -1 else layers[owner][0]
            upper = primary[41] if owner == -1 else layers[owner+1][0]
            assert lower < gf[3] < upper
            handoff = {'position_m': pf[:3], 'path_m': pf[3], 'direction': uf[:3],
                       'entry_path_m': uf[3], 'stored_geometry': gf,
                       'source_owner': owner, 'packed_owner_bounds_m': [lower, upper]}
        route_counts[str(route)] += 1
        status_counts[str(t[7])] += 1
        rows.append({'record_index': i, 'pixel': report['samples'][i]['pixel'], 'route': route,
                     'original_status': old[7], 'status': t[7], 'phase': phase,
                     'counts': {'prefix': p_attempt, 'suffix': s_attempt, 'fallback': f_attempt,
                                'original_complete': counted_detail[i][0]},
                     'failure_reason': reason, 'handoff': handoff,
                     'changed_terminal_word_indices': changes, 'terminal_words': t,
                     'original_terminal_words': old, 'state_words': state, 'diagnostic_words': d})
    assert progress == [0, 23, max(sum(s[12:15]) for s in states), 0]
    assert dict(status_counts) == report['raw_status_counts']
    outputs = read(CANDIDATE + '-outputs.json')
    output_hashes = {item['path']: item['sha256'] for item in outputs}
    baseline_hashes = {p + s: digest(p + s) for p in (ORIGINAL, COUNTED)
                       for s in ('.json', '-readback.bin', '-primary.bin', '-geometry.bin', '-frames.bin', '-field.bin', '-certificates.bin')}
    return {'schema': 'cartesian-suffix-runtime-review-v1', 'candidate': CANDIDATE,
            'scope': __doc__.strip(), 'source_input_count': len(sources['files']),
            'output_hash_count': len(outputs), 'source_inputs_and_outputs_verified': True,
            'five_uploads_exact_original_and_counted': True, 'all_23_launches_exact': True,
            'all_statuses_unchanged': True, 'route_counts': dict(route_counts),
            'status_counts': dict(status_counts), 'progress_recomputed': progress,
            'changed_terminal_words': changed_words,
            'exact_original_terminal_records': sum(r['route'] in (0, 2, 3) for r in rows),
            'positive_suffix_completions': route_counts['1'],
            'fallback_runtime_exercised': route_counts['2'] > 0,
            'aperture_mask_runtime_exercised': route_counts['3'] > 0,
            'attempt_work': {'candidate_total': sum(sum(s[12:15]) for s in states),
                             'original_total': sum(d[0] for d in counted_detail)},
            'pipeline_creation_seconds': report['pipeline_creation'],
            'runtime_or_throughput_timing_available': False,
            'runtime_layout_and_parity_gate_passed': True,
            'usefulness_gate_positive': route_counts['1'] > 0,
            'numerical_nonregression_gate': 'Pending separate frozen per-ray reference audit; runtime parity does not establish suffix accuracy.',
            'source_input_hashes': sources['files'], 'candidate_output_hashes': output_hashes,
            'baseline_hashes': baseline_hashes, 'records': rows}


if __name__ == '__main__':
    destination = ROOT / (CANDIDATE + '-runtime-review.json')
    assert not destination.exists(), 'Preserve retained reviews.'
    result = review()
    destination.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    print(json.dumps({key: value for key, value in result.items()
                      if key not in ('records', 'source_input_hashes', 'candidate_output_hashes', 'baseline_hashes')}, indent=2))
