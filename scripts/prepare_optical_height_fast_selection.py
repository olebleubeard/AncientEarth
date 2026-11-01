"""Retain actual full-frame words for the six conditioning and three delta rays."""
import hashlib
import json
from pathlib import Path
import struct


def build():
    original = json.loads(Path('artifacts/optical-height-performance-first-tail-selection.json').read_text())
    samples = [(row['label'], row['pixel']) for row in original['samples']]
    samples.extend([
        ('maximum_endpoint_delta', [378, 957]),
        ('maximum_direction_delta', [1619, 822]),
        ('maximum_path_delta', [925, 932]),
    ])
    assert len(samples) == 9 and len({tuple(pixel) for _, pixel in samples}) == 9
    candidate = 'artifacts/optical-height-performance-fast-bounds'
    baseline = 'artifacts/optical-height-performance-conditioning'
    raw = {name: Path(prefix + '-readback.bin').read_bytes()
           for name, prefix in [('candidate', candidate), ('baseline', baseline)]}
    assert all(len(data) == 1920 * 1080 * 48 for data in raw.values())
    for suffix in ['-uniform.bin', '-view.bin']:
        assert Path(candidate + suffix).read_bytes() == Path(baseline + suffix).read_bytes()
    records = []
    for label, pixel in samples:
        index = pixel[1] * 1920 + pixel[0]
        assert 0 <= pixel[0] < 1920 and 0 <= pixel[1] < 1080
        words = {name: list(struct.unpack_from('<12I', data, 48 * index))
                 for name, data in raw.items()}
        records.append({'label': label, 'pixel': pixel,
                        'raw_terminal_words': words['candidate'],
                        'baseline_terminal_words': words['baseline']})
    return {
        'scope': 'Exact original camera launch required. Six prior conditioning cases plus three worst finite output deltas, selected for independent reference checks. Differences alone are not accuracy evidence.',
        'candidate': candidate, 'baseline': baseline,
        'source_raw_sha256': {name: hashlib.sha256(data).hexdigest() for name, data in raw.items()},
        'dimensions': [1920, 1080], 'samples': records,
    }


if __name__ == '__main__':
    destination = Path('artifacts/optical-height-performance-fast-bounds-selection.json')
    if destination.exists():
        raise SystemExit(f'Refusing to overwrite {destination}')
    destination.write_text(json.dumps(build(), indent=2) + '\n')
    print(destination)
