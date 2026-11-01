"""Audit captured actual height trials, including rejected stages and bounds."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

import numpy as np


def review(prefix, checkpoint):
    data = json.loads(Path(prefix + '.json').read_text())
    for item in json.loads(Path(prefix + '-outputs.json').read_text()):
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256']
    assert json.loads(Path(prefix + '-input-stability.json').read_text())['inputs_unchanged_through_run']
    for suffix in ['-uniform.bin', '-rays.bin']:
        assert Path(prefix + suffix).read_bytes() == Path(checkpoint + suffix).read_bytes()
    words = np.fromfile(prefix + '-readback.bin', dtype='<u4').reshape((-1, data['record_words']))
    original = np.fromfile(checkpoint + '-readback.bin', dtype='<u4').reshape((-1, 36))
    assert len(words) == len(original)
    assert words.tolist() == data['gpu_raw_records']
    layout = data['fixture']['trial_capture']
    capacity, vectors = layout['capacity'], layout['vectors_per_trial']
    assert vectors == 27 and layout['first_trial_word'] == 40 and layout['header_word'] == 36
    assert data['record_words'] == 40 + capacity * vectors * 4
    stage_counts, returned_counts, controller_counts = Counter(), Counter(), Counter()
    records, flattened = [], []
    for ray_index, raw in enumerate(words):
        count, declared_capacity, truncated, reserved = map(int, raw[36:40])
        assert declared_capacity == capacity and reserved == 0
        assert truncated == int(count > capacity)
        assert not truncated, 'Incomplete captures cannot supply the claimed complete trial workload'
        trials = raw[40:40+count*vectors*4].reshape((-1, vectors, 4))
        for trial_index, trial in enumerate(trials):
            f = trial.copy().view('<f4')
            stage = int(trial[23, 3]); valid = int(trial[12, 1]); kind = int(trial[24, 0])
            assert stage in range(9) and valid in (0, 1) and kind in (0, 1, 2)
            assert np.isfinite(f[:3]).all() and np.isfinite(f[25:]).all()
            assert trial[1, 3] == trial[25, 3] == trial[26, 2] == trial[26, 3] == 0
            assert f[0, 3] != 0
            for row in list(range(3, 11)) + list(range(13, 21)):
                assert trial[row, 3] == 0
            if stage < 8:
                assert not np.any(trial[13:23]) and not np.any(trial[23, :3])
                assert valid == 0
            else:
                assert np.isfinite(f[13:23]).all() and np.isfinite(f[23, :3]).all()
            if not valid:
                assert not np.any(trial[3:12])
                assert f[12, 0] == np.float32(3e37) and not np.any(trial[12, 2:])
            else:
                assert stage == 8 and np.isfinite(f[3:12]).all()
                assert f[12, 0] >= 0 and f[12, 2] > .01 and f[12, 3] > 0
            if kind == 2:
                assert valid == 1
                assert float(f[12, 0]) + float(f[24, 1]) <= float(f[24, 2])
            stage_counts[stage] += 1; returned_counts[valid] += 1; controller_counts[kind] += 1
            flattened.append(trial.reshape(-1).tolist())
        records.append({'ray_index': ray_index, 'trial_count': count,
                        'terminal_changed_words': int(np.count_nonzero(raw[:36] != original[ray_index])),
                        'status': int(raw[7]), 'attempts': int(raw[8])})
    return dict(scope='Actual seven-ray nh_trial invocations, including early stage, finite-coefficient, bound and controller rejection. Pre-bounds zeros at stage index<8 mean unavailable/reset data, not zero computed coefficients. First36 parity is reported explicitly; capture arithmetic/context is not assumed identical. No timing, complete-frame workload or runtime acceptance claim.',
                report_sha256=hashlib.sha256(Path(prefix + '.json').read_bytes()).hexdigest(),
                all_output_hashes_verified=True, checkpoint=checkpoint,
                terminal_changed_words=sum(row['terminal_changed_words'] for row in records),
                rays=records, captured_trials=len(flattened), stage_counts=dict(stage_counts),
                returned_valid_counts=dict(returned_counts), controller_kind_counts=dict(controller_counts),
                raw_trial_record_words=vectors*4,
                raw_trials=flattened)


if __name__ == '__main__':
    prefix, checkpoint = sys.argv[1:3]
    result = review(prefix, checkpoint)
    Path(prefix + '-independent-review.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'raw_trials'}, indent=2))
