"""Exact retained curve extraction for the CPU anchor feasibility screen.

The 34 capture records are accepted height trials, not full scene solutions.
The 23 selected records are final pending curves after uncertainty retries;
their last-piece buffer is deliberately not read. No earlier selected trial
history is reconstructed. Geometry/anchor preparation belongs to the caller.
"""
from fractions import Fraction
import hashlib
import json
from pathlib import Path

import numpy as np

from test_optical_normal_residual import add, exact, mul, sub

ROOT = Path(__file__).resolve().parents[1]
CAPTURE = 'artifacts/optical-height-trial-capture'
SELECTED = 'artifacts/optical-normal-selected-split-gpu-first'
SCENE = 'artifacts/optical-normal-scene-packet'
INPUT_PATHS = (
    CAPTURE + '-readback.bin', CAPTURE + '-uniform.bin',
    SELECTED + '.json', SELECTED + '-states.bin', SELECTED + '-field.bin',
    SELECTED + '-certificates.bin', SCENE + '-certificates.bin',
)
SOURCE_PATHS = (
    'scripts/optical_patch_feasibility_inputs.py',
    'scripts/test_optical_normal_residual.py',
    'scripts/test_optical_normal_intervals.py',
    'scripts/optical_height_trial_capture.rs',
    'scripts/optical_normal_selected_state.wgsl',
    'scripts/optical_normal_scene_curve.wgsl',
    'scripts/optical_normal_selected_control.wgsl',
)


def input_inventory():
    """Runtime inputs and mathematical layout/arithmetic sources, by SHA256."""
    return {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest()
            for p in INPUT_PATHS + SOURCE_PATHS}


def _words(path, width):
    data = np.fromfile(ROOT / path, dtype='<u4')
    assert len(data) % width == 0, path
    return data.reshape(-1, width)


def _floats(words):
    return np.asarray(words, dtype='<u4').view('<f4')


def _parameters(field, cert, anchor_words, index_words):
    f = _floats(field)
    c = _floats(cert)
    count = int(f[1, 3])
    assert float(count) == f[1, 3] and int(cert[0, 2]) == count
    assert 2 <= count <= 128
    if np.array_equal(anchor_words, field[0, 2:]):
        anchor_index = None
        residual_words = cert[4, :2]
        owners = [int(f[1, 2])]
        assert np.array_equal(index_words, field[1, :2])
    else:
        matches = np.flatnonzero(np.all(field[5:5 + count, :2] == anchor_words, axis=1))
        assert len(matches) == 1
        anchor_index = int(matches[0])
        residual_words = cert[int(cert[1, 1]) + anchor_index, :2]
        assert index_words[0] == field[5 + anchor_index, 2]
        # Capture did not store the owner integer. Preserve every branch
        # consistent with its actual anchor/index/slope rather than guessing
        # from the sign of the attempted height step.
        owners = []
        for owner in (anchor_index, anchor_index + 1):
            slope = field[5 + owner - 1, 3] if 0 < owner < count else 0
            if index_words[1] == slope:
                owners.append(owner)
        assert owners
    result = {
        'anchor_split': tuple(map(float, _floats(anchor_words))),
        'eye_split': tuple(map(float, f[0, 2:])),
        'anchor_residual': tuple(map(float, _floats(residual_words))),
        'eye_residual': tuple(map(float, c[4, :2])),
        'axis_interval': tuple((float(c[2, i]), float(c[3, i])) for i in range(3)),
        'e_interval': (float(c[2, 3]), float(c[3, 3])),
    }
    return result, anchor_index, owners, residual_words


def _quarter(tolerance, numerical_error):
    # Match ns_prepare_piece: external scalar wrappers, outward subtraction,
    # and downward endpoint of multiplication by the exact quarter.
    remaining = sub(exact(tolerance), exact(numerical_error))
    quarter = mul(remaining, (.25, .25))
    assert 0 < quarter[0] <= quarter[1]
    assert Fraction(quarter[0]) <= (Fraction(tolerance) - Fraction(numerical_error)) / 4
    return quarter[0]


def load_actual_cases():
    """Return 34 accepted near-scene and 23 final pending scalar cases.

    ``case`` matches the reviewed scalar map/Cartesian producer transcription.
    Raw words and source identity remain alongside the convenient float view.
    The nominal 167 m capture filter is descriptive, not a ceiling certificate.
    """
    field = _words(CAPTURE + '-uniform.bin', 4)
    cert = _words(SCENE + '-certificates.bin', 4)
    assert np.array_equal(field, _words(SELECTED + '-field.bin', 4))
    assert np.array_equal(cert, _words(SELECTED + '-certificates.bin', 4))
    captured = _words(CAPTURE + '-readback.bin', 13864)
    assert captured.shape == (7, 13864)
    results = []
    accepted = 0
    for ray, record in enumerate(captured):
        count, capacity, overflow, reserved = map(int, record[36:40])
        assert capacity == 128 and count <= capacity and overflow == reserved == 0
        for trial in range(count):
            w = record[40 + 108 * trial:40 + 108 * (trial + 1)]
            f = _floats(w)
            if w[96] != 2:
                continue
            accepted += 1
            assert w[49] == 1 and w[95] == 8
            parameters, anchor, owners, residual = _parameters(field, cert, w[8:10], w[10:12])
            coeff = [list(map(float, f[12 + 4 * j:15 + 4 * j])) for j in range(4)]
            path = [row[0] for row in coeff]
            coeff[0][0] = float(f[3])
            for row in coeff[1:]:
                row[0] = 0.0
            case = dict(parameters, start=tuple(map(float, f[:3])), coeff=coeff)
            nominal_minimum = sum(case['anchor_split']) + case['start'][0] + min(0., float(f[3]))
            if nominal_minimum > 167.:
                continue
            numerical_error = add(exact(float(f[48])), exact(float(f[97])))[1]
            tolerance = float(f[98])
            results.append({
                'id': f'capture-{ray}-{trial}', 'population': 'accepted-height-capture',
                'ray': ray, 'trial': trial, 'case': case, 'theta_end': 1.,
                'start_u': list(map(float, f[4:7])),
                'ucoeff': [list(map(float, f[28 + 4 * j:31 + 4 * j])) for j in range(4)],
                'path': path, 'optical': list(map(float, f[44:48])),
                'numerical_error': numerical_error, 'tolerance': tolerance,
                'quarter_remaining_limit': _quarter(tolerance, numerical_error),
                'source_anchor_index': anchor, 'owner_candidates': owners,
                'owner_scope': 'owner integer not captured; branches matching exact anchor/index/slope only',
                'anchor_residual_words': residual.tolist(), 'source_index_words': w[10:12].tolist(),
                'raw_words': w.tolist(), 'raw_input': CAPTURE + '-readback.bin',
                'raw_word_offset': ray * 13864 + 40 + 108 * trial,
                'nominal_height_minimum_m_not_certificate': nominal_minimum,
                'scope': 'Accepted physical height polynomial with original path and source correction; no scene acceptance.',
            })
    assert accepted == 495 and len(results) == 34
    report = json.loads((ROOT / (SELECTED + '.json')).read_text())
    state = _words(SELECTED + '-states.bin', 128).reshape(23, 32, 4)
    samples = {r['record_index']: r for r in report['samples']}
    assert set(samples) == set(range(23))
    for ray, w in enumerate(state):
        f = _floats(w)
        assert w[7, 3] == 4 and w[12, 2] == ray and w[12, 3] == 8
        assert w[16, 3] == 1 and 0 < f[14, 3] <= 1
        parameters, anchor, owners, residual = _parameters(field, cert, w[3, 2:], w[4, :2])
        assert int(w[7, 0]) in owners and np.array_equal(residual, w[4, 2:])
        case = dict(parameters, start=tuple(map(float, f[0, :3])),
                    coeff=[list(map(float, row)) for row in f[13:17, :3]])
        numerical_error, tolerance = float(f[15, 3]), float(f[23, 0])
        results.append({
            'id': f'pending-{ray}', 'population': 'final-selected-pending',
            'ray': ray, 'pixel': samples[ray]['pixel'], 'case': case,
            'theta_end': float(f[14, 3]), 'start_u': list(map(float, f[1, :3])),
            'ucoeff': f[17:21, :3].astype(float).tolist(),
            'path': list(map(float, f[17:21, 3])), 'optical': list(map(float, f[21])),
            'numerical_error': numerical_error, 'tolerance': tolerance,
            'quarter_remaining_limit': _quarter(tolerance, numerical_error),
            'source_anchor_index': anchor, 'owner': int(w[7, 0]), 'owner_candidates': owners,
            'anchor_residual_words': residual.tolist(), 'source_index_words': w[4, :2].tolist(),
            'raw_words': w.reshape(-1).tolist(), 'raw_input': SELECTED + '-states.bin',
            'raw_word_offset': 128 * ray, 'prefix_path_m': float(f[0, 3]),
            'is_height': bool(w[12, 0]),
            'scope': 'Final uncommitted submillimetre retry; stale piece bytes and earlier trial history are not used.',
        })
    assert len(results) == 57
    return results


if __name__ == '__main__':
    cases = load_actual_cases()
    print(json.dumps({'counts': {name: sum(c['population'] == name for c in cases)
                                for name in ('accepted-height-capture', 'final-selected-pending')},
                      'input_inventory': input_inventory()}, indent=2))
