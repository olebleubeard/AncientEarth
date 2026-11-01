"""Exhaustive source-level parity for the array-free optical number plate.

This checks integer glyph lookup, not native compiler execution or rendering.
"""
import re
import unittest
from pathlib import Path


class OpticalSignRowsTests(unittest.TestCase):
    def test_original_glyph_pixels_and_decimal_places_are_preserved(self):
        # Original authored 5x7 glyphs, retained independently as visible rows.
        glyphs = [
            ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
            ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
            ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
            ["11110", "00001", "00001", "01110", "00001", "00001", "11110"],
            ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
            ["11111", "10000", "10000", "11110", "00001", "00001", "11110"],
            ["01110", "10000", "10000", "11110", "10001", "10001", "01110"],
            ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
            ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
            ["01110", "10001", "10001", "01111", "00001", "00001", "01110"],
        ]
        source = (Path(__file__).resolve().parents[1] /
                  "src/desktop/shaders/refraction_scene.wesl").read_text()
        row_source = source.split("fn sign_row(", 1)[1].split("fn sign_divisor(", 1)[0]
        cases = re.findall(r"case (\d+)u:\{rows=(\d+)u;last=(\d+)u;\}", row_source)
        self.assertEqual(len(cases), 10)
        self.assertEqual(sorted(int(d) for d, _, _ in cases), list(range(10)))
        self.assertIn("if row==6u{return last;}", row_source)
        self.assertIn("if row>=7u{return 0u;}", row_source)
        self.assertIn("return (rows>>(row*5u))&31u;", row_source)
        for digit, packed, last in cases:
            digit, packed, last = int(digit), int(packed), int(last)
            self.assertLess(packed, 2**30)
            for row in range(7):
                mask = last if row == 6 else (packed >> (5 * row)) & 31
                self.assertEqual(mask, int(glyphs[digit][row], 2))
                for cell in range(5):
                    self.assertEqual(bool(mask & (1 << (4 - cell))), glyphs[digit][row][cell] == "1")
        divisor_source = source.split("fn sign_divisor(", 1)[1].split("fn sign_radiance(", 1)[0]
        divisors = {int(d): int(v) for d, v in re.findall(r"case (\d+)u:\{return (\d+)u;\}", divisor_source)}
        self.assertEqual(divisors, {0: 1000, 1: 100, 2: 10})
        self.assertIn("default:{return 1u;}", divisor_source)
        for number in range(10000):
            text = f"{number:04d}"
            for digit in range(4):
                self.assertEqual(number // divisors.get(digit, 1) % 10, int(text[digit]))
        radiance = source.split("fn sign_radiance(", 1)[1].split("fn marker_radiance(", 1)[0]
        self.assertIn("if digit>=4u||cell>=5u{return vec3(0.0);}", radiance)
        self.assertIn("sign_row(value,row-1u)&(1u<<(4u-cell))", radiance)


if __name__ == "__main__":
    unittest.main()
