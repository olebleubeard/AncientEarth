"""Finite exact-rational checks of the diagnostic WGSL interval contract."""
from fractions import Fraction
import math
from pathlib import Path
import struct
import unittest


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def value(x):
    return struct.unpack('<f', struct.pack('<I', x))[0]


MIN_NORMAL = value(0x00800000)
TINY_BOUND = value(0x01000000)


def widen(x, steps):
    raw = bits(x)
    if raw & 0x7fffffff <= 0x00800000 + steps:
        return -TINY_BOUND, TINY_BOUND
    if raw & 0x80000000:
        return value(raw + steps), value(raw - steps)
    return value(raw - steps), value(raw + steps)


def rational_ulp(x):
    x = abs(x)
    if not x:
        return Fraction(1, 2 ** 149)
    exponent = x.numerator.bit_length() - x.denominator.bit_length()
    power = Fraction(2 ** exponent) if exponent >= 0 else Fraction(1, 2 ** -exponent)
    if x < power:
        exponent -= 1
    # At an exactly represented power of two, WGSL uses the smaller gap.
    if x == power:
        exponent -= 1
    shift = max(-149, exponent - 23)
    return Fraction(2 ** shift) if shift >= 0 else Fraction(1, 2 ** -shift)


def sterbenz_guard(a, b, observed):
    if a == b:
        return True
    return ((a > 0 and b > 0) or (a < 0 and b < 0)) and (
        min(abs(a), abs(b)) >= MIN_NORMAL
        and max(abs(a), abs(b)) <= 2 * min(abs(a), abs(b))
        and abs(observed) >= MIN_NORMAL
    )


class NormalIntervalReview(unittest.TestCase):
    def test_one_neighbor_encloses_both_permitted_basic_rounding_directions(self):
        # WGSL basic +,-,* require either adjacent correctly-rounded result.
        # Enumerate both choices using rational order, never binary64's rounded
        # arithmetic as the reference. This supports a narrower diagnostic
        # enclosure; division deliberately keeps its separate eight-step test.
        inputs = [value((e << 23) | m) for e in [1, 2, 77, 126, 127, 128, 227, 247]
                  for m in [0, 1, 0x3fffff, 0x7fffff]]
        exact_results = []
        for a in inputs:
            for b in inputs:
                for sign in [-1, 1]:
                    aa, bb = Fraction(a), sign * Fraction(b)
                    exact_results.extend([aa + bb, aa - bb])
                    if abs(aa * bb) < 2 ** 126:
                        exact_results.append(aa * bb)
        checked = 0
        for exact in exact_results:
            nearest = bits(float(exact))
            candidates = [value(raw) for raw in range(max(0, nearest - 2), nearest + 3)
                          if raw <= 0xffffffff and math.isfinite(value(raw))]
            candidates.extend([0., -0.])
            lower = max(x for x in candidates if Fraction(x) <= exact)
            upper = min(x for x in candidates if Fraction(x) >= exact)
            for rounded in [lower, upper]:
                observed = [rounded]
                if abs(rounded) < MIN_NORMAL:
                    observed.append(math.copysign(0., rounded))
                for result in observed:
                    lo, hi = widen(result, 1)
                    self.assertLessEqual(Fraction(lo), exact)
                    self.assertGreaterEqual(Fraction(hi), exact)
                    checked += 1
        self.assertGreater(checked, 10000)

    def test_sterbenz_endpoint_shortcut_has_exact_rational_result_or_retains_ftz_band(self):
        source = Path(__file__).with_name('optical_normal_interval.wgsl').read_text()
        self.assertIn('if a==b{return true;}', source)
        self.assertIn('max(aa,bb)<=2.0*min(aa,bb)', source)
        self.assertIn('abs(difference)>=NC_I_MIN_NORMAL', source)
        self.assertIn('nc_interval_sterbenz(a.x,-b.x,lo)', source)
        self.assertIn('nc_interval_sterbenz(a.x,b.y,lo)', source)
        certified = flushed = outside = 0
        # Prescribed operands straddle binades and the exact factor-of-two
        # boundary. Fraction, rather than another rounded subtraction, is the
        # reference. The proof applies separately to each interval endpoint.
        for exponent in [-126, -125, -124, -100, -10, 0, 1, 60, 120]:
            raw = (exponent + 127) << 23
            positive = [value(raw + delta) for delta in
                        [0, 1, 2, 0x3fffff, 0x7ffffe, 0x7fffff,
                         0x800000, 0x800001]]
            for sign_a in [-1, 1]:
                for sign_b in [-1, 1]:
                    for aa in positive:
                        for bb in positive:
                            a, b = sign_a * aa, sign_b * bb
                            exact = Fraction(a) - Fraction(b)
                            observed = value(bits(float(exact)))
                            observations = [observed]
                            if 0 < abs(observed) < MIN_NORMAL:
                                observations.append(math.copysign(0., observed))
                            for result in observations:
                                if sterbenz_guard(a, b, result):
                                    self.assertEqual(Fraction(result), exact)
                                    certified += 1
                                elif result == 0 and a != b:
                                    lo, hi = widen(result, 4)
                                    self.assertLessEqual(Fraction(lo), exact)
                                    self.assertGreaterEqual(Fraction(hi), exact)
                                    flushed += 1
                                elif a * b < 0 or max(abs(a), abs(b)) > 2 * min(abs(a), abs(b)):
                                    outside += 1
                            # Opposite-sign addition uses exactly the same
                            # lemma after negating its second operand.
                            self.assertEqual(Fraction(a) + Fraction(-b), exact)
        self.assertGreater(certified, 500)
        self.assertGreater(flushed, 20)
        self.assertGreater(outside, 500)
        # Equal operands prove zero even if tiny; merely observing zero from
        # unequal normal inputs cannot prove that identity under WGSL FTZ.
        a, b = value(0x00800001), value(0x00800000)
        self.assertFalse(sterbenz_guard(a, b, 0.))
        self.assertTrue(sterbenz_guard(a, a, 0.))

    def test_representable_widening_contains_allowed_results_across_binades(self):
        source = Path(__file__).with_name('optical_normal_interval.wgsl').read_text()
        self.assertIn('nc_interval_down_steps(lo,8u)', source)
        self.assertIn('nc_interval_up_steps(hi,8u)', source)
        self.assertIn('nc_interval_down_steps(x,1u)', source)
        self.assertIn('nc_interval_up_steps(x,1u)', source)
        checked = 0
        # Exact real operation results on both sides of powers of two, including
        # values between representable outputs. This tests the specification's
        # allowance independently of the machine's particular division rounding.
        for exponent in [-126, -100, -10, 0, 1, 60, 120]:
            center = Fraction(2 ** exponent) if exponent >= 0 else Fraction(1, 2 ** -exponent)
            spacing = center / (2 ** 23)
            for offset in [Fraction(k, 8) for k in range(-32, 33)]:
                for sign in [-1, 1]:
                    exact = sign * (center + offset * spacing)
                    nearest = bits(float(exact))
                    for raw in range(nearest - 8, nearest + 9):
                        candidate = value(raw)
                        if not math.isfinite(candidate):
                            continue
                        if abs(Fraction(candidate) - exact) <= Fraction(5, 2) * rational_ulp(exact):
                            lo, hi = widen(candidate, 8)
                            self.assertLessEqual(Fraction(lo), exact)
                            self.assertGreaterEqual(Fraction(hi), exact)
                            checked += 1
        self.assertGreater(checked, 4000)
        # A potentially flushed exact subnormal result is enclosed as well.
        for raw in [0, 1, 0x007fffff, 0x00800000, 0x80000000, 0x80000001, 0x807fffff]:
            x = value(raw)
            for observed in [x, math.copysign(0., x)]:
                lo, hi = widen(observed, 4)
                self.assertLessEqual(Fraction(lo), Fraction(x))
                self.assertGreaterEqual(Fraction(hi), Fraction(x))

    def test_exponent_preguards_prevent_overflow_and_restrict_division_domain(self):
        source = Path(__file__).with_name('optical_normal_interval.wgsl').read_text()
        self.assertIn('ea+eb<=378u', source)
        self.assertIn('ea-eb<=125', source)
        self.assertIn('mb<0x00800000u || mb>0x7e800000u', source)
        self.assertIn('if abs(x)<NC_I_MIN_NORMAL', source)
        magnitudes = [value((e << 23) | mantissa) for e in [1, 2, 60, 126, 127, 128, 200, 250]
                      for mantissa in [0, 1, 0x7fffff]]
        for a in magnitudes:
            for b in magnitudes:
                ea, eb = bits(a) >> 23, bits(b) >> 23
                if ea + eb <= 378:
                    self.assertLess(Fraction(a) * Fraction(b), Fraction(2 ** 126))
                if ea - eb <= 125:
                    self.assertLess(Fraction(a) / Fraction(b), Fraction(2 ** 126))
        self.assertLess(bits(value(0x007fffff)), 0x00800000)
        self.assertGreater(bits(value(0x7e800001)), 0x7e800000)
        # Public zero is deliberately widened; only explicitly authored/proved
        # zero may bypass the allowance, needed for exact tangent identities.
        self.assertEqual(widen(0., 4), (-TINY_BOUND, TINY_BOUND))


if __name__ == '__main__':
    unittest.main()
