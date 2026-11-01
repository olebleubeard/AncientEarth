"""Exact rational finite fixtures for two independently rounded divisions.

WGSL2026-10-21 §15.7.4 defines ULP as the smallest bracketing spacing and
permits division error2.5 ULP. Eight adjacent steps enclose BOTH permitted
results, not just the exact quotient. This finite enumeration supplements
the binade-spacing proof; it does not simulate a GPU/compiler.
"""
from fractions import Fraction as Q
import math
import struct
import unittest


def value(bits):
    return Q(struct.unpack('<f', struct.pack('<I', bits))[0])


def ulp(q):
    if q <= value(0x00800000):
        return Q(1, 2**149)
    e = q.numerator.bit_length() - q.denominator.bit_length()
    if q < Q(2)**e:
        e -= 1
    # The exact binade endpoint's smaller bracketing spacing is below it.
    if q == Q(2)**e:
        e -= 1
    return Q(2)**(e-23)


class IndependentQuotientReview(unittest.TestCase):
    def test_every_pair_of_allowed_normal_results_at_binade_boundaries(self):
        maximum = 0
        # Upper and lower binade neighbors at every normal exponent used by
        # the admitted nonzero distance/step range, plus wider finite scales.
        for exponent in [-125, -100, -24, -20, -1, 0, 1, 40, 100, 125]:
            boundary = (exponent + 127) << 23
            spacing = value(boundary + 1) - value(boundary)
            for quarter in range(-24, 25):
                q = value(boundary) + quarter * spacing / 4
                allowed = [b for b in range(boundary-32, boundary+33)
                           if abs(value(b)-q) <= Q(5, 2)*ulp(q)]
                self.assertTrue(allowed)
                # Every possible first result must bracket every second one.
                for first in allowed:
                    for second in allowed:
                        self.assertLessEqual(abs(first-second), 8)
                maximum = max(maximum, max(allowed)-min(allowed))
        self.assertEqual(maximum, 7)

    def test_min_normal_flush_boundary_enclosure(self):
        boundary = 0x00800000
        tiny_bound = 2*value(boundary)
        h = Q(1, 2**149)
        for quarter in range(-40, 41):
            q = value(boundary) + quarter*h/4
            allowed_bits = [b for b in range(boundary-20, boundary+21)
                            if abs(value(b)-q) <= Q(5, 2)*ulp(q)]
            allowed_values = [value(b) for b in allowed_bits]
            if any(b < boundary for b in allowed_bits):
                allowed_values.append(Q(0))  # Allowed flush after division.
            for first in allowed_bits:
                if first <= boundary+8:
                    low, high = -tiny_bound, tiny_bound
                else:
                    low, high = value(first-8), value(first+8)
                for second in allowed_values:
                    self.assertLessEqual(low, second)
                    self.assertGreaterEqual(high, second)
            if any(b < boundary for b in allowed_bits):
                self.assertTrue(all(-tiny_bound <= y <= tiny_bound for y in allowed_values))


if __name__ == '__main__':
    unittest.main()
