#!/usr/bin/env python3
"""NASA SVS 4720 presentation maps; no terrain displacement or invented geography.

Run with the cached scientific Python packages on PYTHONPATH if necessary.
The .ael payload is gzip(AELUNAR1 + u32 width,height,levels,sRGB + mip RGBA8).
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import struct

import numpy as np
from PIL import Image

RADIUS_M = 1737400.0


def reduce_box(array, size):
    return np.stack([np.asarray(Image.fromarray(array[:, :, c].astype(np.float32)).resize(size, Image.Resampling.BOX)) for c in range(3)], axis=2)


def write_texture(path, field, srgb):
    if field.ndim != 3 or field.shape[2] != 3 or min(field.shape[:2]) < 1 or not np.isfinite(field).all():
        raise ValueError('Expected finite nonempty H x W x 3 field')
    height, width, _ = field.shape
    levels = []
    while True:
        if srgb:
            encoded = np.where(field <= 0.0031308, 12.92 * field, 1.055 * np.maximum(field, 0) ** (1 / 2.4) - 0.055)
        else:
            field /= np.maximum(np.linalg.norm(field, axis=2, keepdims=True), 1e-30)
            encoded = field * 0.5 + 0.5
        rgba = np.empty((*field.shape[:2], 4), dtype=np.uint8)
        rgba[:, :, :3] = np.rint(np.clip(encoded, 0, 1) * 255).astype(np.uint8)
        rgba[:, :, 3] = 255
        levels.append(rgba.tobytes())
        if field.shape[:2] == (1, 1):
            break
        field = reduce_box(field, (max(1, field.shape[1] // 2), max(1, field.shape[0] // 2)))
    payload = b'AELUNAR1' + struct.pack('<4I', width, height, len(levels), int(srgb)) + b''.join(levels)
    path.write_bytes(gzip.compress(payload, compresslevel=9, mtime=0))
    return {'path': path.as_posix(), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'width': width, 'height': height, 'mip_levels': len(levels), 'srgb': srgb}


def normals(heights):
    """Central longitude/latitude derivatives at pixel centers, metres/radian.

    Longitude is periodic. Across either pole, geographic continuation reflects
    the latitude row and shifts longitude by pi. No evaluation at the singular
    pole itself, because all samples are cell centers.
    """
    if heights.ndim != 2 or min(heights.shape) < 1 or not np.isfinite(heights).all() or np.any(heights <= -RADIUS_M):
        raise ValueError('Expected finite nonempty heights with positive lunar radius')
    rows, columns = heights.shape
    if columns != rows * 2 or columns % 2:
        raise ValueError('Expected full-globe 2:1 equirectangular map')
    dlon, dlat = 2 * np.pi / columns, np.pi / rows
    longitude_gradient = (np.roll(heights, -1, axis=1) - np.roll(heights, 1, axis=1)) / (2 * dlon)
    north = np.concatenate([np.roll(heights[:1], columns // 2, axis=1), heights[:-1]])
    south = np.concatenate([heights[1:], np.roll(heights[-1:], columns // 2, axis=1)])
    latitude_gradient = (north - south) / (2 * dlat)
    latitude = np.pi / 2 - (np.arange(rows, dtype=np.float64) + 0.5) * dlat
    radius = RADIUS_M + heights
    result = np.stack([-longitude_gradient / (radius * np.cos(latitude)[:, None]), -latitude_gradient / radius, np.ones_like(heights)], axis=2)
    result /= np.linalg.norm(result, axis=2, keepdims=True)
    return result.astype(np.float32)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=Path, default=Path('.cache/lunar'))
    parser.add_argument('--output', type=Path, default=Path('assets/lunar'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    color_path = args.source / 'lroc_color_poles_8k.tif'
    height_path = args.source / 'ldem_16_uint.tif'
    color = np.asarray(Image.open(color_path).convert('RGB'), dtype=np.float32) / 255
    color = np.where(color <= .04045, color / 12.92, ((color + .055) / 1.055) ** 2.4)
    outputs = [write_texture(args.output / 'color.ael', color, True)]
    del color
    raw = np.asarray(Image.open(height_path))
    if raw.dtype != np.uint16:
        raise ValueError('Expected unsigned 16-bit LOLA half-metre map')
    heights = raw.astype(np.float64) * .5 - 10000.0
    outputs.append(write_texture(args.output / 'normal.ael', normals(heights), False))
    sources = [{'file': p.name, 'url': 'https://svs.gsfc.nasa.gov/vis/a000000/a004700/a004720/' + p.name, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in [color_path, height_path]]
    provenance = {
        'source_page': 'https://svs.gsfc.nasa.gov/4720/', 'credit': "NASA's Scientific Visualization Studio",
        'sources': sources, 'outputs': outputs,
        'registration': 'Full globe; east-positive longitude -180 to +180 left to right; planetocentric latitude +90 to -90 top to bottom; pixel centers (column+.5)/width,(row+.5)/height; cartographic ME source approximated by existing IAU_MOON text-PCK attitude.',
        'color_modification': 'Unchanged 2019 NASA RGB level zero, treated as sRGB display color. Lower mip levels are box averages in linear RGB then re-encoded sRGB. NASA already adjusted exposure/white balance, inpainted dropouts and filled polar latitudes from LDAM; not calibrated radiometry.',
        'normal_modification': 'Decode h=0.5*DN-10000 metres relative to1737400m sphere. Centered periodic angular derivatives and reflected half-turn polar continuation. Tangent-space outward normal (-h_lambda/((R+h)cos(phi)),-h_phi/(R+h),1), normalized, no exaggeration. Lower mips box-average vectors and renormalize; RGBA8 linear encoding. Shading only; no displacement, lunar silhouette/ranges remain mean sphere.',
        'height_min_max_m': [float(heights.min()), float(heights.max())],
        'runtime_format': 'gzip with mtime0; AELUNAR1, little-endian u32 width,height,mips,sRGB, complete row-major RGBA8 mip chain largest first',
    }
    (args.output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(outputs, indent=2))


if __name__ == '__main__':
    main()
