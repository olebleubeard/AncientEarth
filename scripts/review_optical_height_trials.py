"""Independent complete detail and wrapping-checksum review of trial isolation."""
import argparse
from decimal import Decimal
import hashlib
import json
import math
from pathlib import Path
from fractions import Fraction

import numpy as np

from test_optical_height_bounds_fast import true_vertical_interval


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def hash_word(h, word):
    # All products fit u64. Masking explicitly implements WGSL u32 wrapping,
    # without depending on NumPy overflow behavior or float conversion.
    return (((h.astype(np.uint64) ^ word.astype(np.uint64)) * 16777619 + 2166136261)
            & np.uint64(0xffffffff)).astype('<u4')


def hash_records(records):
    result = np.zeros((len(records), 4), dtype='<u4')
    for column in range(records.shape[1]):
        result = hash_word(result, records[:, column])
    return result


def comparison(a, b):
    assert a.shape == b.shape
    differences = np.argwhere(a != b)
    return {'changed_words': len(differences),
            'changed_records': int(np.count_nonzero(np.any((a != b).reshape(len(a), -1), axis=1))),
            'first_changed_word_indices': differences[:64].tolist(),
            'scope': 'Every word is compared; only the first64 locations are summarized. Complete raw arrays retain all differences.'}


def certificate_enclosures(inputs, result):
    """Independent exact encoded-polynomial checks, separate from checksums.

    This checks the reported certificate endpoints and necessary polynomial
    domains. The source-reviewed error estimator/rounding proof is separate;
    neither this nor a checksum establishes complete ray/scene accuracy.
    """
    checked = 0
    for source, row in zip(inputs, result):
        if row[19, 1] != 1:
            continue
        assert row[30, 3] == 8
        # A successful fast certificate and the original fallback both return
        # the exact supplied dense coefficients without alteration.
        assert np.array_equal(row[10:19], row[20:29])
        values = source.copy().view('<f4').astype(float)
        stages = row[20:31].copy().view('<f4').astype(float)
        candidate = row[10:20].copy().view('<f4').astype(float)
        q, u, step = values[0, :3], values[1, :3], float(values[0, 3])
        chart = [[Fraction(float(x)) for x in r[1:3]] for r in stages[:4]]
        direction = [[Fraction(float(x)) for x in r[:3]] for r in stages[4:8]]
        Q = sum(abs(x) for r in chart for x in r)
        U = sum(abs(x) for r in direction for x in r)
        norm_upper = sum(abs(Fraction(float(x))) for x in u)
        lower_v = true_vertical_interval(q, u, 1 if step > 0 else -1)[0]-U-norm_upper*Q
        reported_v = Fraction(float(candidate[9, 2]))
        assert 0 < reported_v <= lower_v
        reported_path = Fraction(float(candidate[9, 3]))
        path = [Fraction(float(r[0])) for r in stages[:4]]
        assert reported_path >= sum(map(abs, path))
        assert reported_path >= Fraction(abs(step))/reported_v
        chart_extra = sum(map(abs, chart[3]))/16 + sum(abs(Fraction(float(x))) for x in stages[9, 1:3])
        assert sum(abs(Fraction(float(x))) for x in q[1:])+Q+chart_extra <= 1
        direction_extra = sum(map(abs, direction[3]))/16 + sum(abs(Fraction(float(x))) for x in stages[10, :3])
        assert max(abs(Fraction(float(x))) for x in u)-U-direction_extra > 0
        p2, p3, p4, p5 = path
        c1, c2, c3, c4 = p2+p3, p4-p3+p5, -p4-2*p5, p5
        bernstein = [c1, c1+Fraction(2,3)*c2,
                     c1+Fraction(4,3)*c2+c3, c1+2*c2+3*c3+4*c4]
        assert min(bernstein) > 0
        checked += 1
    return {'valid_polynomial_records_checked': checked,
            'scope': 'Exact encoded chart/direction variation, positive norm/chart domains, reported vertical lower and path upper endpoints, and independently expanded derivative Bernstein signs. Source-reviewed estimator arithmetic and vendor evaluation allowances remain separate; no ray/contact acceptance is inferred.'}


def details(prefix, capture, inner, repetitions):
    report = json.loads(Path(prefix + '.json').read_text())
    fast = report.get('fast_bounds', False)
    assert isinstance(fast, bool)
    assert report['fixture'].get('upper_only_height_bounds', False) == fast
    for item in json.loads(Path(prefix + '-outputs.json').read_text()):
        assert sha(item['path']) == item['sha256']
    assert json.loads(Path(prefix + '-input-stability.json').read_text())['inputs_unchanged_through_run']
    assert report['fixture']['inner'] == inner and report['fixture']['repetitions'] == repetitions
    assert report['fixture']['capture_prefix'] == capture
    inputs = np.fromfile(prefix + '-inputs.bin', dtype='<u4').reshape((-1, 27, 4))
    result = np.fromfile(prefix + '-detail.bin', dtype='<u4').reshape((-1, 41, 4))
    assert len(inputs) == len(result) > 0
    assert report['count'] == len(inputs) and report['detail_record_words'] == 164
    assert report['cost_invocations'] == len(inputs) * repetitions
    capture_report = json.loads(Path(capture + '.json').read_text())
    captured = np.fromfile(capture + '-readback.bin', dtype='<u4').reshape((-1, capture_report['record_words']))
    selected = []
    for record in captured:
        count = int(record[36])
        assert record[37] == 128 and record[38] == 0 and count <= 128
        selected.extend(record[40:40+108*count].reshape((-1, 27, 4)))
    assert np.array_equal(inputs, np.asarray(selected))
    assert Path(prefix + '-uniform.bin').read_bytes() == Path(capture + '-uniform.bin').read_bytes()
    pairs = {'original_vs_capture': (result[:, :10], inputs[:, 3:13]),
             'recombined_vs_original': (result[:, 10:20], result[:, :10]),
             'stages_vs_capture': (result[:, 20:31], inputs[:, 13:24]),
             'captured_bounds_vs_original': (result[:, 31:41], result[:, :10])}
    comparisons = {name: comparison(a, b) for name, (a, b) in pairs.items()}
    # Separate encoded flags from numerical payload and quantify the observed
    # change of compiler context. These are coefficient/bound differences,
    # not trajectory endpoint errors.
    replay = result[:, :10].copy().view('<f4').astype(float)
    retained = inputs[:, 3:13].copy().view('<f4').astype(float)
    original_delta = np.abs(replay-retained)
    stage_delta = np.abs(result[:, 20:31].copy().view('<f4').astype(float)
                         - inputs[:, 13:24].copy().view('<f4').astype(float))
    local_tolerance = inputs[:, 24, 2].copy().view('<f4').astype(float)
    path_limit = inputs[:, 24, 3].copy().view('<f4').astype(float)
    context = {
        'returned_valid_flag_changes': int(np.count_nonzero(result[:, 9, 1] != inputs[:, 12, 1])),
        'stage_index_changes': int(np.count_nonzero(result[:, 30, 3] != inputs[:, 23, 3])),
        'maximum_chart_dense_coefficient_difference': float(original_delta[:, :4, 1:3].max()),
        'maximum_path_dense_coefficient_difference_m': float(original_delta[:, :4, 0].max()),
        'maximum_direction_dense_coefficient_difference': float(original_delta[:, 4:8, :3].max()),
        'maximum_optical_excess_dense_coefficient_difference_m': float(original_delta[:, 8].max()),
        'maximum_embedded_chart_difference': float(stage_delta[:, 9, 1:3].max()),
        'maximum_error_estimate_difference_m': float(original_delta[:, 9, 0].max()),
        'maximum_vertical_lower_difference': float(original_delta[:, 9, 2].max()),
        'maximum_path_upper_difference_m': float(original_delta[:, 9, 3].max()),
        'local_error_predicate_changes': int(np.count_nonzero((replay[:, 9, 0] < local_tolerance) != (retained[:, 9, 0] < local_tolerance))),
        'path_limit_predicate_changes': int(np.count_nonzero((replay[:, 9, 3] <= path_limit) != (retained[:, 9, 3] <= path_limit))),
        'scope': 'Finite captured-vs-isolated compiler-context effects only. Two necessary controller comparisons are checked; complete controller acceptance is not replayed.'}
    checksums = {}
    cost_records = [('full', result[:, 10:20] if fast else result[:, :10]),
                    ('stages', result[:, 20:31]), ('bounds', result[:, 31:41])]
    if fast:
        cost_records.append(('reference', result[:, :10]))
    for name, records in cost_records:
        per_input = hash_records(records)
        expected = np.zeros((len(inputs), 4), dtype='<u4')
        for iteration in range(inner):
            expected = hash_word(expected, per_input[(np.arange(len(inputs)) + 17*iteration) % len(inputs)])
        actual = np.fromfile(prefix + '-cost-' + name + '.bin', dtype='<u4').reshape((-1, 4))
        assert len(actual) == len(inputs) * repetitions
        expected = np.tile(expected, (repetitions, 1))
        checksums[name] = comparison(actual, expected)
        checksums[name]['raw_sha256'] = sha(prefix + '-cost-' + name + '.bin')
    timing = {}
    previous_end = 0
    expected_entries = ['trial_detail','cost_full','cost_stages','cost_bounds']
    if fast:
        expected_entries.append('cost_reference')
    assert [entry['entry'] for entry in report['entries']] == expected_entries
    for entry in report['entries']:
        if entry['entry'] == 'trial_detail':
            assert entry['iterations'] == []
            assert entry['output_bytes'] == len(inputs)*164*4
            continue
        assert entry['output_bytes'] == len(inputs)*repetitions*16
        assert len(entry['iterations']) == 8
        kept = []
        for index, row in enumerate(entry['iterations']):
            begin, end = row['raw_timestamps']
            assert previous_end < begin < end
            previous_end = end
            assert row['iteration'] == index and row['warmup'] == (index < 3)
            assert math.isfinite(row['timestamp_period_ns']) and row['timestamp_period_ns'] > 0
            seconds = Decimal(end-begin) * Decimal(str(row['timestamp_period_ns'])) * Decimal('1e-9')
            assert abs(float(seconds)-row['gpu_seconds']) <= 2*math.ulp(row['gpu_seconds'])
            if not row['warmup']:
                kept.append(seconds*1000)
        timing[entry['entry']] = {'kept_gpu_milliseconds': list(map(str, kept)),
                                 'median_gpu_milliseconds': str(sorted(kept)[2])}
    fast_effect = None
    if fast:
        candidate = result[:, 10:20].copy().view('<f4').astype(float)
        reference = result[:, :10].copy().view('<f4').astype(float)
        delta = np.abs(candidate-reference)
        fast_effect = {
            'encoded_polynomial_enclosures': certificate_enclosures(inputs, result),
            'candidate_vs_reference_words': comparison(result[:, 10:20], result[:, :10]),
            'candidate_vs_reference_coefficient_words': comparison(result[:, 10:19], result[:, :9]),
            'valid_flag_changes': int(np.count_nonzero(result[:, 19, 1] != result[:, 9, 1])),
            'candidate_valid_count': int(np.count_nonzero(result[:, 19, 1] == 1)),
            'reference_valid_count': int(np.count_nonzero(result[:, 9, 1] == 1)),
            'maximum_error_estimate_difference_m': float(delta[:, 9, 0].max()),
            'maximum_vertical_lower_difference': float(delta[:, 9, 2].max()),
            'maximum_path_upper_difference_m': float(delta[:, 9, 3].max()),
            'local_error_predicate_changes': int(np.count_nonzero((candidate[:, 9, 0] < local_tolerance) != (reference[:, 9, 0] < local_tolerance))),
            'path_limit_predicate_changes': int(np.count_nonzero((candidate[:, 9, 3] <= path_limit) != (reference[:, 9, 3] <= path_limit))),
            'scope': 'The candidate certificate may return different conservative bounds. Numerical equality is recorded, not required. Finite enclosure proof and independent ray references remain separate requirements; these two predicates are not a replay of the complete controller.'}
    return {'scope': 'All original/captured, extracted/recombined and retained-coefficient detail words are compared. Timed-kernel checksum outputs are independently reconstructed using exact wrapping integer arithmetic on corresponding detail records and the identical cyclic input schedule. In fast mode full uses candidate detail10:20 and reference uses original0:10. Checksums are additional compiler-context evidence, not collision-free numerical equality proofs; full detail arrays are separately compared. Component timings exclude controller endpoint-charge work and are not additive or native frame costs.',
            'all_output_hashes_verified': True, 'report_sha256': sha(prefix + '.json'),
            'capture': capture, 'capture_raw_sha256': sha(capture + '-readback.bin'),
            'raw_input_sha256': sha(prefix + '-inputs.bin'), 'raw_detail_sha256': sha(prefix + '-detail.bin'),
            'trial_count': len(inputs), 'inner': inner, 'repetitions': repetitions, 'fast_bounds': fast,
            'trial_evaluations_per_dispatch': len(inputs)*inner*repetitions,
            'detail_comparisons': comparisons, 'checksum_comparisons': checksums,
            'capture_to_isolated_context': context, 'fast_certificate_effect': fast_effect,
            'component_timings': timing, 'positive_strictly_separated_timestamp_pairs': 8*len(cost_records),
            'all_detail_words_equal': all(row['changed_words'] == 0 for row in comparisons.values()),
            'all_cost_checksums_equal_detail': all(row['changed_words'] == 0 for row in checksums.values())}


def self_check():
    # Separately expressed Python arbitrary-integer arithmetic checks every
    # lane, integer overflow and the fixed zero origin of both hash levels.
    records = np.array([[[0, 1, 0xffffffff, 0x80000000], [3, 7, 11, 13]],
                        [[0x12345678, 0xabcdef01, 2, 5], [17, 19, 23, 29]]], dtype='<u4')
    expected = []
    for record in records.tolist():
        h = [0]*4
        for row in record:
            h = [((a ^ b)*16777619 + 2166136261) % 2**32 for a, b in zip(h, row)]
        expected.append(h)
    assert hash_records(records).tolist() == expected


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('capture')
    parser.add_argument('--inner', type=int, default=8)
    parser.add_argument('--repetitions', type=int, default=256)
    args = parser.parse_args()
    assert 1 <= args.inner <= 64 and 1 <= args.repetitions <= 1024
    self_check()
    output = details(args.prefix, args.capture, args.inner, args.repetitions)
    Path(args.prefix + '-independent-review.json').write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps(output, indent=2))
