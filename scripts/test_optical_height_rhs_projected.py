"""Finite diagnostics for the unused projected height derivative.

The 80-digit reference uses the original three-dimensional shape operator.
The candidate projects it algebraically before evaluation. Binary32 results
below are scalar round-to-nearest transcriptions, not all allowed WGSL
roundings or GPU execution. Complete-ray/source/contact checks remain required.
"""
import json
from pathlib import Path
import random
import struct
import unittest

import mpmath as mp

A = 6378137.0
E = 0.0066943799901413165


def f32(x):
    return struct.unpack('<f', struct.pack('<f', float(x)))[0]


def dot(a, b, cast):
    return cast(cast(cast(a[0]*b[0])+cast(a[1]*b[1]))+cast(a[2]*b[2]))


def field(chart, u, k, h, refractivity, slope, cast, projected):
    a, b = map(cast, chart)
    u, k = tuple(map(cast, u)), tuple(map(cast, k))
    h, refractivity, slope = map(cast, (h, refractivity, slope))
    earth, e = cast(A), cast(E)
    norm2 = cast(cast(1)+cast(cast(a*a)+cast(b*b)))
    inv = cast(1/mp.sqrt(norm2))
    n = (cast(a*inv), inv, cast(-b*inv))
    z, v = dot(n, k, cast), dot(u, n, cast)
    d = cast(cast(1)-cast(cast(e*z)*z))
    root = cast(mp.sqrt(d))
    inverse_v = cast(1/v)
    index = cast(cast(1)+refractivity)
    if projected:
        dr = cast(earth+cast(h*root))
        dm = cast(cast(earth*cast(cast(1)-e))+cast(cast(h*d)*root))
        ratio = cast(cast(earth*e)/dm)
        scale = cast(cast(root*inverse_v)/cast(n[1]*dr))
        pu = (cast(u[0]-cast(a*u[1])), cast(-u[2]-cast(b*u[1])))
        pk = (cast(k[0]-cast(a*k[1])), cast(-k[2]-cast(b*k[1])))
        mu = cast(dot(k, u, cast)-cast(z*v))
        q = (inverse_v, *(cast(scale*cast(p+cast(cast(ratio*mu)*t))) for p, t in zip(pu, pk)))
        du = tuple(cast(cast(cast(x*inverse_v)-y)*cast(slope/index)) for x, y in zip(n, u))
    else:
        nr = cast(earth/root)
        mr = cast(cast(earth*cast(cast(1)-e))/cast(d*root))
        tangent = tuple(cast(y-cast(x*v)) for x, y in zip(n, u))
        meridian = tuple(cast(y-cast(x*z)) for x, y in zip(n, k))
        coefficient = cast(cast(earth*e)/cast(cast(cast(d*root)*cast(mr+h))*cast(nr+h)))
        c = cast(coefficient*dot(meridian, u, cast))
        dn = tuple(cast(cast(t/cast(nr+h))+cast(m*c)) for t, m in zip(tangent, meridian))
        q = (inverse_v,
             cast(cast(cast(dn[0]-cast(a*dn[1]))/n[1])*inverse_v),
             cast(cast(cast(-dn[2]-cast(b*dn[1]))/n[1])*inverse_v))
        du = tuple(cast(cast(cast(x-cast(y*v))*cast(slope/index))*inverse_v) for x, y in zip(n, u))
    return (*q, *du, cast(refractivity*inverse_v))


def cases():
    rng = random.Random(0x513DF1)
    for i in range(600):
        chart = [rng.uniform(-0.65, 0.65), rng.uniform(-0.65, 0.65)]
        latitude = rng.uniform(-mp.pi/2, mp.pi/2)
        axis = [0., float(mp.sin(latitude)), -float(mp.cos(latitude))]
        normal = [chart[0], 1., -chart[1]]
        nlength = sum(x*x for x in normal)**.5
        normal = [x/nlength for x in normal]
        while True:
            direction = [rng.uniform(-1.4, 1.4) for _ in range(3)]
            vertical = sum(x*y for x, y in zip(normal, direction))
            if .025 < abs(vertical) < 1.8:
                break
        yield (chart, direction, axis, rng.uniform(-10000., 120001.),
               rng.uniform(0., .0004), rng.uniform(-2e-8, 2e-8))


class ProjectedDerivative(unittest.TestCase):
    def test_exact_algebra_including_descending_and_off_unit_stages(self):
        with mp.workdps(80):
            for data in cases():
                old = field(*data, mp.mpf, False)
                new = field(*data, mp.mpf, True)
                for a, b in zip(old, new):
                    self.assertLessEqual(abs(a-b), mp.mpf('1e-70')*max(1, abs(a), abs(b)))

    def test_binary32_diagnostic_finite_and_measured(self):
        maxima = {'original': [0.]*7, 'projected': [0.]*7}
        with mp.workdps(80):
            for data in cases():
                # Oracle uses identical encoded state/axis inputs and the
                # canonical WGS84 constants. Reported finite errors therefore
                # include the shader's binary32 eccentricity quantization.
                encoded = ([f32(x) for x in data[0]], [f32(x) for x in data[1]],
                           [f32(x) for x in data[2]], *map(f32, data[3:]))
                old, new = (field(*encoded, f32, flag) for flag in (False, True))
                truth = field(*encoded, mp.mpf, False)
                for name, values in [('original', old), ('projected', new)]:
                    for j, (value, reference) in enumerate(zip(values, truth)):
                        self.assertTrue(mp.isfinite(value))
                        maxima[name][j] = max(maxima[name][j], float(abs(mp.mpf(value)-reference)))
        # Diagnostic thresholds catch transcription errors, not full-ray
        # accuracy claims. Both signs and deliberately non-unit stages are used.
        self.assertLess(maxima['projected'][0], 4e-5)
        self.assertLess(max(maxima['projected'][1:3]), 1e-11)
        self.assertLess(max(maxima['projected'][3:6]), 5e-12)
        self.assertLess(maxima['projected'][6], 2e-8)
        print(json.dumps({'scope': '600 scalar binary32 field evaluations, not GPU/ray acceptance',
                          'component_order': ['path', 'chart_a', 'chart_b', 'u_x', 'u_y', 'u_z', 'optical_excess'],
                          'maximum_absolute_derivative_errors': maxima}, indent=2))

    def test_helper_remains_isolated_with_original_fallback(self):
        source = Path(__file__).with_name('optical_height_rhs_projected.wgsl').read_text()
        self.assertEqual(source.count('return nh_rhs_reference(state,orientation);'), 1)
        self.assertIn('h>=-10000.0 && h<=120001.0', source)
        self.assertIn('orientation*vertical>0.01', source)
        self.assertIn('index<0.5', source)


if __name__ == '__main__':
    unittest.main()
