"""Audit retained atmosphere-only timing and every terminal record."""
from collections import Counter
from decimal import Decimal
import hashlib
import json
import math
from pathlib import Path
import re
import sys

import numpy as np


def summary(values):
    ordered = np.sort(values)
    return {name: int(ordered[max(0, math.ceil(q * len(ordered)) - 1)])
            for name, q in [('min', 0), ('p50', .5), ('p95', .95), ('p99', .99), ('max', 1)]}


def review(prefix, comparison=None):
    path = Path(prefix)
    data = json.loads(path.with_suffix('.json').read_text())
    for item in json.loads(Path(str(path) + '-outputs.json').read_text()):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256'], item['path']
    assert json.loads(Path(str(path) + '-input-stability.json').read_text())['inputs_unchanged_through_run']
    words = np.fromfile(Path(str(path) + '-readback.bin'), dtype='<u4').reshape((-1, 12))
    assert len(words) == math.prod(data['dimensions'])
    shader = Path(str(path) + '.wgsl').read_text(encoding='utf-8')
    entry_pattern = r'@compute\s+@workgroup_size\(\s*(\d+)\s*\)\s+fn normal_performance\b'
    entry_sizes = re.findall(entry_pattern, shader)
    assert len(entry_sizes) == 1
    actual_group_size = int(entry_sizes[0])
    assert actual_group_size == data.get('group_size', data['workgroup_size'])
    assert actual_group_size in (32, 64, 128, 256)
    if 'dispatch' in data:
        assert data['dispatch'] == [(len(words)+actual_group_size-1)//actual_group_size, 1, 1]
    assert words.nbytes == data['terminal_bytes'] == len(words) * 48
    statuses = Counter(map(int, words[:, 7]))
    assert {str(k): v for k, v in statuses.items()} == data['status_counts']
    for sample in data['samples']:
        x, y = sample['pixel']
        assert words[y * data['dimensions'][0] + x].tolist() == sample['raw_terminal_words']
    assert np.isfinite(words[:, :7].copy().view('<f4')).all()
    previous_end = 0
    kept = []
    assert len(data['iterations']) == 8
    for index, row in enumerate(data['iterations']):
        begin, end = row['raw_timestamps']
        assert previous_end < begin < end
        assert row['iteration'] == index and row['warmup'] == (index < 3)
        previous_end = end
        seconds = Decimal(end - begin) * Decimal(str(row['timestamp_period_ns'])) * Decimal('1e-9')
        assert abs(float(seconds) - row['gpu_seconds']) <= 2 * math.ulp(row['gpu_seconds'])
        if not row['warmup']:
            kept.append(seconds)
    assert len(kept) == 5
    distributions = {}
    for status in sorted(statuses):
        selected = words[words[:, 7] == status]
        distributions[str(status)] = {'count': len(selected),
            'attempts': summary(selected[:, 8]), 'root_calls': summary(selected[:, 9]),
            'reason_counts': dict(Counter(map(int, selected[:, 10]))),
            'attempt_budget_exhaustions': int(np.count_nonzero(selected[:, 8] == 4096))}
    result = {'scope': 'All terminal words and eight timestamp pairs independently checked. Timed program omits terrain, h0, vacuum entry, radiance, stars, UI and presentation. Status 0 means atmospheric exit in this isolated program, not visible sky. No application FPS, scene-equivalence or monotone-cost lower-bound claim.',
            'input_report_sha256': hashlib.sha256(path.with_suffix('.json').read_bytes()).hexdigest(),
            'all_output_hashes_verified': True, 'pixel_count': len(words),
            'kept_gpu_milliseconds': [str(x * 1000) for x in kept],
            'median_gpu_milliseconds': str(sorted(kept)[2] * 1000),
            'execution_layout': {
                'shader_entry_workgroup_size': actual_group_size,
                'explicit_group_size': data.get('group_size'),
                'descriptive_workgroup_size': data['workgroup_size'],
                'descriptive_field_matches_entry': data['workgroup_size'] == actual_group_size,
                'scope': 'Actual assembled entry, explicit group_size when present, and dispatch are checked. Earlier sweep reports retain a stale descriptive workgroup_size=32 field; it is not used to infer the launch.'},
            'status_distributions': distributions,
            'selected_samples_raw_equal': len(data['samples'])}
    if comparison:
        for suffix in ['-uniform.bin', '-view.bin']:
            assert Path(prefix + suffix).read_bytes() == Path(comparison + suffix).read_bytes(), suffix
        baseline = np.fromfile(comparison + '-readback.bin', dtype='<u4').reshape((-1, 12))
        assert baseline.shape == words.shape
        baseline_shader = Path(comparison + '.wgsl').read_text(encoding='utf-8')
        assert len(re.findall(entry_pattern, baseline_shader)) == 1
        strip_entry = lambda text: re.sub(entry_pattern, '@compute @workgroup_size(GROUP) fn normal_performance', text)
        unequal = words != baseline
        changed = np.flatnonzero(np.any(unequal, axis=1))
        result['same_input_terminal_comparison'] = {
            'baseline': comparison,
            'baseline_raw_sha256': hashlib.sha256(Path(comparison + '-readback.bin').read_bytes()).hexdigest(),
            'changed_words': int(np.count_nonzero(unequal)),
            'changed_pixels': len(changed),
            'changed_statuses': int(np.count_nonzero(unequal[:, 7])),
            'changed_reasons': int(np.count_nonzero(unequal[:, 10])),
            'shader_equal_except_entry_workgroup_size': strip_entry(shader) == strip_entry(baseline_shader),
            'first_changed_pixel_indices': changed[:64].tolist(),
            'scope': 'Every terminal word compared on exactly identical uniform and camera bytes. Full raw buffers retain all differences; no physical oracle is inferred from parity.'}
    return result


if __name__ == '__main__':
    prefix = sys.argv[1]
    result = review(prefix, sys.argv[2] if len(sys.argv) > 2 else None)
    Path(prefix + '-independent-review.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
