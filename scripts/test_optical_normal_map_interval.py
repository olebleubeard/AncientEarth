"""Independent finite tests of the unused canonical scene-map enclosure.

The scalar transcription uses normal f32 rounding followed by the shader's
outward endpoint operations. A separate exact-rational test covers every
permitted inverseSqrt output near selected binades. Neither is a claim about
GPU execution, global ray error, scene packing, or all mixed rounding choices.
The map oracle is a direct 90-digit Cartesian construction and differentiation,
not the shader's cancellation-resistant position/derivative identities.
"""
from fractions import Fraction as Q
import math
from pathlib import Path
import random
import unittest

import mpmath as mp

from test_optical_normal_intervals import (
    MIN_NORMAL, TINY_BOUND, bits, value, widen, rational_ulp,
)
from test_optical_normal_residual import add, sub, mul, exact, f32


ZERO = (0., 0.)
ONE = (1., 1.)
UP = (ZERO, ONE, ZERO)
A = 6378137
E2 = 0.0066943799901413165


def div(a, b):
    if b[0] <= 0 <= b[1]:
        raise ValueError('uncertain denominator')
    if a == ZERO:
        return ZERO
    # Four independent endpoint ratios avoid reusing the shader's extremum
    # selection logic. The same eight-neighbor rounding allowance applies.
    ratios = [f32(x / y) for x in a for y in b]
    return widen(min(ratios), 8)[0], widen(max(ratios), 8)[1]


def rsqrt(a):
    if not (MIN_NORMAL <= a[0] <= a[1] <= 1e20):
        raise ValueError('inverse-root domain')
    lo, hi = f32(1 / math.sqrt(a[1])), f32(1 / math.sqrt(a[0]))
    return widen(lo, 4)[0], widen(hi, 4)[1]


def square(a):
    lo, hi = mul(a, a)
    return max(0., lo), hi


def vexact(v):
    return tuple(map(exact, v))


def vadd(a, b):
    return tuple(add(x, y) for x, y in zip(a, b))


def vsub(a, b):
    return tuple(sub(x, y) for x, y in zip(a, b))


def scale(a, b):
    return tuple(mul(x, b) for x in a)


def dot(a, b):
    return add(add(mul(a[0], b[0]), mul(a[1], b[1])), mul(a[2], b[2]))


def polynomial(start, coeff, t):
    c2, c3, c4, c5 = map(vexact, coeff)
    z = sub(ONE, t)
    return vadd(vexact(start), scale(vadd(c2, scale(vadd(c3,
        scale(vadd(c4, scale(c5, z)), t)), z)), t))


def derivative(coeff, t):
    c2, c3, c4, c5 = map(vexact, coeff)
    t2, t3 = mul(t, t), mul(mul(t, t), t)
    b3 = sub(ONE, mul((2., 2.), t))
    b4 = sub(mul((2., 2.), t), mul((3., 3.), t2))
    b5 = add(sub(mul((2., 2.), t), mul((6., 6.), t2)), mul((4., 4.), t3))
    return vadd(vadd(c2, scale(c3, b3)), vadd(scale(c4, b4), scale(c5, b5)))


def map_enclosure(case, t):
    q = polynomial(case['start'], case['coeff'], t)
    dq = derivative(case['coeff'], t)
    q2 = add(square(q[1]), square(q[2]))
    inverse = rsqrt(add(ONE, q2))
    dy = sub(ZERO, div(mul(q2, mul(inverse, inverse)), add(ONE, inverse)))
    dn = (mul(q[1], inverse), dy, sub(ZERO, mul(q[2], inverse)))
    n = vadd(UP, dn)
    axis, e = case['axis_interval'], case['e_interval']
    z0 = axis[1]
    dz = dot(axis, dn)
    z = add(z0, dz)
    d0, d = sub(ONE, mul(e, square(z0))), sub(ONE, mul(e, square(z)))
    r0, r = rsqrt(d0), rsqrt(d)
    radius = mul((float(A), float(A)), r)
    product = mul(r, r0)
    dr = div(mul(mul(mul(mul((float(A), float(A)), e), dz),
                          add(z, z0)), mul(product, product)), add(r, r0))
    ah, al = map(exact, case['anchor_split'])
    eh, el = map(exact, case['eye_split'])
    h = add(add(add(ah, al), q[0]), case['anchor_residual'])
    dh = add(add(add(sub(ah, eh), sub(al, el)), q[0]),
             sub(case['anchor_residual'], case['eye_residual']))
    tangent = vsub(dn, scale(axis, mul(e, dz)))
    initial = vsub(UP, scale(axis, mul(e, z0)))
    position = vadd(vadd(scale(tangent, radius), scale(initial, dr)),
                    vadd(scale(dn, h), scale(UP, dh)))
    w = (dq[1], ZERO, sub(ZERO, dq[2]))
    nprime = scale(vsub(w, scale(n, dot(n, w))), inverse)
    meridian = vsub(axis, scale(n, z))
    coefficient = mul(div(mul(radius, e), d), dot(nprime, axis))
    velocity = vsub(vadd(scale(n, dq[0]), scale(nprime, add(radius, h))),
                    scale(meridian, coefficient))
    return position, velocity


def packed_interval(x):
    """Outward pack a high-precision point; endpoints survive FTZ."""
    nearest = f32(float(x))
    lo, hi = nearest, nearest
    if mp.mpf(lo) > x:
        lo = widen(lo, 1)[0]
    if mp.mpf(hi) < x:
        hi = widen(hi, 1)[1]
    if 0 < abs(lo) < MIN_NORMAL:
        lo = 0. if lo > 0 else -MIN_NORMAL
    if 0 < abs(hi) < MIN_NORMAL:
        hi = MIN_NORMAL if hi > 0 else 0.
    if x < 0 and lo == 0:
        lo = -MIN_NORMAL
    if x > 0 and hi == 0:
        hi = MIN_NORMAL
    return lo, hi


def make_case(axis_raw, anchor, eye, start, coeff):
    norm = mp.sqrt(sum(mp.mpf(x)**2 for x in axis_raw))
    axis = tuple(mp.mpf(x) / norm for x in axis_raw)
    def split(h):
        high = f32(h)
        low = f32(h - high)
        residual = mp.mpf(h) - mp.mpf(high) - mp.mpf(low)
        return (high, low), packed_interval(residual)
    anchor_split, anchor_residual = split(anchor)
    eye_split, eye_residual = split(eye)
    return dict(axis=axis, axis_interval=tuple(map(packed_interval, axis)),
                e=mp.mpf(E2), e_interval=packed_interval(mp.mpf(E2)),
                anchor=mp.mpf(anchor), eye=mp.mpf(eye),
                anchor_split=anchor_split, anchor_residual=anchor_residual,
                eye_split=eye_split, eye_residual=eye_residual,
                start=tuple(map(f32, start)),
                coeff=tuple(tuple(map(f32, c)) for c in coeff))


def canonical(case, theta):
    # Expanded power polynomial independently obtains the chart state. The
    # oracle subtracts two full Cartesian points at high precision; it never
    # uses the stable displacement or analytic derivative formulas under test.
    q = []
    for component in range(3):
        c2, c3, c4, c5 = [mp.mpf(c[component]) for c in case['coeff']]
        q.append(mp.mpf(case['start'][component]) + (c2+c3)*theta
                 + (c4-c3+c5)*theta**2 + (-c4-2*c5)*theta**3 + c5*theta**4)
    n = (q[1], mp.mpf(1), -q[2])
    norm = mp.sqrt(sum(x*x for x in n))
    n = tuple(x/norm for x in n)
    k, e = case['axis'], case['e']
    def point(normal, height):
        z = sum(x*y for x, y in zip(normal, k))
        radius = A / mp.sqrt(1-e*z*z)
        return tuple(radius*(x-e*z*y)+height*x for x, y in zip(normal, k))
    p = point(n, case['anchor']+q[0])
    origin = point((mp.mpf(0), mp.mpf(1), mp.mpf(0)), case['eye'])
    return tuple(x-y for x, y in zip(p, origin))


def scene_acceleration_upper(case):
    """Finite scalar transcription of the separate scene-curve bridge."""
    c = case['coeff']
    q = [add(exact(abs(v[1])), exact(abs(v[2]))) for v in c]
    h = [exact(abs(v[0])) for v in c]
    q1, h1 = add(add(q[0], q[1]), add(q[2], q[3])), add(add(h[0], h[1]), add(h[2], h[3]))
    q2 = add(add(mul((2., 2.), q[1]), mul((4., 4.), q[2])), mul((2., 2.), q[3]))
    h2 = add(add(mul((2., 2.), h[1]), mul((4., 4.), h[2])), mul((2., 2.), h[3]))
    h0 = add(add(*map(exact, case['anchor_split'])), exact(case['start'][0]))
    hmax = add(add(exact(max(map(abs, h0))), h1), exact(max(map(abs, case['anchor_residual']))))
    q1_squared = mul(q1, q1)
    normal_second = add(q2, mul((3., 3.), q1_squared))
    radius = add(add((6400000., 6400000.), hmax), (44000., 44000.))
    coupling = mul(mul((2., 2.), h1), q1)
    last = mul(mul((44000., 44000.), exact(f32(3.025))), q1_squared)
    return add(add(h2, coupling), add(mul(radius, normal_second), last))[1]


class NormalMapIntervalReview(unittest.TestCase):
    def test_inverse_root_four_neighbors_cover_allowed_two_ulp_outputs(self):
        checked = 0
        # Exact desired real outputs, including fractions between f32 values.
        # The enclosure lemma depends only on output error, so it covers a
        # correctly bounded inverse-root endpoint without assuming RN mode.
        for exponent in [-34, -10, -1, 0, 1, 40, 62]:
            center = Q(2)**exponent
            spacing = center / 2**23
            for offset in range(-40, 41):
                truth = center + Q(offset, 8)*spacing
                nearest = bits(float(truth))
                for raw in range(nearest-5, nearest+6):
                    observed = value(raw)
                    if abs(Q(observed)-truth) <= 2*rational_ulp(truth):
                        lo, hi = widen(observed, 4)
                        self.assertLessEqual(Q(lo), truth)
                        self.assertGreaterEqual(Q(hi), truth)
                        checked += 1
        self.assertGreater(checked, 2000)
        with self.assertRaises(ValueError):
            rsqrt((0., 1.))
        with self.assertRaises(ValueError):
            rsqrt((1., f32(1e21)))
        source = Path(__file__).with_name('optical_normal_map_interval.wgsl').read_text()
        self.assertIn('a.x>=NC_I_MIN_NORMAL && a.y<=1.0e20', source)
        self.assertIn('nc_interval_down_steps(lower,4u)', source)
        self.assertIn('nc_interval_up_steps(upper,4u)', source)

    def assert_encloses(self, interval, reference):
        self.assertTrue(all(math.isfinite(x) for x in interval))
        self.assertLessEqual(mp.mpf(interval[0]), reference)
        self.assertGreaterEqual(mp.mpf(interval[1]), reference)

    def test_cartesian_and_differentiated_oracles_at_poles_heights_and_far_charts(self):
        generator = random.Random(0x6e6f726d)
        checked = 0
        with mp.workdps(90):
            axes = [(0., 0., -1.), (0., 1., 0.), (0., -1., 0.),
                    (0., .6, -.8), (.3, -.4, .5)]
            for axis in axes:
                for height in [-9999.875, 0., 3757.200123456789, 100000.123456789]:
                    for chart in [(0., 0.), (1e-7, -2e-7), (.35, -.4)]:
                        # Alternate affine-H and general dense-H records.
                        for affine in [True, False]:
                            coeff = [[generator.uniform(-1000, 1000),
                                      generator.uniform(-.003, .003),
                                      generator.uniform(-.003, .003)] for _ in range(4)]
                            if affine:
                                for c in coeff[1:]:
                                    c[0] = 0.
                            case = make_case(axis, height, height-37.123456789,
                                             (.03125, *chart), coeff)
                            for t in [0., .125, .5, .875, 1.]:
                                position, velocity = map_enclosure(case, exact(t))
                                p = canonical(case, mp.mpf(t))
                                dp = tuple(mp.diff(lambda tt: canonical(case, tt)[i],
                                                   mp.mpf(t)) for i in range(3))
                                for interval, ref in zip(position+velocity, p+dp):
                                    self.assert_encloses(interval, ref)
                                checked += 1
            self.assertEqual(checked, 600)

    def test_interval_parameters_and_tiny_inputs_keep_the_independent_curve_enclosed(self):
        with mp.workdps(90):
            cases = [
                make_case((0., .8, -.6), -26.00552287325263, -26.00552287325263,
                          (0., 0., 0.), [[0., 0., 0.]]*4),
                make_case((.2, -.3, .9), 78.084735, 52.359210968,
                          (value(1), value(0x80000001), value(2)),
                          [[1e-38, -1e-38, 1e-37], [0., 0., 0.],
                           [0., 0., 0.], [0., 0., 0.]]),
                make_case((0., 1., 0.), -10000., 125.123456789,
                          (250., -.25, .125),
                          [[-100., .003, -.007], [50., -.005, .004],
                           [-13., .002, .008], [8., -.001, .002]]),
            ]
            checked = 0
            for case in cases:
                for lo, hi in [(0., .125), (.25, .75), (.875, 1.)]:
                    position, velocity = map_enclosure(case, (lo, hi))
                    for j in range(9):
                        t = mp.mpf(lo)+(mp.mpf(hi)-lo)*j/8
                        p = canonical(case, t)
                        dp = tuple(mp.diff(lambda tt: canonical(case, tt)[i], t)
                                   for i in range(3))
                        for interval, ref in zip(position+velocity, p+dp):
                            self.assert_encloses(interval, ref)
                        checked += 1
            self.assertEqual(checked, 81)
        source = Path(__file__).with_name('optical_normal_map_interval.wgsl').read_text()
        self.assertIn('theta.y<=curve.theta_end', source)
        self.assertIn('parameters.anchor_residual', source)
        self.assertIn('parameters.eye_residual', source)
        self.assertIn('parameters.eccentricity.y>0.007', source)
        self.assertIn('if !nc_m_valid(p)||!nc_m_valid(velocity){return failure;}', source)

    def test_scene_acceleration_bound_covers_independently_differentiated_dense_curve(self):
        generator = random.Random(0x503232)
        with mp.workdps(90):
            # Independently check the three canonical constants used in the
            # source proof. Sampled curves below supplement that proof; they
            # cannot establish a bound between the sampled parameter values.
            e = mp.mpf(E2)
            self.assertLess(A/mp.sqrt(1-e), 6400000)
            self.assertLess(A*e/(1-e)**mp.mpf('1.5'), 44000)
            self.assertLess(3+3*e/(1-e), mp.mpf(f32(3.025)))
            checked = 0
            for axis in [(0., 0., -1.), (0., 1., 0.), (.3, -.4, .5)]:
                for height in [-9999.875, 17.123456789, 120000.123456789]:
                    for affine in [False, True]:
                        coefficients = [[generator.uniform(-2000, 2000),
                                         generator.uniform(-.03, .03),
                                         generator.uniform(-.03, .03)] for _ in range(4)]
                        if affine:
                            for c in coefficients[1:]:
                                c[0] = 0.
                        case = make_case(axis, height, height-1, (3.125, .35, -.2), coefficients)
                        bound = scene_acceleration_upper(case)
                        for j in range(17):
                            t = mp.mpf(j)/16
                            acceleration = [mp.diff(lambda tt: canonical(case, tt)[i], t, 2)
                                            for i in range(3)]
                            self.assertLessEqual(mp.sqrt(sum(x*x for x in acceleration)), bound)
                            checked += 1
            self.assertEqual(checked, 306)
        source = Path(__file__).with_name('optical_normal_scene_curve.wgsl').read_text()
        self.assertIn('nc_interval_exact(6400000.0)', source)
        self.assertIn('nc_interval_exact(44000.0)', source)
        self.assertIn('nc_interval_exact(3.025)', source)


if __name__ == '__main__':
    unittest.main()
