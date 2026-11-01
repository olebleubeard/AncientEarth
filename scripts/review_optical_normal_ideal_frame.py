"""Independent high-precision direct-angle audit of Rust ideal-frame outputs."""
import argparse
import itertools
import json
import struct
from pathlib import Path
import mpmath as mp


def number(bits):
    return mp.mpf(struct.unpack('<d', struct.pack('<Q', bits))[0])


def check(bounds, expected):
    assert number(bounds[0]) <= expected <= number(bounds[1]), (bounds, str(expected))


def review(report):
    largest_origin_width = mp.mpf(0)
    largest_rotation_width = mp.mpf(0)
    with mp.workdps(90):
        earth_a, eccentricity = number(report['a_bits']), number(report['e2_bits'])
        assert len(report['cases']) == 225
        for case in report['cases']:
            east = list(map(number, case['raw_east_bits']))
            up = list(map(number, case['raw_up_bits']))
            north = list(map(number, case['raw_north_bits']))
            assert east[2] == 0
            # Direct spherical-angle construction is independent of the
            # interval helper's Cartesian normalization/linear combinations.
            longitude = mp.atan2(-east[0], east[1])
            latitude = mp.atan2(up[2], north[2])
            sl, cl, s, c = mp.sin(longitude), mp.cos(longitude), mp.sin(latitude), mp.cos(latitude)
            columns = [[-sl, cl, 0], [c*cl, c*sl, s], [s*cl, s*sl, -c]]
            for i, column in enumerate(columns):
                for j, value in enumerate(column):
                    check(case['columns_bits'][i][j], value)
                    b = case['columns_bits'][i][j]
                    largest_rotation_width = max(largest_rotation_width, number(b[1])-number(b[0]))
                for j, other in enumerate(columns):
                    assert abs(sum(x*y for x,y in zip(column,other))-(i==j)) < mp.mpf('1e-85')
            matrix = mp.matrix([[columns[j][i] for j in range(3)] for i in range(3)])
            assert abs(mp.det(matrix)-1) < mp.mpf('1e-85')
            raw_axis = [east[2], up[2], -north[2]]
            magnitude = mp.sqrt(sum(x*x for x in raw_axis))
            for j in range(3):
                assert abs(columns[j][2]-raw_axis[j]/magnitude) < mp.mpf('1e-85')
            height = number(case['geodetic_bits'][2])
            radius = earth_a/mp.sqrt(1-eccentricity*s*s)
            origin = [(radius+height)*c*cl, (radius+height)*c*sl, (radius*(1-eccentricity)+height)*s]
            point = list(map(number, case['point_bits']))
            difference = [x-y for x,y in zip(origin,point)]
            for j in range(3):
                check(case['origin_bits'][j], origin[j])
                check(case['origin_residual_bits'][j], difference[j])
                check(case['eye_minus_point_bits'][j], sum(x*y for x,y in zip(columns[j],difference)))
                b = case['origin_bits'][j]
                largest_origin_width = max(largest_origin_width, number(b[1])-number(b[0]))
            for corner in itertools.product(*[[number(x) for x in b] for b in case['vector_bits']]):
                for j in range(3):
                    check(case['projected_vector_bits'][j], sum(x*y for x,y in zip(columns[j],corner)))
    return dict(cases=225, projected_box_corners=1800, passed=True,
                maximum_origin_component_width_m=float(largest_origin_width),
                maximum_rotation_component_width=float(largest_rotation_width),
                scope='Exact exported binary64 frame convention, direct 90-digit spherical-angle/Cartesian oracle; finite host enclosure evidence only')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('input', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = review(json.loads(args.input.read_text(encoding='utf-8-sig')))
    output = json.dumps(result, indent=2)
    if args.output:
        assert not args.output.exists(), 'retain earlier evidence'
        args.output.write_text(output+'\n', encoding='utf-8')
    print(output)
