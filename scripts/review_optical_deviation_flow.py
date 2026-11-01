"""Retained common-schedule flow rejection/cost screen; no sharing admission."""
import argparse
from collections import Counter
from decimal import Decimal
import math
from pathlib import Path
import json

from review_optical_deviation_stencil import number, read, sha


def interval(value):
    assert len(value) == 2 and all(math.isfinite(x) for x in value)
    assert value[0] <= value[1]


def review(prefix):
    data = read(prefix + '.json')
    assert data['schema'] == 'ancientearth.deviation-flow-bounds.v1'
    for item in read(prefix + '-outputs.json'):
        assert sha(item['path']) == item['sha256']
    sources = read(prefix + '-sources.json')
    stability = read(prefix + '-input-stability.json')
    before = {p['path']: p['sha256'] for p in sources['inputs']}
    after = {p['path']: p['sha256'] for p in stability['after_run']}
    assert stability['inputs_unchanged_through_run'] and before == after
    lifecycle = read(prefix + '-lifecycle.json')
    assert lifecycle['exit_code'] == 0 and not lifecycle['timed_out']
    assert not lifecycle['terminated_by_wrapper'] and lifecycle['failure'] is None
    original = read(data['input'])
    assert data['input_bits'] == original['input_bits']
    bits = data['input_bits']
    eye = number(bits['height'])
    knots = [(number(h), number(n)) for h, n in bits['knots']]
    assert len(knots) == 45 and all(a[0] < b[0] for a, b in zip(knots, knots[1:]))
    heights = [h for h, _ in knots if h > eye]
    assert len(heights) == 44
    expected_cells = [c['cell'] for c in original['cases'] if c['cell'][2] <= 8]
    assert [c['cell'] for c in data['cases']] == expected_cells
    assert len(expected_cells) == 24 and data['certified_admitted_cells'] == 0
    rows = []
    total_states = 0
    for case in data['cases']:
        x, y, width, height = case['cell']
        assert width == height and width in (4, 8)
        assert 0 <= x <= 1920-width and 0 <= y <= 1080-height
        assert case['certified_admission'] is False
        assert len(case['raw_launch_affine_bits']) == 3
        for axis in case['raw_launch_affine_bits']:
            assert len(axis) == 3
            list(map(number, axis))
        for name in ('attempts', 'accepted', 'domain_rejections', 'estimator_rejections'):
            assert isinstance(case[name], int) and case[name] >= 0
        assert case['attempts'] <= 2048
        assert case['attempts'] == sum(case[k] for k in ('accepted', 'domain_rejections', 'estimator_rejections'))
        assert math.isfinite(case['cpu_seconds']) and case['cpu_seconds'] >= 0
        for name in ('max_accepted_center_estimator_m', 'max_accepted_whole_estimator_m'):
            assert math.isfinite(case[name]) and case[name] >= 0
        assert case['max_accepted_center_estimator_m'] <= 1e-5
        states = case['source_states']
        assert [s['height'] for s in states] == heights[:len(states)]
        for state in states:
            assert len(state['center_states']) == 10
            for value in state['center_states']:
                interval(value)
            bounds = state['correction_bounds_xyz_du_path_excess']
            assert len(bounds) == 8
            for bound in bounds:
                interval(bound['value'])
                assert all(math.isfinite(bound[k]) and bound[k] >= 0 for k in ('gradient', 'bilinear_error'))
        total_states += len(states)
        if case['exit']:
            assert case['reason'] == 'source top' and len(states) == 44
            assert case['last_height'] == knots[-1][0]
            assert case['max_accepted_whole_estimator_m'] > 1e-5
        else:
            assert case['reason'] == 'whole-cell initial conditioning'
            assert case['last_height'] == eye and not states and case['attempts'] == 0
            assert case['max_accepted_center_estimator_m'] == case['max_accepted_whole_estimator_m'] == 0
        rows.append({'cell': case['cell'], 'exit': case['exit'], 'reason': case['reason'],
                     'attempts': case['attempts'], 'accepted': case['accepted'],
                     'source_records': len(states), 'single_pass_cpu_seconds': case['cpu_seconds'],
                     'center_estimator_max_m': case['max_accepted_center_estimator_m'] if case['accepted'] else None,
                     'whole_estimator_max_m': case['max_accepted_whole_estimator_m'] if case['accepted'] else None,
                     'unmeasured_raw_zero_estimates': not bool(case['accepted'])})
    groups = []
    for size in (4, 8):
        cases = [c for c in data['cases'] if c['cell'][2] == size]
        completed = [c for c in cases if c['exit']]
        assert len(cases) == 12 and len(completed) == 10
        groups.append({'size': size, 'cases': len(cases), 'source_top_screens': len(completed),
                       'conditioning_refusals': len(cases)-len(completed),
                       'completed_screen_cpu_ms_min': min(c['cpu_seconds'] for c in completed)*1000,
                       'completed_screen_cpu_ms_max': max(c['cpu_seconds'] for c in completed)*1000,
                       'all_screen_cpu_seconds_sum': str(sum((Decimal(str(c['cpu_seconds'])) for c in cases), Decimal(0))),
                       'whole_estimator_maxima_range_m': [min(c['max_accepted_whole_estimator_m'] for c in completed),
                                                         max(c['max_accepted_whole_estimator_m'] for c in completed)]})
    return {'scope': __doc__, 'report_sha256': sha(prefix+'.json'),
            'frozen_input_and_output_hashes_verified': True,
            'current_input_paths_changed_since_run': [p for p, h in before.items() if not Path(p).exists() or sha(p) != h],
            'input_bits_and_cell_selection_exact': True, 'source_records_checked': total_states,
            'reason_counts': dict(Counter(c['reason'] for c in data['cases'])),
            'certified_admitted_cells': 0, 'decision': 'Reject this flow screen for sharing/GPU-port development.',
            'sizes': groups, 'cases': rows,
            'limitations': 'This audit checks the retained source/frozen manifests, input bits, endpoint coverage, '
                           'finite interval/majorant payloads, accounting and single-pass CPU costs. It does not '
                           'independently replay the RK maps or prove the reported derivative bounds. Center-only '
                           'estimator selection is not a whole-cell accuracy gate; successful source-top screens '
                           'still have unacceptable whole-cell estimates. Refusal zero fields are unmeasured. '
                           'Costs exclude corner solves and consumer work. No ODE-global, encoded-f32, scene, '
                           'GPU-speed or native-flight acceptance follows.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix')
    parser.add_argument('--output')
    args = parser.parse_args()
    result = review(args.prefix)
    output = args.output or args.prefix+'-independent-review.json'
    assert not Path(output).exists(), output
    Path(output).write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    print(json.dumps({k: result[k] for k in ('source_records_checked', 'reason_counts', 'sizes', 'decision')}, indent=2))
