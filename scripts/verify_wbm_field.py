"""Independent GDAL fixtures for the native Copernicus water-body field.

Reads only the four previously licensed source COGs. Coordinates are GDAL
PixelIsPoint sample centers, not positions reconstructed by the Rust importer.
CRC32 covers every decoded byte; SHA256 pins each unmodified original source.
No downloads, DEM/geoid inputs, or changes to a user's cache.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib

import numpy as np
import rasterio

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "crates/earth-math/tests/fixtures/wbm_source.rs"
SOURCES = [
    ("N29_00_W095_00", 29, -95),
    ("N29_00_W096_00", 29, -96),
    ("N35_00_E138_00", 35, 138),
    ("N35_00_E139_00", 35, 139),
]


def generate(cache: Path) -> str:
    reviewed = json.loads((ROOT / "licenses/water-sources.json").read_text(encoding="utf-8"))
    digests = {entry["filename"]: entry["sha256"] for entry in reviewed["files"]}
    lines = [
        "// Original numeric fixtures from scripts/verify_wbm_field.py (GDAL/Rasterio).",
        "// Source rights and SHA256 identities: licenses/water-sources.json.",
        "// CRC32 covers the entire independently decoded row-major UInt8 raster.",
        "pub type Raster = (&'static str, i32, i32, [u64; 4], u32, usize);",
        "pub type Post = (usize, usize, usize, f64, f64, u8);",
        "#[rustfmt::skip]",
        "pub const RASTERS: &[Raster] = &[",
    ]
    posts = []
    for source_index, (name, south, west) in enumerate(SOURCES):
        path = cache / f"{name}-WBM.tif"
        if hashlib.sha256(path.read_bytes()).hexdigest() != digests[path.name]:
            raise ValueError(f"Unreviewed source bytes: {path}")
        with rasterio.open(path) as source:
            if not (source.crs.to_epsg() == 4326 and source.tags().get("AREA_OR_POINT") == "Point"
                    and source.shape == (3600, 3600) and source.count == 1
                    and source.dtypes == ("uint8",) and source.nodata is None):
                raise ValueError(f"Unexpected native WBM format: {name}")
            values = source.read(1)
            if np.max(values) > 3:
                raise ValueError(f"Unknown category: {name}")
            counts = np.bincount(values.ravel(), minlength=4).tolist()
            crc = zlib.crc32(values.tobytes())
            runs = int(values.shape[0] + np.count_nonzero(values[:, 1:] != values[:, :-1]))
            lines.append(f'    ("{name}", {south}, {west}, {counts}, 0x{crc:08x}, {runs}),')
            indices = {(0, 0), (0, 3599), (3599, 0), (3599, 3599), (1800, 1800)}
            for category in range(4):
                matching = np.flatnonzero(values.ravel() == category)
                if len(matching):
                    for flat in [matching[0], matching[len(matching) // 2], matching[-1]]:
                        indices.add(divmod(int(flat), 3600))
            # Both sides of real source-category changes, distributed over rows.
            changed = np.flatnonzero(values[:, 1:] != values[:, :-1])
            for flat in changed[::max(1, len(changed) // 6)][:6]:
                row, column = divmod(int(flat), 3599)
                indices.add((row, column))
                indices.add((row, column + 1))
            for row, column in sorted(indices):
                longitude, latitude = map(float, source.xy(row, column))
                if abs(longitude - (west + column / 3600)) > 2e-12 \
                        or abs(latitude - (south + 1 - row / 3600)) > 2e-12:
                    raise ValueError(f"Unexpected source-post coordinates: {name}")
                posts.append(f"    ({source_index}, {row}, {column}, {latitude!r}, {longitude!r}, {int(values[row, column])}),")
    return "\n".join(lines + ["];", "#[rustfmt::skip]", "pub const POSTS: &[Post] = &["] + posts + ["];", ""])


def audit_dump(cache: Path, dump: Path) -> dict:
    """Compare every Rust-exported native category with independent GDAL bytes."""
    raw = dump.read_bytes()
    if raw[:8] != b"AEWBMT01":
        raise ValueError("Unrecognized review export")
    count, = struct.unpack_from("<I", raw, 8)
    cursor = 12
    manifest = json.loads((cache / "licenses/assets.json").read_text(encoding="utf-8"))
    assets = {entry["path"]: entry for entry in manifest["assets"]}
    results = []
    for _ in range(count):
        south, west, width, height, offsets_count, run_count = struct.unpack_from("<ii4I", raw, cursor)
        cursor += 24
        offsets = np.frombuffer(raw, dtype="<u4", count=offsets_count, offset=cursor)
        cursor += offsets_count * 4
        runs = np.frombuffer(raw, dtype="<u4", count=run_count, offset=cursor)
        cursor += run_count * 4
        name = f"Copernicus_DSM_COG_10_{'N' if south >= 0 else 'S'}{abs(south):02}_00_{'E' if west >= 0 else 'W'}{abs(west):03}_00_WBM.tif"
        relative = "assets/" + name
        path = cache / relative
        entry = assets[relative]
        source_sha = hashlib.sha256(path.read_bytes()).hexdigest()
        if source_sha != entry["sha256"]:
            raise ValueError(f"Cached source hash differs from reviewed manifest: {name}")
        provenance_path = cache / entry["provenance"]
        if hashlib.sha256(provenance_path.read_bytes()).hexdigest() != entry["provenance_sha256"]:
            raise ValueError(f"Provenance hash differs: {name}")
        provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
        if provenance["sha256"] != source_sha or provenance["output_sha256"] != source_sha:
            raise ValueError(f"Source/provenance identity differs: {name}")
        license_path = cache / "licenses/Copernicus-WorldDEM-30-eula_F.pdf"
        if hashlib.sha256(license_path.read_bytes()).hexdigest() != provenance["license_sha256"]:
            raise ValueError(f"License identity differs: {name}")
        with rasterio.open(path) as source:
            if source.shape != (height, width) or source.crs.to_epsg() != 4326 \
                    or source.tags().get("AREA_OR_POINT") != "Point" or source.dtypes != ("uint8",):
                raise ValueError(f"Source registration differs: {name}")
            values = source.read(1)
            for row, column in [(0, 0), (height - 1, width - 1)]:
                longitude, latitude = source.xy(row, column)
                if abs(latitude - (south + 1 - row / 3600)) > 2e-12 \
                        or abs(longitude - (west + column / width)) > 2e-12:
                    raise ValueError(f"Source post coordinates differ: {name}")
            if offsets_count != height + 1 or offsets[0] != 0 or offsets[-1] != run_count:
                raise ValueError(f"Invalid exported offsets: {name}")
            for row in range(height):
                segment = runs[int(offsets[row]):int(offsets[row + 1])]
                ends = (segment >> 2).astype(np.int64)
                lengths = np.diff(np.r_[0, ends])
                if np.any(lengths <= 0) or ends[-1] != width:
                    raise ValueError(f"Invalid row run sequence: {name}, row{row}")
                expanded = np.repeat(segment & 3, lengths).astype(np.uint8)
                if not np.array_equal(expanded, values[row]):
                    raise ValueError(f"Rust categories differ from native GDAL source: {name}, row{row}")
            results.append(dict(source=name, source_sha256=source_sha, posts=width * height,
                                runs=run_count, category_counts=np.bincount(values.ravel(), minlength=4).tolist()))
    if cursor != len(raw):
        raise ValueError("Trailing or truncated review export")
    result = dict(tiles=count, posts=sum(item["posts"] for item in results),
                  runs=sum(item["runs"] for item in results), sources=results)
    (ROOT / ".cache/wbm-field-verification.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/water")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--audit-cache", type=Path,
                        help="Independently audit the Rust export from this isolated reviewed download cache")
    parser.add_argument("--dump", type=Path, default=ROOT / ".cache/wbm-field-review.bin")
    args = parser.parse_args()
    if args.audit_cache:
        result = audit_dump(args.audit_cache, args.dump)
        print(f"Exact GDAL agreement: {result['tiles']} native source masks, {result['posts']:,} posts, {result['runs']:,} lossless row runs.")
        raise SystemExit(0)
    content = generate(args.cache)
    if args.check:
        if TARGET.read_text(encoding="utf-8") != content:
            raise SystemExit("Native WBM source fixtures differ")
    else:
        TARGET.write_text(content, encoding="utf-8", newline="\n")
    print("Verified four licensed native WBM sources: all 51,840,000 decoded categories, registration and sampled category transitions.")
