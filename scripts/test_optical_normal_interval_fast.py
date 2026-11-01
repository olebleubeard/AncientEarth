"""Independent finite checks of the diagnostic interval fast paths."""
from fractions import Fraction as Q
from pathlib import Path
import unittest

from test_optical_normal_intervals import bits, value, widen, rational_ulp


class NormalFastIntervalReview(unittest.TestCase):
    def test_sign_selected_ratio_extrema_enclose_every_endpoint_and_allowed_rounding(self):
        checked = 0
        for a in [(-7., -2.), (-2., 0.), (-2., 3.), (0., 0.), (0., 3.), (2., 7.),
                  (value(0x3f7fffff), value(0x3f800001))]:
            for positive in [(1., 1.), (1., 2.), (.25, 8.),
                             (value(0x3f7fffff), value(0x3f800001))]:
                for sign in [-1, 1]:
                    b = positive if sign > 0 else (-positive[1], -positive[0])
                    for scale in [2. ** -60, 1., 2. ** 60]:
                        original_a = tuple(x * scale for x in a)
                        numerator = original_a if sign > 0 else (-original_a[1], -original_a[0])
                        denominator = b if sign > 0 else (-b[1], -b[0])
                        lower_d = denominator[0] if numerator[0] < 0 else denominator[1]
                        upper_d = denominator[1] if numerator[1] < 0 else denominator[0]
                        lower, upper = Q(numerator[0]) / Q(lower_d), Q(numerator[1]) / Q(upper_d)
                        # Oracle enumerates all four original signed ratios,
                        # independently of the sign-based selection rule.
                        endpoints = [Q(x) / Q(y) for x in original_a for y in b]
                        self.assertEqual(lower, min(endpoints))
                        self.assertEqual(upper, max(endpoints))
                        for exact in [lower, upper]:
                            nearest = bits(float(exact))
                            for raw in range(max(0, nearest - 8), nearest + 9):
                                candidate = value(raw)
                                if abs(Q(candidate) - exact) <= Q(5, 2) * rational_ulp(exact):
                                    lo, hi = widen(candidate, 8)
                                    self.assertLessEqual(Q(lo), exact)
                                    self.assertGreaterEqual(Q(hi), exact)
                                    checked += 1
        self.assertGreater(checked, 1000)

    def test_fast_domains_and_checked_fallback_contract(self):
        source = Path(__file__).with_name('optical_normal_interval_fast.wgsl').read_text()
        self.assertIn('NC_F_ADD_LIMIT:f32=0x1p120f', source)
        self.assertIn('NC_F_MUL_LIMIT:f32=0x1p60f', source)
        self.assertLess(Q(2 ** 120) + Q(2 ** 120), Q(3e37))
        self.assertLess(Q(2 ** 60) * Q(2 ** 60), Q(3e37))
        for name in ['add', 'sub', 'mul']:
            self.assertIn(f'return nc_interval_{name}(a,b);', source)
        self.assertIn('numerator=-a.yx;denominator=-b.yx', source)
        self.assertIn('nc_interval_quotient_safe(numerator.x,lower_denominator)', source)
        self.assertIn('nc_interval_quotient_safe(numerator.y,upper_denominator)', source)
        self.assertIn('nc_interval_down_steps(lo,8u)', source)
        self.assertIn('nc_interval_up_steps(hi,8u)', source)


if __name__ == '__main__':
    unittest.main()
