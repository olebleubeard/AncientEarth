"""Independently audit raw normal-chart queries against Cartesian/principal axes."""
import hashlib
import json
import math
from pathlib import Path
import struct
import sys


def dot(a, b):
    return math.fsum(x * y for x, y in zip(a, b))


def norm(a):
    return math.sqrt(dot(a, a))


def difference(a, b):
    return [x - y for x, y in zip(a, b)]


def add(a, b):
    return [x + y for x, y in zip(a, b)]


def scale(a, k):
    return [x * k for x in a]


def canonical(case):
    """Direct Cartesian map and principal-curvature RHS, not the GPU shape formula."""
    a = 6378137.0
    e2 = (1 / 298.257223563) * (2 - 1 / 298.257223563)
    latitude = math.radians(case['latitude_deg'])
    pole = [0.0, math.sin(latitude), -math.cos(latitude)]
    offset, ca, cb, _ = case['encoded_state']
    u = case['encoded_direction'][:3]
    normal = scale([ca, 1.0, -cb], 1 / math.sqrt(1 + ca * ca + cb * cb))
    branch = case['encoded_branch']
    height = branch[0] + branch[1] + offset
    z = dot(normal, pole)
    d = 1 - e2 * z * z
    nr = a / math.sqrt(d)
    mr = a * (1 - e2) / (d ** 1.5)
    # Deliberately subtract two direct Cartesian normal-coordinate positions.
    # At these scales binary64 cancellation contributes only nanometres.
    absolute = add(scale(difference(normal, scale(pole, e2 * z)), nr), scale(normal, height))
    up = [0.0, 1.0, 0.0]
    nr0 = a / math.sqrt(1 - e2 * pole[1] ** 2)
    eye = add(scale(difference(up, scale(pole, e2 * pole[1])), nr0), scale(up, case['eye_height_m']))
    vertical = dot(u, normal)
    tangent = difference(u, scale(normal, vertical))
    north = difference(pole, scale(normal, z))
    north_norm = norm(north)
    dn = scale(tangent, 1 / (nr + height))
    if north_norm > 1e-12:
        north = scale(north, 1 / north_norm)
        dn = add(dn, scale(north, dot(u, north) * (1 / (mr + height) - 1 / (nr + height))))
    dq = [vertical, (dn[0] - ca * dn[1]) / normal[1], (-dn[2] - cb * dn[1]) / normal[1]]
    du = scale(difference(normal, scale(u, vertical)), branch[3] / (1 + branch[2] + branch[3] * offset))
    return {'normal': normal, 'position': difference(absolute, eye), 'rhsq': dq, 'rhsu': du,
            'physical_derivative': u}


def audit(prefix):
    path = Path(str(prefix) + '.json')
    report = json.loads(path.read_text())
    raw = Path(str(prefix) + '-readback.bin').read_bytes()
    uploaded = Path(str(prefix) + '-inputs.bin').read_bytes()
    cases = report['cases']
    assert len(cases) == 192 and len(raw) == 192 * 112 and len(uploaded) == 192 * 64
    maxima = {}
    field_valid = trial_valid = initial_zero = 0
    source_reference_differences = {}
    for index, case in enumerate(cases):
        words = struct.unpack_from('<28I', raw, 112 * index)
        values = struct.unpack_from('<28f', raw, 112 * index)
        assert list(words) == case['gpu_raw_words']
        encoded = sum((case[k] for k in ('encoded_pose', 'encoded_branch', 'encoded_state', 'encoded_direction')), [])
        assert struct.pack('<16f', *encoded) == uploaded[index * 64:(index + 1) * 64]
        assert all(math.isfinite(v) for i, v in enumerate(values) if i not in (11, 27))
        field_valid += words[11] == 1
        trial_valid += words[27] == 1
        reference = canonical(case)
        for key, start in [('normal', 0), ('position', 4), ('rhsq', 8), ('rhsu', 12), ('physical_derivative', 16)]:
            error = norm(difference(values[start:start + 3], reference[key]))
            if error > maxima.get(key, {'error': -1})['error']:
                maxima[key] = {'error': error, 'case': index, 'latitude_deg': case['latitude_deg'],
                               'eye_height_m': case['eye_height_m'], 'encoded_state': case['encoded_state']}
        for key, supplied in [('normal', 'canonical_normal'), ('position', 'canonical_position_delta_m'),
                              ('rhsq', 'canonical_coordinate_derivative'), ('rhsu', 'canonical_direction_derivative'),
                              ('physical_derivative', 'canonical_position_derivative')]:
            error = norm(difference(reference[key], case[supplied]))
            source_reference_differences[key] = max(source_reference_differences.get(key, 0), error)
        if index % 6 == 0:
            assert all(v == 0 for v in values[4:7])
            initial_zero += 1
    assert field_valid == trial_valid == 192 and initial_zero == 32
    outputs = json.loads(Path(str(prefix) + '-outputs.json').read_text(encoding='utf-8-sig'))
    for item in outputs:
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256']
    return {'scope': '192 finite raw field/trial queries; independent direct Cartesian map and principal-curvature RHS. Trial errors are recorded, not validated as global/contact accuracy or accepted solver budgets.',
            'cases': len(cases), 'field_valid': field_valid, 'trial_valid': trial_valid,
            'initial_positions_exactly_zero': initial_zero, 'maxima': maxima,
            'maximum_reference_reformulation_differences': source_reference_differences,
            'all_output_hashes_match': True, 'raw_sha256': hashlib.sha256(raw).hexdigest()}


if __name__ == '__main__':
    prefix = Path(sys.argv[1] if len(sys.argv) > 1 else 'artifacts/optical-normal-query-first')
    result = audit(prefix)
    Path(str(prefix) + '-independent-review.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
