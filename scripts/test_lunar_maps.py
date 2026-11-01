"""Finite analytic tests of the lunar map preprocessing, without source assets."""
import tempfile
import unittest
from pathlib import Path

import numpy as np

from prepare_lunar_maps import RADIUS_M, normals, write_texture
from verify_lunar_maps import load_texture


class LunarMapTests(unittest.TestCase):
    def test_flat_and_smooth_geographic_surface_gradients_include_seam_and_poles(self):
        flat = normals(np.full((8, 16), -1000.0))
        self.assertTrue(np.array_equal(flat, np.broadcast_to([0, 0, 1], flat.shape)))
        # A smooth Cartesian-coordinate height has no longitude discontinuity.
        # Symmetric angular differences of its sine/cosine terms have the exact
        # sinc attenuation, including the geographic polar continuation.
        rows, columns = 128, 256
        d = np.pi / rows
        phi = np.pi / 2 - (np.arange(rows)[:, None] + .5) * d
        longitude = -np.pi + (np.arange(columns)[None, :] + .5) * d
        height = 8000 * np.cos(phi) * np.cos(longitude) + 3000 * np.sin(phi)
        hl = -8000 * np.cos(phi) * np.sin(longitude) * np.sin(d) / d
        hp = (-8000 * np.sin(phi) * np.cos(longitude) + 3000 * np.cos(phi)) * np.sin(d) / d
        n = np.stack(np.broadcast_arrays(np.cos(phi)*np.cos(longitude), np.cos(phi)*np.sin(longitude), np.sin(phi)), axis=2)
        east = np.stack(np.broadcast_arrays(-np.sin(longitude), np.cos(longitude), np.zeros_like(phi)), axis=2)
        north = np.stack(np.broadcast_arrays(-np.sin(phi)*np.cos(longitude), -np.sin(phi)*np.sin(longitude), np.cos(phi)), axis=2)
        radius = RADIUS_M + height
        a = n*hl[:, :, None] + east*(radius*np.cos(phi))[:, :, None]
        b = n*hp[:, :, None] + north*radius[:, :, None]
        expected = np.cross(a, b)
        expected /= np.linalg.norm(expected, axis=2, keepdims=True)
        result = normals(height)
        actual = east*result[:, :, :1] + north*result[:, :, 1:2] + n*result[:, :, 2:3]
        self.assertLess(float(np.max(np.linalg.norm(actual-expected, axis=2))), 6e-8)
        for value in [np.empty((0, 0)), np.zeros((4, 4)), np.full((2, 4), np.nan), np.full((2, 4), -RADIUS_M)]:
            with self.assertRaises(ValueError):
                normals(value)

    def test_color_mips_average_linear_light_and_keep_full_resolution_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "color.ael"
            linear = np.zeros((2, 4, 3), np.float32)
            linear[:, 1::2] = 1
            write_texture(path, linear, True)
            levels, srgb = load_texture(path)
            self.assertEqual(srgb, 1)
            self.assertTrue(np.all(levels[0][:, ::2, :3] == 0))
            self.assertTrue(np.all(levels[0][:, 1::2, :3] == 255))
            # Half linear white is sRGB 187.516... -> 188, not code-space 128.
            self.assertTrue(np.all(levels[1][:, :, :3] == 188))
            self.assertTrue(np.all(levels[2][:, :, :3] == 188))

    def test_vector_mips_renormalize_and_reject_nonfinite_fields(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "normal.ael"
            field = np.empty((2, 4, 3), np.float32)
            field[:, ::2] = [.6, 0, .8]
            field[:, 1::2] = [-.6, 0, .8]
            write_texture(path, field, False)
            levels, srgb = load_texture(path)
            self.assertEqual(srgb, 0)
            for level in levels[1:]:
                self.assertTrue(np.all(level[:, :, :3] == [128, 128, 255]))
            for invalid in [np.zeros((2, 2)), np.empty((0, 1, 3)), np.full((1, 1, 3), np.inf)]:
                with self.assertRaises(ValueError):
                    write_texture(path, invalid, False)


if __name__ == "__main__":
    unittest.main()
