"""Audit isolated accepted-curve scene inputs/outcomes, never whole-ray hits."""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path

import numpy as np


def load_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def review(prefix):
    report = load_json(prefix + '.json')
    cartesian = report.get('cartesian_curves', False) or report.get('cartesian_covered', False)
    assert report['curves'] != cartesian and not any(report[k] for k in ('reference', 'selected', 'timed'))
    assert load_json(prefix + '-input-stability.json')['inputs_unchanged_through_run']
    for record in load_json(prefix + '-outputs.json'):
        assert hashlib.sha256(Path(record['path']).read_bytes()).hexdigest() == record['sha256'], record['path']
    fixture = report['fixture']['retained_curve_fixture'] if cartesian else report['fixture']
    capture = fixture['capture_prefix']
    original = load_json(capture + '.json')
    captured = np.fromfile(capture + '-readback.bin', dtype='<u4').reshape(original['count'], original['record_words'])
    uploaded = np.fromfile(prefix + '-curves.bin', dtype='<u4')
    inputs = uploaded[:495*112].reshape(495, 112)
    n = report['record_count']
    packet_metadata_differences = []
    if cartesian:
        outer = report['fixture']; embedded_packet = outer['cartesian_packet']
        packet_prefix = outer['cartesian_packet_prefix']
        packet = load_json(packet_prefix + '.json')
        assert embedded_packet.keys() == packet.keys()
        for key in packet:
            if key != 'curves':
                assert embedded_packet[key] == packet[key], key
        assert len(embedded_packet['curves']) == len(packet['curves'])
        for i, (copy, original_row) in enumerate(zip(embedded_packet['curves'], packet['curves'], strict=True)):
            assert copy.keys() == original_row.keys()
            for key, value in original_row.items():
                if copy[key] != value:
                    # serde_json parsing/printing can change a binary64
                    # metadata value. Neither copy supplies GPU bounds: the
                    # exact uploaded binary packet is checked separately.
                    assert key in ('remaining_m', 'numerical_source_error_m', 'tolerance_m')
                    assert isinstance(value, float) and isinstance(copy[key], float)
                    assert math.isfinite(value) and math.isfinite(copy[key])
                    packet_metadata_differences.append({
                        'curve_index': i, 'field': key, 'packet_value': value,
                        'embedded_report_value': copy[key],
                        'difference_in_original_ulps': abs(copy[key]-value)/math.ulp(value)})
        assert packet['curve_count'] == 495 and packet['record_words'] == 80
        assert not packet['scalar_majorant'] and n == packet['piece_count']
        piece_bytes = Path(packet_prefix + '-pieces.bin').read_bytes()
        assert uploaded[495*112:].tobytes() == piece_bytes
        pieces = np.frombuffer(piece_bytes, dtype='<u4').reshape(n, 80)
        piece_values = pieces.view('<f4')
        assert outer['binding6_layout'] == {
            'original_curve_records': 495, 'original_words_per_record': 112,
            'piece_base_vec4': 495*28, 'piece_record_vec4': 20, 'pieces': n}
        cursor = 0
        for i, row in enumerate(packet['curves']):
            count = row['pieces']
            assert row['curve_index'] == i and row['first_piece'] == cursor
            assert count in (1, 2, 4, 8, 16, 32, 64)
            for j in range(count):
                words = pieces[cursor+j]; values = piece_values[cursor+j]
                assert words[76:].tolist() == [i, j, count, 0]
                assert values[:3].tolist() == [j/count, (j+1)/count, float(count)]
            cursor += count
        assert cursor == n and len(packet['curves']) == 495
    else:
        assert len(uploaded) == 495*112 and n == 495
    assert Path(prefix + '-field.bin').read_bytes() == Path(capture + '-uniform.bin').read_bytes()
    for kind in ('geometry', 'frames', 'certificates'):
        assert Path(prefix + '-' + kind + '.bin').read_bytes() == Path('artifacts/optical-normal-scene-packet-' + kind + '.bin').read_bytes()
    uniform = np.fromfile(prefix + '-field.bin', dtype='<u4')
    certificates = np.fromfile(prefix + '-certificates.bin', dtype='<u4')
    layer_count = int(certificates[2])
    assert 2 <= layer_count <= 128
    expected = []
    for ray_index, ray in enumerate(captured):
        count, capacity, overflow = map(int, ray[36:39])
        assert capacity == 128 and overflow == 0 and count <= capacity
        for trial_index in range(count):
            trial = ray[40 + 108 * trial_index:40 + 108 * (trial_index + 1)]
            if trial[96] == 2:
                assert trial[49] == 1 and trial[95] == 8
                expected.append((ray_index, trial_index, trial))
    assert len(expected) == 495
    for i, ((ray, trial, values), record) in enumerate(zip(expected, inputs, strict=True)):
        assert np.array_equal(record[:108], values), i
        assert record[110:].tolist() == [ray, trial]
        anchor = values[8:10]
        matches = [j for j in range(layer_count) if np.array_equal(anchor, uniform[20 + 4*j:22 + 4*j])]
        if np.array_equal(anchor, uniform[2:4]):
            assert not matches
            residual = certificates[16:18]
        else:
            assert len(matches) == 1
            residual = certificates[4*(5 + matches[0]):4*(5 + matches[0]) + 2]
        assert np.array_equal(record[108:110], residual)
        meta = fixture['records'][i]
        assert meta['record_index'] == i and [meta['capture_ray'], meta['capture_trial']] == [ray, trial]
        assert meta['anchor_words'] == anchor.tolist() and meta['canonical_anchor_residual_words'] == residual.tolist()
    if cartesian:
        # Every output indexes its original captured interval; the extra
        # packet changes only the bounded position-map evaluation.
        curve_indices = pieces[:, 76]
        inputs = inputs[curve_indices]
        ranges = piece_values[:, :2]
    else:
        curve_indices = np.arange(n)
        ranges = np.tile([0., 1.], (n, 1))
    raw = Path(prefix + '-readback.bin').read_bytes()
    stride = report['diagnostic_bytes_per_record']
    assert stride in (32, 48)
    assert len(raw) == report['output_bytes'] == 16 + (48+stride)*n
    assert report['terminal_bytes'] == 16 + 48*n
    header = np.frombuffer(raw[:16], dtype='<u4')
    assert header.tolist() == report['header'] == report['expected_header'] and report['header_matches']
    assert header[:2].tolist() == report['dimensions'] == [n, 1]
    assert report['dispatch'] == [(n + 31)//32, 1, 1]
    terminal = np.frombuffer(raw[16:16+48*n], dtype='<u4').reshape(n, 12)
    details = np.frombuffer(raw[16+48*n:], dtype='<u4').reshape(n, stride//4)
    tf = terminal.view('<f4'); df = details.copy().view('<f4'); inf = inputs.view('<f4')
    assert np.isfinite(tf[:, :7]).all() and np.isfinite(tf[:, 8:11]).all()
    assert np.isfinite(df[:, 4:8]).all()
    if stride == 48:
        telemetry = report['fixture']['stage_diagnostics']
        assert telemetry['diagnostic_bytes_per_record'] == 48
        valid_codes = {0, *(row['code'] for row in telemetry['stages'])}
        assert set(map(int, details[:,8])) <= valid_codes
        assert set(map(int, details[:,10])) <= {0,1,2,0xffffffff}
        assert not np.any(details[:,11])
        addresses = details[:,9].astype(np.uint64)
        first_face = int(certificates[6])
        assert np.all((addresses == 0xffffffff) | ((addresses >= first_face) &
                      (addresses+6 <= len(certificates)//4) & ((addresses-first_face)%6 == 0)))
    assert set(map(int, terminal[:, 7])) <= {1, 2, 4}
    assert set(map(int, details[:, 0])) <= {0, 1, 2}
    mapping = np.array([4, 1, 2], dtype='<u4')
    assert np.array_equal(mapping[details[:, 0]], terminal[:, 7])
    assert np.array_equal(details[:, 2:4], inputs[:, 110:112])
    assert np.array_equal(details[:, 7], inputs[:, 98])
    assert np.all(details[:, 1] <= 65537)
    assert np.all(df[:, 7] > 0)
    counts = Counter(map(int, terminal[:, 7]))
    assert {str(k): v for k, v in counts.items()} == report['raw_status_counts']
    assert len(report['samples']) == n
    for i, sample in enumerate(report['samples']):
        assert sample['record_index'] == i and sample['pixel'] == [i, 0]
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == details[i].tolist()
    rows = []
    nominal_below = 0
    ceiling = float(certificates.view('<f4')[18])
    for i in range(n):
        event = int(details[i, 0]); theta, lower, upper, tolerance = map(float, df[i, 4:8])
        if event == 1:
            assert float(ranges[i, 0]) <= lower <= theta <= upper <= float(ranges[i, 1])
        else:
            assert theta == lower == upper == 0
        h0 = float(inf[i, 8]) + float(inf[i, 9]) + float(inf[i, 0])
        nominal_min_h = h0 + min(float(inf[i, 3])*float(t) for t in ranges[i])
        nominal_below += nominal_min_h <= ceiling
        rows.append({'record_index': i, 'curve_index': int(curve_indices[i]),
                     'piece_index': int(pieces[i, 77]) if cartesian else 0,
                     'theta_range': list(map(float, ranges[i])),
                     'capture_ray': int(inputs[i, 110]), 'capture_trial': int(inputs[i, 111]),
                     'event_kind': event, 'status': int(terminal[i, 7]), 'scene_visits': int(details[i, 1]),
                     'theta': theta, 'lower': lower, 'upper': upper, 'tolerance_m': tolerance,
                     'local_path_m': float(tf[i, 3]), 'material_owner_word': int(terminal[i, 11]),
                     'nominal_minimum_height_m': nominal_min_h,
                     'terminal_words': terminal[i].tolist(), 'diagnostic_words': details[i,:8].tolist(),
                     'stage_words': details[i,8:].tolist()})
    scope = ('Independent byte/status audit of 495 retained accepted height intervals. '
             'Every 108-word captured curve and appended source residual/IDs is checked. '
             'Status4 is a clear interval; contact paths are local; uncertainty remains explicit. '
             'No integration, continuation ordering, full-ray first-contact, global accuracy, '
             'rendering or performance acceptance is inferred. Nominal ceiling grouping is '
             'descriptive, not the shader sufficient predicate.')
    if cartesian:
        scope += (' Each output is one certified Cartesian piece. All appended packet bytes '
                  'and per-piece identities/domains are checked against the independently '
                  'audited static packet; this output audit does not rerun the packet oracle. '
                  'No later hit is accepted as a first event through an uncertain earlier piece.')
    return {'scope': scope, 'cartesian': cartesian,
            'input_prefix': prefix, 'all_output_hashes_verified': True, 'current_header_verified': True,
            'all_original_capture_words_exact': True, 'canonical_anchor_residuals_exact': True,
            'all_cartesian_packet_bytes_exact': True if cartesian else None,
            'embedded_packet_metadata_float_differences': packet_metadata_differences,
            'diagnostic_bytes_per_record': stride,
            'uncertainty_return_code_counts': dict(Counter(map(int, details[:,8]))) if stride == 48 else None,
            'h0_coverage_result_counts': dict(Counter(map(int, details[:,10]))) if stride == 48 else None,
            'counts': dict(counts), 'event_counts': dict(Counter(map(int, details[:, 0]))),
            'nominal_intervals_at_or_below_ceiling': nominal_below,
            'maximum_scene_visits_per_interval': int(details[:, 1].max()),
            'counts_by_capture_ray': {str(ray): dict(Counter(int(r['status']) for r in rows if r['capture_ray'] == ray))
                                      for ray in range(original['count'])}, 'records': rows}


def compare(candidate, baseline):
    current = review(candidate); previous = review(baseline)
    candidate_report = load_json(candidate+'.json'); baseline_report = load_json(baseline+'.json')
    uniform_loop_change = candidate_report.get('uniform_loops', False) and not baseline_report.get('uniform_loops', False)
    all_inputs_identical = True
    for suffix in ('primary', 'field', 'geometry', 'frames', 'certificates', 'curves'):
        after = Path(candidate+'-'+suffix+'.bin').read_bytes()
        before = Path(baseline+'-'+suffix+'.bin').read_bytes()
        if suffix == 'primary' and uniform_loop_change:
            policy = candidate_report['loop_policy']
            assert policy['uniform_byte_offset'] == 156 and policy['loop_count'] == 2 and policy['iterations'] == 32
            assert len(after) == len(before) == 2240
            assert after[:156] == before[:156] and after[160:] == before[160:]
            assert int.from_bytes(before[156:160], 'little') == policy['previous_word']
            assert int.from_bytes(after[156:160], 'little') == policy['current_word'] == 0x42000000
            all_inputs_identical = all_inputs_identical and after == before
        else:
            assert after == before, suffix
    if uniform_loop_change:
        old = Path(baseline+'.wgsl').read_text(); new = Path(candidate+'.wgsl').read_text()
        literal = 'for(var iteration=0u;iteration<32u;iteration+=1u)'
        uniform = 'for(var iteration=0u;iteration<u32(settings.budget.w);iteration+=1u)'
        assert old.count(literal) == 2 and 'settings.budget' not in old
        assert new == old.replace(literal, uniform)
    a = current['records']; b = previous['records']; assert len(a) == len(b)
    terminal_differences = diagnostic_differences = 0; changed = []
    for after, before in zip(a, b, strict=True):
        assert after['record_index'] == before['record_index']
        td = sum(x != y for x,y in zip(after['terminal_words'], before['terminal_words'], strict=True))
        dd = sum(x != y for x,y in zip(after['diagnostic_words'], before['diagnostic_words'], strict=True))
        terminal_differences += td; diagnostic_differences += dd
        if td or dd:
            changed.append({'record_index': after['record_index'],
                            'before_terminal_words': before['terminal_words'], 'after_terminal_words': after['terminal_words'],
                            'before_diagnostic_words': before['diagnostic_words'], 'after_diagnostic_words': after['diagnostic_words']})
    return {'scope': 'Exact same-upload static scene-piece comparison. Counts/parity do not establish full-ray contacts, accuracy or performance.',
            'candidate': candidate, 'baseline': baseline, 'records': len(a),
            'all_six_input_buffers_identical': all_inputs_identical, 'both_raw_audits_pass': True,
            'only_uniform_loop_word_changed': uniform_loop_change,
            'exact32_loop_source_substitution_verified': uniform_loop_change,
            'candidate_counts': current['counts'], 'baseline_counts': previous['counts'],
            'terminal_words_compared': 12*len(a), 'diagnostic_words_compared': 8*len(a),
            'terminal_word_differences': terminal_differences,
            'diagnostic_word_differences': diagnostic_differences, 'changed_records': changed}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('prefix'); parser.add_argument('--baseline'); args = parser.parse_args()
    result = compare(args.prefix, args.baseline) if args.baseline else review(args.prefix)
    output = Path(args.prefix + ('-baseline-comparison.json' if args.baseline else '-independent-review.json'))
    assert not output.exists(), 'retain previous evidence'
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items()
                      if k not in ('records', 'changed_records', 'embedded_packet_metadata_float_differences')}, indent=2))
