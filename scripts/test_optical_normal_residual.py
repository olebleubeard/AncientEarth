"""Finite scalar checks of the bounded diagnostic residual specialization."""
from fractions import Fraction as Q
import math
from pathlib import Path
import random
import unittest

from test_optical_normal_intervals import MIN_NORMAL, TINY_BOUND, bits, value, widen, sterbenz_guard


def f32(x):
    return value(bits(x))


def exact(x):
    return (-TINY_BOUND, TINY_BOUND) if abs(x) < MIN_NORMAL else (x, x)


def add(a, b):
    if a == (0., 0.):
        return b
    if b == (0., 0.):
        return a
    lo, hi = f32(a[0] + b[0]), f32(a[1] + b[1])
    return (lo if sterbenz_guard(a[0], -b[0], lo) else widen(lo, 1)[0],
            hi if sterbenz_guard(a[1], -b[1], hi) else widen(hi, 1)[1])


def sub(a, b):
    return add(a, (-b[1], -b[0]))


def mul(a, b):
    if a == (0., 0.) or b == (0., 0.):
        return (0., 0.)
    products = [f32(x * y) for x in a for y in b]
    return widen(min(products), 1)[0], widen(max(products), 1)[1]


def residual(coefficients, offset, parameter, gap):
    r2, r3, r4, r5 = map(exact, coefficients)
    t = exact(parameter) if parameter else (0., 0.)
    z = sub((1., 1.), t)
    poly = add(r2, mul(z, add(r3, mul(t, add(r4, mul(z, r5))))))
    high = exact(gap[0]) if gap[0] else (0., 0.)
    low = add(exact(gap[1]), (-gap[2], gap[2]))
    return add(add(add(high, exact(offset)), mul(t, poly)), low)


class NormalResidualReview(unittest.TestCase):
    def test_bounded_scalar_specialization_encloses_independent_exact_quartic(self):
        generator = random.Random(4271)
        checked = 0
        cases = [([1e8, -1e8, 1e8, -1e8], 1e8, (-1e8, 1e-5, 1e-9)),
                 ([0., 0., 0., 0.], 0., (0., 0., 0.)),
                 ([1e-38, -1e-38, 1e-37, -1e-37], 1e-38, (-1e-38, 0., 0.)),
                 ([80., -.001, .0001, -.00001], 78.084671, (-78.084735, 1e-6, 1e-12))]
        for _ in range(160):
            magnitude = 10 ** generator.uniform(-37, 8)
            cs = [generator.uniform(-1, 1) * magnitude for _ in range(4)]
            offset = generator.uniform(-1, 1) * magnitude
            cases.append((cs, offset, (-offset, generator.uniform(-1, 1) * magnitude * 1e-7,
                                      generator.random() * magnitude * 1e-12)))
        for coefficients, offset, gap in cases:
            coefficients, offset, gap = list(map(f32, coefficients)), f32(offset), tuple(map(f32, gap))
            for t in [0., value(1), value(0x3effffff), .5, value(0x3f000001), .75, 1.]:
                actual = residual(coefficients, offset, t, gap)
                a, b, c, d = map(Q, coefficients)
                theta = Q(t)
                # Independent expanded power polynomial; no nested interval
                # operation or rounded intermediate is reused as the oracle.
                polynomial = (a + b) * theta + (c - b + d) * theta ** 2 + (-c - 2*d) * theta ** 3 + d * theta ** 4
                center = Q(gap[0]) + Q(offset) + polynomial + Q(gap[1])
                self.assertLessEqual(Q(actual[0]), center - Q(gap[2]))
                self.assertGreaterEqual(Q(actual[1]), center + Q(gap[2]))
                self.assertTrue(all(math.isfinite(x) and abs(x) < 1e9 for x in actual))
                checked += 1
        self.assertEqual(checked, 1148)

    def test_outer_domain_and_operation_order_match_reviewed_contract(self):
        source = Path(__file__).with_name('optical_normal_residual_fast.wgsl').read_text()
        self.assertIn('parameter>=0.0 && parameter<=1.0', source)
        self.assertIn('all(abs(coefficients)<=vec4(1e8))', source)
        self.assertIn('all(abs(boundary_gap)<=vec3(1e8))', source)
        self.assertIn('abs(start.q.x)<=1e8', source)
        self.assertIn('boundary_gap.z>=0.0', source)
        self.assertIn('return nc_residual(start,curve,parameter,boundary_gap);', source)
        self.assertIn('nc_interval_sterbenz(a.x,-b.x,lo)', source)
        self.assertIn('nc_interval_sterbenz(a.x,b.y,lo)', source)
        self.assertIn('nc_rf_add(nc_rf_add(nc_rf_add(high,nc_rf_exact(start.q.x)),nc_rf_mul(theta,poly)),low)', source)
        # Coarse positive envelope, allowing ample outward rounding at each
        # primitive, proves no removed overflow/validity guard can be reached
        # in this admitted domain. The exact endpoint tests above are tighter.
        z = Q(1) + Q(1, 2 ** 22)
        inflation = Q(1) + Q(1, 2 ** 20)
        c = Q(10 ** 8)
        inner = (c + z*c*inflation)*inflation
        inner = (c + inner*inflation)*inflation
        polynomial = (c + z*inner*inflation)*inflation
        total = ((2*c*inflation + polynomial*inflation)*inflation + 2*c*inflation)*inflation
        self.assertLess(total, 10 ** 9)


if __name__ == '__main__':
    unittest.main()
