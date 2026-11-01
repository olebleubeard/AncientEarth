"""Exact encoded-curve bounds for the unused scalar endpoint metric."""
from fractions import Fraction as F
from pathlib import Path
import math
import struct
import unittest

from test_optical_height_bounds_fast import Scalar
from test_optical_normal_intervals import bits, value, widen, rational_ulp, MIN_NORMAL


def quotient_upper(a, b, side):
    exact = F(a)/F(b)
    middle = bits(float(exact))
    allowance = F(5, 2)*rational_ulp(exact)
    candidates = [value(raw) for raw in range(max(0, middle-8), middle+9)
                  if math.isfinite(value(raw)) and abs(F(value(raw))-exact) <= allowance]
    assert candidates
    observed = min(candidates) if side == 0 else max(candidates)
    if abs(observed) < MIN_NORMAL:
        observed = 0.
    upper = widen(observed, 8)[1]
    assert F(upper) >= exact
    return upper


def scalar_metric(coeff, optical, u, span, path_upper, height, side):
    op = Scalar(side)
    Q = op.sum4([op.l1(row[1:]) for row in coeff[:4]])
    U = op.sum4([op.l1(row) for row in coeff[4:]])
    lower = op.down(F(max(map(abs, u)))-F(U))
    if not lower > .25:
        return None
    radius = op.add(6399594., op.add(op.abs(height), span))
    physical = op.add(1., op.mul(radius, quotient_upper(Q, span, side)))
    numerator = op.mul(op.mul(2., path_upper), U)
    denominator = op.down(F(span)*F(lower))
    assert denominator > 0
    direction = quotient_upper(numerator, denominator, side)
    path_sum = op.sum4([op.abs(row[0]) for row in coeff[:4]])
    excess_sum = op.sum4([op.abs(x) for x in optical])
    path = quotient_upper(path_sum, span, side)
    optical_bound = quotient_upper(op.add(path_sum, excess_sum), span, side)
    return [physical, direction, path, optical_bound]


def exact_metric(coeff, optical, u, span, path_upper, height):
    Q = sum(abs(F(x)) for row in coeff[:4] for x in row[1:])
    U = sum(abs(F(x)) for row in coeff[4:] for x in row)
    lower = max(abs(F(x)) for x in u)-U
    radius = 6399594+abs(F(height))+F(span)
    path = sum(abs(F(row[0])) for row in coeff[:4])
    excess = sum(abs(F(x)) for x in optical)
    return [1+radius*Q/F(span), 2*F(path_upper)*U/(F(span)*lower),
            path/F(span), (path+excess)/F(span)]


class HeightMetricFastReview(unittest.TestCase):
    def test_scalar_components_enclose_exact_formulas_and_division_allowance(self):
        checked = 0
        for scale in [2.**-100, 2.**-20, 1., 2.**20]:
            for span in [value(bits(1e-9)), .125, 1., 1e8]:
                for height in [-1e8, -20., 0., 1e8]:
                    # Stronger direction coefficients would intentionally
                    # decline, so keep their whole-curve norm safely positive.
                    coeff = [[scale, .25*scale, -.125*scale],
                             [-.5*scale, -.0625*scale, .125*scale],
                             [.125*scale, .03125*scale, 0.],
                             [-.0625*scale, 0., .0625*scale]]
                    coeff += [[0., .0625, 0.], [.015625, 0., 0.],
                              [0., 0., -.03125], [-.015625, 0., 0.]]
                    excess = [scale/16., -scale/32., scale/64., -scale/128.]
                    truth = exact_metric(coeff, excess, [0., 1., 0.], span, 1e8, height)
                    for side in [0, 1]:
                        observed = scalar_metric(coeff, excess, [0., 1., 0.], span, 1e8, height, side)
                        self.assertIsNotNone(observed)
                        for actual, expected in zip(observed, truth):
                            self.assertGreaterEqual(F(actual), expected)
                            self.assertLess(actual, 3e37)
                        checked += 1
        self.assertEqual(checked, 128)
        # Tiny encoded coefficients retain the FTZ allowance, while all-zero
        # curves still bound the original physical-height derivative1.
        for tiny in [0., value(1), value(0x007fffff)]:
            coeff = [[tiny, -tiny, tiny] for _ in range(8)]
            for side in [0, 1]:
                got = scalar_metric(coeff, [tiny]*4, [0., 1., 0.], .125, 1., 0., side)
                for a, b in zip(got, exact_metric(coeff, [tiny]*4, [0., 1., 0.], .125, 1., 0.)):
                    self.assertGreaterEqual(F(a), b)

    def test_domain_denominators_and_single_reference_fallback(self):
        # Bounds include slack for every admitted rounded constant and operation.
        self.assertGreater(F(249, 10**12), F(1, 2**126))
        self.assertLess(F(21*10**7)*F(9*10**8)/F(99, 10**11), 2*10**26)
        self.assertLess(F(2)*F(111*10**8)*F(2)/F(249, 10**12), 2*10**20)
        self.assertLess(F(82*10**7)/F(99, 10**11), 9*10**17)
        source = Path(__file__).with_name('optical_height_metric_fast.wgsl').read_text()
        self.assertEqual(source.count('return nh_state_per_height(start,curve);'), 1)
        attempt, wrapper = source.split('fn nh_state_per_height_fast(', 1)
        self.assertEqual(attempt.count('return nc_interval_invalid();'), 4)
        self.assertNotIn('return nc_interval_invalid', wrapper)
        self.assertIn('if nc_interval_valid(candidate){return candidate;}', wrapper)
        self.assertIn('nc_interval_down(span*norm_lower)', source)
        self.assertIn('span>=1.0e-9 && span<=1.0e8', source)
        # Explicit norm uncertainty prevents a shortcut success, rather than
        # substituting a larger denominator or clamping to a positive value.
        coeff = [[0., 0., 0.] for _ in range(8)]
        coeff[4][1] = .875
        self.assertIsNone(scalar_metric(coeff, [0.]*4, [0., 1., 0.], 1., 1., 0., 0))

    def test_actual620_returned_curves_against_exact_metric(self):
        root = Path(__file__).parent.parent/'artifacts'
        input_path = root/'optical-height-trial-cost-fast-single-fallback-inputs.bin'
        detail_path = root/'optical-height-trial-cost-fast-single-fallback-detail.bin'
        if not input_path.exists() or not detail_path.exists():
            self.skipTest('supplementary retained workload unavailable')
        inputs = input_path.read_bytes();details = detail_path.read_bytes()
        self.assertEqual(len(inputs), 620*108*4)
        self.assertEqual(len(details), 620*164*4)
        checked = 0
        for i in range(620):
            initial = struct.unpack_from('<108f', inputs, i*108*4)
            detail = struct.unpack_from('<164f', details, i*164*4)
            rows = [detail[k:k+4] for k in range(40, 80, 4)]
            coeff = [row[:3] for row in rows[:8]]
            optical = rows[8];u = initial[4:7];span = abs(initial[3])
            # Exact represented input to the metric, matching only the two
            # binary32 height additions; canonical field accuracy is separate.
            height = value(bits(initial[8]+value(bits(initial[9]+initial[0]))))
            path = rows[9][3]
            truth = exact_metric(coeff, optical, u, span, path, height)
            for side in [0, 1]:
                got = scalar_metric(coeff, optical, u, span, path, height, side)
                self.assertIsNotNone(got)
                for actual, expected in zip(got, truth):
                    self.assertGreaterEqual(F(actual), expected)
                checked += 1
        self.assertEqual(checked, 1240)


if __name__ == '__main__':
    unittest.main()
