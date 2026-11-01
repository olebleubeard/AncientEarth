"""Audit selected native-camera height-controller records without timing claims."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

import numpy as np


def review(prefix, full_prefix):
    data = json.loads(Path(prefix + '.json').read_text())
    assert data['selected'] and data['record_words'] == 36
    for item in json.loads(Path(prefix + '-outputs.json').read_text()):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256']
    assert json.loads(Path(prefix + '-input-stability.json').read_text())['inputs_unchanged_through_run']
    words = np.fromfile(prefix + '-readback.bin', dtype='<u4').reshape((-1, 36))
    assert len(words) == data['record_count'] and words.nbytes == data['terminal_bytes']
    baseline = np.fromfile(full_prefix + '-readback.bin', dtype='<u4').reshape((-1, 12))
    assert len(baseline) == data['dimensions'][0] * data['dimensions'][1]
    for suffix in ['-uniform.bin', '-view.bin']:
        assert Path(prefix + suffix).read_bytes() == Path(full_prefix + suffix).read_bytes()
    rows = []
    for index, sample in enumerate(data['samples']):
        assert sample['record_index'] == index
        assert words[index].tolist() == sample['raw_terminal_words']
        x, y = sample['pixel']
        old = baseline[y * data['dimensions'][0] + x]
        f = words[index].copy().view('<f4')
        assert np.isfinite(f[:7]).all() and np.isfinite(f[12:]).all()
        row = {
            'pixel': [x, y], 'status': int(words[index, 7]),
            'attempts': int(words[index, 8]), 'source_root_calls': int(words[index, 9]),
            'reason': int(words[index, 10]), 'path_m': float(f[3]),
            'baseline_status': int(old[7]),
            'terminal_changed_words': int(np.count_nonzero(words[index, :12] != old)),
            'launch_local': f[12:15].tolist(),
            'offset_m': float(f[16]), 'local_tolerance_m': float(f[18]),
            'owned_source_index': float(f[19]), 'chart_ab': f[20:22].tolist(),
            'height_proposal_m': float(f[22]), 'height_attempts': float(f[23]),
            'last_height_error_interval_m': f[24:26].tolist(),
            'last_metres_per_height': float(f[26]), 'last_charge_m': float(f[27]),
            'last_charge_over_local_tolerance': float(f[27]) / float(f[18]),
            'last_vertical': float(f[28]), 'last_attempted_height_m': float(f[29]),
            'last_height_result_kind': float(f[30]), 'height_accepts': float(f[31]),
            'last_curve_error_m': float(f[32]), 'source_anchor_parts_m': f[33:35].tolist(),
            'raw_words': words[index].tolist(),
            'diagnostic_slots': [f[i:i+4].tolist() for i in range(16, 36, 4)],
        }
        # Both parameterizations reuse these diagnostic globals. The path
        # branch overwrites their meanings; do not interpret its residuals
        # as chart coordinates or its root allocation as an enum/counter.
        height_layout = (float(f[23]).is_integer() and f[23] >= 0
                         and float(f[30]) in (0., 1., 2.)
                         and float(f[31]).is_integer() and f[31] >= 0
                         and max(abs(f[20]), abs(f[21])) <= 1.)
        row['height_telemetry_layout_consistent'] = bool(height_layout)
        if not height_layout:
            for key in ['chart_ab', 'height_proposal_m', 'height_attempts',
                        'last_height_error_interval_m', 'last_metres_per_height',
                        'last_charge_m', 'last_charge_over_local_tolerance',
                        'last_vertical', 'last_attempted_height_m',
                        'last_height_result_kind', 'height_accepts']:
                del row[key]
        rows.append(row)
    assert len(rows) == len(words)
    assert {str(k): v for k, v in Counter(row['status'] for row in rows).items()} == data['status_counts']
    return {
        'scope': 'Selected exact-camera diagnostic only. Every retained output hash and raw record is checked. Terminal comparison is against the named full-frame baseline on identical uniform/camera bytes; changed numerical methods may differ. Height telemetry layout consistency is only a heuristic shape check, not a shader-authored branch tag; raw shared slots remain authoritative. Last-trial telemetry is not a trajectory-wide error certificate or timing result.',
        'report_sha256': hashlib.sha256(Path(prefix + '.json').read_bytes()).hexdigest(),
        'baseline': full_prefix,
        'baseline_raw_sha256': hashlib.sha256(Path(full_prefix + '-readback.bin').read_bytes()).hexdigest(),
        'all_output_hashes_verified': True,
        'record_count': len(rows),
        'terminal_changed_words': sum(row['terminal_changed_words'] for row in rows),
        'status_counts': dict(Counter(row['status'] for row in rows)), 'records': rows,
    }


if __name__ == '__main__':
    result = review(sys.argv[1], sys.argv[2])
    Path(sys.argv[1] + '-independent-review.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'records'}, indent=2))
    for row in result['records']:
        print(json.dumps({k: v for k, v in row.items() if k != 'raw_words'}))
