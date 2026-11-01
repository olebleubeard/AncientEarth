"""Independent finite clear-ray readback review; no terrain or timing claim."""
import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import struct

import numpy as np

from verify_optical_dop853 import geometry, integrate, differences


def decoded(words):
    return np.array(struct.unpack('<' + 'f' * len(words), struct.pack('<' + 'I' * len(words), *words)))


def encoded_gap_checks(uniform):
    """Exact rational differences of the uploaded floats, no host gap formula."""
    count, owner = int(uniform[1][3]), int(uniform[1][2])
    knots = uniform[5:5 + count]
    gaps = uniform[133:]
    exact = lambda pair: sum((Fraction(float(x)) for x in pair[:2]), Fraction())
    pairs = [(uniform[3], uniform[0][2:], [-10000., 0.] if owner == 0 else knots[owner - 1]),
             (uniform[4], uniform[0][2:], knots[owner])]
    pairs.extend((gaps[i], knots[i + 1], knots[i]) for i in range(count - 1))
    pairs.append((gaps[count - 1], knots[0], [-10000., 0.]))
    largest = 0.
    for encoded, a, b in pairs:
        reference = exact(a) - exact(b)
        residual = abs(exact(encoded) - reference)
        assert residual <= Fraction(float(encoded[2]))
        largest = max(largest, float(residual))
    return {'count': len(pairs), 'maximum_exact_split_residual_m': largest}


def review(prefix):
    path = Path(prefix)
    report = json.loads(path.with_suffix('.json').read_text())
    for record in json.loads(Path(str(path) + '-outputs.json').read_text()):
        actual = hashlib.sha256(Path(record['path']).read_bytes()).hexdigest()
        assert actual == record['sha256'], record['path']
    assert json.loads(Path(str(path) + '-input-stability.json').read_text())['inputs_unchanged_through_run']
    raw = Path(str(path) + '-readback.bin').read_bytes()
    words = struct.unpack('<' + 'I' * (len(raw) // 4), raw)
    assert list(words) == sum(report['gpu_raw_records'], [])
    uniform_raw = Path(str(path) + '-uniform.bin').read_bytes()
    uniform = np.array(struct.unpack('<' + 'f' * (len(uniform_raw) // 4), uniform_raw)).reshape((-1, 4))
    assert uniform.tolist() == report['fixture']['encoded_uniform']
    rows, failed = [], []
    for case, record in zip(report['fixture']['cases'], report['gpu_raw_records'], strict=True):
        original = case['canonical_inputs']
        origin = np.array(original['origin_ecef_m'])
        _, up = geometry(origin)
        east = np.array([-origin[1], origin[0], 0.])
        east /= np.linalg.norm(east)
        north = np.cross(up, east)
        basis = np.column_stack((east, up, -north))
        launch = decoded(record[12:15])
        input_case = dict(original, initial_direction=list(basis @ launch))
        if record[7] != 0:
            # A failed prefix is not an exit and must never be compared with
            # the final reference endpoint as though it were an accuracy error.
            failed.append({'name': case['name'], 'status': record[7], 'reason': record[10],
                           'attempts': record[8], 'root_calls': record[9],
                           'partial_path_m': float(decoded(record[3:4])[0]),
                           'raw_words': record,
                           'decoded_diagnostic_slots': [list(decoded(record[i:i+4]))
                                                        for i in range(16, len(record), 4)]})
            continue
        coarse, reference = integrate(input_case, False), integrate(input_case, True)
        observed = {'endpoint_ecef_m': list(origin + basis @ decoded(record[:3])),
                    'exit_direction': list(basis @ decoded(record[4:7])),
                    'geometric_path_m': float(decoded(record[3:4])[0])}
        def compare(ref):
            return {'endpoint_m': float(np.linalg.norm(np.array(observed['endpoint_ecef_m']) - ref['endpoint_ecef_m'])),
                    'direction': float(np.linalg.norm(np.array(observed['exit_direction']) - ref['exit_direction'])),
                    'path_m': abs(observed['geometric_path_m'] - ref['geometric_path_m'])}
        rows.append({'name': case['name'], 'raw_launch_words': record[12:15],
                     'launch_norm_defect': float(abs(np.linalg.norm(launch) - 1)),
                     'attempts': record[8], 'root_calls': record[9],
                     'canonical_original_launch_difference': compare(case['reference']),
                     'canonical_readback_launch_difference': compare(reference),
                     'canonical_readback_launch_reference': reference,
                     'reference_convergence': differences(coarse, reference)})
    return {'scope': 'Only completed clear rays are compared with final canonical WGS84 references; every failed prefix remains separately recorded and is not counted as an accurate exit. Readback-launch references normalize the actual GPU launch mapped through the original f64 eye basis. Source/axis/coordinate packing and accumulated integration error remain in the measured differences. Optical path is not returned by this GPU fixture. No collision, full-image, throughput or universal accuracy claim.',
            'input_report_sha256': hashlib.sha256(path.with_suffix('.json').read_bytes()).hexdigest(),
            'all_output_hashes_verified': True, 'encoded_gap_checks': encoded_gap_checks(uniform),
            'resolved': len(rows), 'all_clear_rays_resolved': not failed,
            'failed_cases': failed, 'cases': rows,
            'maximum_original_launch_difference': {key: max((row['canonical_original_launch_difference'][key] for row in rows), default=None) for key in ['endpoint_m', 'direction', 'path_m']},
            'maximum_readback_launch_difference': {key: max((row['canonical_readback_launch_difference'][key] for row in rows), default=None) for key in ['endpoint_m', 'direction', 'path_m']},
            'maximum_reference_convergence': {key: max((row['reference_convergence'][key] for row in rows), default=None) for key in ['endpoint_m', 'direction', 'path_m', 'optical_path_m']}}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    args = parser.parse_args()
    result = review(args.prefix)
    output = Path(str(args.prefix) + '-independent-review.json')
    output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ('cases', 'failed_cases')}, indent=2))
