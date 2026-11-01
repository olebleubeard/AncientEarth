"""Independent GDAL/full-byte and Cartesian-normal audit of NASA lunar maps.

Does not import the production preprocessor. Original TIFFs and derived payloads
are hash pinned. Every level-zero RGB byte and every normal texel are checked;
lower mip levels receive format/finite-unit-vector checks, not an independent
reproduction of Pillow's resampling. No radiometric or positional accuracy claim.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import struct

import numpy as np
import rasterio

ROOT = Path(__file__).resolve().parents[1]
PINNED = {
    ".cache/lunar/lroc_color_poles_8k.tif": "4af8b0cd4d50c30851359d98e7e72040240dd8d03256b58b345b5b76e9edb4ef",
    ".cache/lunar/ldem_16_uint.tif": "45a2b32d56e81ed30db07fead8abc842b249b6511219d9ca2c53f81bc2dc5d62",
    "assets/lunar/color.ael": "12e8065af6587222e6933faae21ee3dc8f745786e450a47c08d114c3f36a7917",
    "assets/lunar/normal.ael": "21f398102477c4786c532c0cd84ea7933b4cc02d22d0823dcafdf4321d174565",
}


def load_texture(path):
    payload = gzip.decompress(path.read_bytes())
    if payload[:8] != b"AELUNAR1":
        raise ValueError("Bad lunar magic")
    width, height, levels, srgb = struct.unpack_from("<4I", payload, 8)
    if not width or not height or levels != max(width, height).bit_length() or srgb > 1:
        raise ValueError("Invalid dimensions or mip count")
    offset, images = 24, []
    for level in range(levels):
        w, h = max(1, width >> level), max(1, height >> level)
        size = w * h * 4
        image = np.frombuffer(payload, np.uint8, size, offset).reshape(h, w, 4)
        if not np.all(image[:, :, 3] == 255):
            raise ValueError("Nonopaque lunar texture")
        images.append(image)
        offset += size
    if offset != len(payload):
        raise ValueError("Trailing/missing mip bytes")
    return images, srgb


def audit():
    for relative, expected in PINNED.items():
        actual = hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"Source or derived hash changed: {relative}")
    color, srgb = load_texture(ROOT / "assets/lunar/color.ael")
    assert srgb == 1 and color[0].shape == (4096, 8192, 4)
    checked_color_bytes = 0
    with rasterio.open(ROOT / ".cache/lunar/lroc_color_poles_8k.tif") as source:
        assert (source.width, source.height, source.count) == (8192, 4096, 3)
        for row in range(0, source.height, 64):
            original = source.read(window=((row, row + 64), (0, source.width))).transpose(1, 2, 0)
            if not np.array_equal(original, color[0][row:row+64, :, :3]):
                raise ValueError(f"NASA RGB level zero changed at block {row}")
            checked_color_bytes += original.size
    del color
    normals, srgb = load_texture(ROOT / "assets/lunar/normal.ael")
    assert srgb == 0 and normals[0].shape == (2880, 5760, 4)
    with rasterio.open(ROOT / ".cache/lunar/ldem_16_uint.tif") as source:
        assert (source.width, source.height, source.count, source.dtypes[0]) == (5760, 2880, 1, "uint16")
        raw = source.read(1)
    heights = raw.astype(np.float64) / 2.0 - 10000.0
    rows, columns = heights.shape
    dphi, dlambda = np.pi / rows, 2 * np.pi / columns
    longitude = -np.pi + (np.arange(columns) + .5) * dlambda
    sl, cl = np.sin(longitude)[None, :], np.cos(longitude)[None, :]
    max_angle, max_encoded_error, changed_roundings = 0.0, 0, 0
    for first in range(0, rows, 32):
        indices = np.arange(first, min(rows, first + 32))
        phi = np.pi / 2 - (indices[:, None] + .5) * dphi
        sp, cp = np.sin(phi), np.cos(phi)
        h = heights[indices]
        # Explicit source-neighbor indexing, including the half-turn reflected
        # polar continuation, independent of the production concatenate/roll.
        north = heights[np.maximum(indices - 1, 0)].copy()
        south = heights[np.minimum(indices + 1, rows - 1)].copy()
        if indices[0] == 0:
            north[0] = heights[0, (np.arange(columns) + columns // 2) % columns]
        if indices[-1] == rows - 1:
            south[-1] = heights[-1, (np.arange(columns) + columns // 2) % columns]
        hp = (north - south) / (2 * dphi)
        hl = (h[:, (np.arange(columns) + 1) % columns] - h[:, (np.arange(columns) - 1) % columns]) / (2 * dlambda)
        radial = np.stack(np.broadcast_arrays(cp * cl, cp * sl, sp), axis=2)
        east = np.stack(np.broadcast_arrays(-sl, cl, np.zeros_like(cp)), axis=2)
        north_axis = np.stack(np.broadcast_arrays(-sp * cl, -sp * sl, cp), axis=2)
        # Differentiate p=(R+h)n in Cartesian coordinates, then cross the two
        # tangent vectors. This does not invoke the production normal formula.
        radius = 1737400.0 + h
        tangent_lambda = radial * hl[:, :, None] + east * (radius * cp)[:, :, None]
        tangent_phi = radial * hp[:, :, None] + north_axis * radius[:, :, None]
        normal = np.cross(tangent_lambda, tangent_phi)
        normal /= np.linalg.norm(normal, axis=2, keepdims=True)
        tangent_components = np.stack([np.sum(normal * basis, axis=2) for basis in [east, north_axis, radial]], axis=2)
        expected = np.rint((tangent_components * .5 + .5) * 255).astype(np.int16)
        encoded = normals[0][indices, :, :3].astype(np.int16)
        error = np.abs(encoded - expected)
        max_encoded_error = max(max_encoded_error, int(error.max()))
        changed_roundings += int(np.count_nonzero(error))
        if np.any(error > 1):
            raise ValueError(f"Normal differs from Cartesian source derivative at row {first}")
        decoded = encoded.astype(np.float64) / 127.5 - 1
        decoded /= np.linalg.norm(decoded, axis=2, keepdims=True)
        angle = np.arctan2(np.linalg.norm(np.cross(decoded, tangent_components), axis=2), np.sum(decoded * tangent_components, axis=2))
        max_angle = max(max_angle, float(angle.max()))
    if max_angle > .007:
        raise ValueError("RGBA8 normal angular error exceeds the quantization fixture bound")
    max_mip_norm_error = 0.0
    for level in normals:
        for first in range(0, level.shape[0], 64):
            decoded = level[first:first+64, :, :3].astype(np.float64) / 127.5 - 1
            length = np.linalg.norm(decoded, axis=2)
            max_mip_norm_error = max(max_mip_norm_error, float(np.max(np.abs(length - 1))))
            if not np.all(decoded[:, :, 2] > 0) or np.any(np.abs(length - 1) > .007):
                raise ValueError("Invalid quantized mip normal")
    return {
        "source_hashes": PINNED,
        "source_decoder": f"rasterio {rasterio.__version__}, GDAL {rasterio.__gdal_version__}",
        "original_rgb_bytes_checked": checked_color_bytes,
        "normal_texels_checked": int(rows * columns),
        "height_min_max_m": [float(heights.min()), float(heights.max())],
        "maximum_normal_channel_difference_from_independent_rounding": max_encoded_error,
        "channels_with_one_count_rounding_difference": changed_roundings,
        "maximum_normal_angle_error_radians": max_angle,
        "all_normal_mips_maximum_quantized_length_error": max_mip_norm_error,
        "limits": "All level-zero color bytes and normal texels checked; lower mips format/unit-vector checks only. Finite derivative/quantization audit, not lunar geodetic accuracy or radiometry.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = audit()
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
