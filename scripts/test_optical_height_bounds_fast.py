"""Finite checks of the diagnostic one-sided height certificate.

Fraction is the oracle for encoded coefficients and arithmetic. Both allowed
adjacent basic rounding choices are tested; these are not RN-only tests. The
captured workload is supplementary finite evidence, not complete ray coverage.
"""
from decimal import Decimal, localcontext
from fractions import Fraction as F
import math
from pathlib import Path
import struct
import unittest

from test_optical_normal_intervals import bits, value, widen, MIN_NORMAL, TINY_BOUND


def adjacent(exact):
    nearest = bits(float(exact))
    candidates = [value(raw) for raw in range(max(0, nearest-2), min(0xffffffff, nearest+2)+1)
                  if math.isfinite(value(raw))]
    candidates += [0., -0.]
    return max(x for x in candidates if F(x) <= exact), min(x for x in candidates if F(x) >= exact)


class Scalar:
    """Basic arithmetic can consistently select either adjacent permitted value."""
    def __init__(self, side):
        self.side = side

    def up(self, exact, steps=1):
        observed = adjacent(exact)[self.side]
        if abs(observed) < MIN_NORMAL:
            observed = math.copysign(0., observed)
        return widen(observed, steps)[1]

    def down(self, exact):
        observed = adjacent(exact)[self.side]
        if abs(observed) < MIN_NORMAL:
            observed = math.copysign(0., observed)
        return widen(observed, 1)[0]

    def abs(self, x):
        return abs(x) if abs(x) >= MIN_NORMAL else TINY_BOUND

    def add(self, a, b):
        return self.up(F(a)+F(b))

    def mul(self, a, b):
        return self.up(F(a)*F(b))

    def l1(self, v):
        assert len(v) in [2, 3]
        result = self.add(self.abs(v[0]), self.abs(v[1]))
        return result if len(v) == 2 else self.add(result, self.abs(v[2]))

    def sum4(self, values):
        return self.add(self.add(values[0], values[1]), self.add(values[2], values[3]))


def true_vertical_interval(q, u, orientation):
    # Exact encoded dot and squared norm; Decimal's correctly rounded sqrt is
    # enclosed by its two neighbors before the rational final division.
    a, b = map(F, q[1:])
    numerator = orientation*(F(u[0])*a + F(u[1]) - F(u[2])*b)
    norm2 = 1+a*a+b*b
    with localcontext() as ctx:
        ctx.prec = 320
        decimal_norm2 = Decimal(norm2.numerator)/Decimal(norm2.denominator)
        root = decimal_norm2.sqrt()
        lo, hi = F(root.next_minus()), F(root.next_plus())
    assert lo*lo <= norm2 <= hi*hi
    return sorted([numerator/lo, numerator/hi])


def certificate(q, u, step, coeff, eq, eu, side):
    """Scalar transcription; returns None for sufficient-certificate deferral."""
    op = Scalar(side)
    qs, us = coeff[:4], coeff[4:8]
    Q = op.sum4([op.l1(x[1:]) for x in qs])
    U = op.sum4([op.l1(x) for x in us])
    initial = op.l1(u)
    vert_error = op.add(U, op.mul(initial, Q))
    # The source uses an allowed inverseSqrt/dot evaluation. This finite test
    # supplies a rounded mathematical result and checks the unchanged 32u
    # allowance against an independently enclosed exact chart normal.
    vlo, vhi = true_vertical_interval(q, u, 1 if step > 0 else -1)
    signed = value(bits(float((vlo+vhi)/2)))
    lower = op.down(F(signed)-F(op.add(vert_error, op.mul(2**-19, initial))))
    chart_extra = op.add(op.mul(op.l1(qs[3][1:]), .0625), op.l1(eq[1:]))
    chart = op.add(op.l1(q[1:]), op.add(Q, chart_extra))
    extra = op.add(op.mul(op.l1(us[3]), .0625), op.l1(eu))
    m = op.down(F(max(map(abs, u)))-F(op.add(U, extra)))
    rest = op.add(op.add(op.abs(qs[1][0]), op.abs(qs[2][0])), op.abs(qs[3][0]))
    if not (lower > .01 and chart <= 1 and m > .25 and qs[0][0] >= MIN_NORMAL and qs[0][0] > rest):
        return None
    path = max(op.sum4([op.abs(x[0]) for x in qs]), op.up(F(abs(step))/F(lower), 8))
    return {'Q': Q, 'U': U, 'vertical': lower, 'chart': chart, 'normal': m,
            'path': path, 'chronology_rest': rest, 'true_vertical': vlo}


def exact_checks(test, q, u, step, coeff, eq, eu, got):
    qs, us = coeff[:4], coeff[4:8]
    Q = sum(abs(F(x)) for row in qs for x in row[1:])
    U = sum(abs(F(x)) for row in us for x in row)
    unorm = sum(abs(F(x)) for x in u)
    chart_extra = sum(abs(F(x)) for x in qs[3][1:])/16 + sum(abs(F(x)) for x in eq[1:])
    dir_extra = sum(abs(F(x)) for x in us[3])/16 + sum(abs(F(x)) for x in eu)
    test.assertGreaterEqual(F(got['Q']), Q)
    test.assertGreaterEqual(F(got['U']), U)
    test.assertGreaterEqual(F(got['chart']), sum(abs(F(x)) for x in q[1:])+Q+chart_extra)
    test.assertLessEqual(F(got['vertical']), got['true_vertical']-U-unorm*Q)
    test.assertLessEqual(F(got['normal']), max(abs(F(x)) for x in u)-U-dir_extra)
    test.assertGreaterEqual(F(got['path']), sum(abs(F(row[0])) for row in qs))
    test.assertGreaterEqual(F(got['path']), F(abs(step))/F(got['vertical']))
    test.assertGreater(F(qs[0][0]), sum(abs(F(row[0])) for row in qs[1:]))
    # Separate expanded power derivative, not the nested dense evaluator.
    p2, p3, p4, p5 = [F(row[0]) for row in qs]
    coefficients = [p2+p3, 2*(p4-p3+p5), 3*(-p4-2*p5), 4*p5]
    for numerator in range(33):
        t = F(numerator, 32)
        derivative = sum(c*t**i for i, c in enumerate(coefficients))
        test.assertGreater(derivative, 0)


class HeightBoundsFastReview(unittest.TestCase):
    def test_one_sided_operations_cover_both_basic_rounding_and_ftz(self):
        inputs = [0., value(1), value(0x007fffff), MIN_NORMAL, value(0x00800001)]
        inputs += [value(((e+127)<<23)+delta) for e in [-70, -2, 0, 20, 26]
                   for delta in [0, 1, 0x3fffff, 0x7fffff]]
        count = 0
        for side in [0, 1]:
            op = Scalar(side)
            for a in inputs:
                for b in inputs:
                    aa, bb = op.abs(a), op.abs(b)
                    self.assertGreaterEqual(F(op.add(aa, bb)), F(a)+F(b))
                    self.assertGreaterEqual(F(op.mul(aa, bb)), F(a)*F(b))
                    self.assertLessEqual(F(op.down(F(a)-F(b))), F(a)-F(b))
                    count += 1
        self.assertGreater(count, 1000)

    def test_sufficient_chronology_and_fallback_cover_signs_and_cancellation(self):
        # Factor checks are independent exact polynomial identities/bounds:
        # |1-2t|<=1; 2t-3t^2 in[-1,1/3]; the cubic is 2t(1-t)(1-2t),
        # whose magnitude <=1/2. The implementation deliberately uses1.
        for i in range(257):
            t = F(i, 256)
            self.assertLessEqual(abs(1-2*t), 1)
            self.assertLessEqual(abs(2*t-3*t*t), 1)
            self.assertLessEqual(abs(2*t-6*t*t+4*t**3), F(1, 2))
        accepted = deferred = 0
        for side in [0, 1]:
            for sign in [-1, 1]:
                for p3, p4, p5 in [(0., 0., 0.), (2., -3., 1.), (-2., 3., -1.),
                                    (8., 1., 1.), (11., 0., 0.)]:
                    coeff = [[10., 0., 0.], [p3, 0., 0.], [p4, 0., 0.], [p5, 0., 0.]]
                    coeff += [[0., 0., 0.]]*4
                    q, u, eq, eu = [0., 0., 0.], [0., float(sign), 0.], [0.]*3, [0.]*3
                    got = certificate(q, u, sign*5., coeff, eq, eu, side)
                    if got:
                        exact_checks(self, q, u, sign*5., coeff, eq, eu, got)
                        accepted += 1
                    else:
                        deferred += 1
        self.assertEqual((accepted, deferred), (12, 8))
        # Tangent/wrong direction, chart-domain overflow and norm uncertainty
        # do not produce a shortcut certificate. They retain original logic.
        simple = [[10., 0., 0.]] + [[0., 0., 0.] for _ in range(7)]
        for u in [[1., 0., 0.], [0., -1., 0.]]:
            self.assertIsNone(certificate([0., 0., 0.], u, 5., simple, eq, eu, 0))
        self.assertIsNone(certificate([0., 1.01, 0.], [0., 1., 0.], 5., simple, eq, eu, 0))
        varied = [row[:] for row in simple]
        varied[4][1] = .8
        self.assertIsNone(certificate([0., 0., 0.], [0., 1., 0.], 5., varied, eq, eu, 0))
        source = Path(__file__).with_name('optical_height_bounds_fast.wgsl').read_text()
        self.assertEqual(source.count('return nh_bounds(start,step,stage);'), 1)
        attempt, wrapper = source.split('fn nh_bounds_fast(', 1)
        self.assertEqual(attempt.count('return nh_invalid();'), 8)
        self.assertIn('let candidate=nh_try_bounds_fast(start,step,stage);', wrapper)
        self.assertIn('if candidate.data.valid==1u{return candidate;}', wrapper)
        self.assertNotIn('return nh_invalid', wrapper)
        self.assertIn('q2.x>chronology_rest', source)
        self.assertIn('nc_interval_up_steps(abs(step)/vertical_lower,8u)', source)

    def test_admitted_magnitudes_prevent_overflow_and_preserve_estimator(self):
        # Larger rational bounds than accumulated one-neighbor factors; at
        # most32 positive operations before each gate, so (1+2^-20)^32<1.001.
        slack = (1+F(1, 2**20))**32
        self.assertLess(slack, F(1001, 1000))
        Q, U, initial = 8*10**8*slack, 12*10**8*slack, 6*slack
        self.assertLess(Q, 9*10**8)
        self.assertLess(U, 13*10**8)
        self.assertLess((U+initial*Q)*slack, 8*10**9)
        self.assertLess((10**8/F(1,100))*slack, 11*10**9)
        self.assertLess(2*(11*10**9)*(18*10**7)/F(1,4)*slack, 2*10**19)
        self.assertLess(3*10**16*slack, 31*10**15)
        original = Path(__file__).with_name('optical_height_step.wgsl').read_text()
        fast = Path(__file__).with_name('optical_height_bounds_fast.wgsl').read_text()
        for prefix in ['let radius=', 'let embedded=', 'let interior=']:
            line = next(line.strip() for line in original.splitlines() if line.strip().startswith(prefix))
            self.assertIn(line, fast)
        self.assertIn('all(abs(nc_anchor)<=vec2(1.0e8))', fast)
        self.assertIn('abs(step)>=1.0e-9 && abs(step)<=1.0e8', fast)

    def test_retained_actual_coefficients_against_exact_real_certificate(self):
        path = Path(__file__).parent.parent/'artifacts/optical-height-trial-cost-first-inputs.bin'
        if not path.exists():
            self.skipTest('supplementary retained workload is unavailable')
        raw = path.read_bytes()
        self.assertEqual(len(raw), 620*108*4)
        count = 0
        for offset in range(0, len(raw), 108*4):
            words = struct.unpack('<108f', raw[offset:offset+108*4])
            rows = [words[i:i+4] for i in range(0, 108, 4)]
            q, u, step = rows[0][:3], rows[1][:3], rows[0][3]
            coeff = [row[:3] for row in rows[13:21]]
            eq, eu = rows[22][:3], rows[23][:3]
            for side in [0, 1]:
                got = certificate(q, u, step, coeff, eq, eu, side)
                if got:
                    exact_checks(self, q, u, step, coeff, eq, eu, got)
                    count += 1
        self.assertGreater(count, 1000)
        print(f'retained scalar certificates: {count}/1240 (two rounding choices per620 trials)')


if __name__ == '__main__':
    unittest.main()
