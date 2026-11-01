"""Source-derived coarse geographic context; no photographs or physical heights.

Reads the reviewed original Natural Earth land polygon archive, then evaluates
land/interior-water at 0.01-degree cell centers using GDAL rasterization. The
result is exact row RLE of that explicitly finite categorical grid.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zipfile
import numpy as np
from rasterio.crs import CRS
from rasterio.features import rasterize
from rasterio.transform import Affine

ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA = "e547d749445eaa0964aba76738090ec88f5e63c4585122170f98c67a7ea922dc"
WIDTH, HEIGHT = 36000, 18000


def inside(point, ring):
    x, y = point
    result = False
    for (ax, ay), (bx, by) in zip(ring, ring[1:]):
        if (ay > y) != (by > y) and x < ax + (y-ay)*(bx-ax)/(by-ay):
            result = not result
    return result


def polygons(data):
    """Read only documented ESRI Polygon records; retain source ring vertices."""
    if (len(data) < 100 or struct.unpack_from(">i", data)[0] != 9994
            or struct.unpack_from(">i", data, 24)[0]*2 != len(data)
            or struct.unpack_from("<2i", data, 28) != (1000, 5)):
        raise ValueError("Unsupported shapefile header")
    offset = 100
    outer, holes = [], []
    records = 0
    while offset < len(data):
        number, words = struct.unpack_from(">2i", data, offset)
        record = data[offset+8:offset+8+2*words]
        offset += 8+2*words
        if number != records+1 or len(record) != 2*words:
            raise ValueError("Malformed shapefile record")
        records += 1
        shape = struct.unpack_from("<i", record)[0]
        if shape == 0:
            continue
        if shape != 5 or len(record) < 44:
            raise ValueError("Only Polygon geometry is accepted")
        parts, count = struct.unpack_from("<2i", record, 36)
        if parts <= 0 or count < 4 or len(record) != 44+parts*4+count*16:
            raise ValueError("Invalid Polygon counts")
        indices = list(struct.unpack_from(f"<{parts}i", record, 44))+[count]
        if indices[0] != 0 or any(a >= b for a, b in zip(indices, indices[1:])):
            raise ValueError("Invalid Polygon ring index")
        points = np.frombuffer(record, dtype="<f8", count=count*2, offset=44+parts*4).reshape(-1, 2)
        if not np.isfinite(points).all() or np.any(np.abs(points) > [180.000000001, 90.000000001]):
            raise ValueError("Source is not finite longitude/latitude degrees")
        for start, end in zip(indices, indices[1:]):
            ring = points[start:end].copy()
            if len(ring) < 4 or not np.array_equal(ring[0], ring[-1]):
                raise ValueError("Unclosed polygon ring")
            twice_area = np.sum(ring[:-1, 0]*ring[1:, 1]-ring[1:, 0]*ring[:-1, 1])
            if twice_area == 0:
                raise ValueError("Degenerate source ring")
            (outer if twice_area < 0 else holes).append(ring)
    # ESRI outer rings are clockwise; holes are counterclockwise. Associate by
    # containment rather than assuming a record is a single simple polygon.
    attached = [[ring] for ring in outer]
    for hole in holes:
        owners = [i for i, ring in enumerate(outer)
                  if np.all(hole[0] >= ring.min(axis=0))
                  and np.all(hole[0] <= ring.max(axis=0)) and inside(hole[0], ring)]
        if len(owners) != 1:
            raise ValueError("Source hole lacks one unambiguous containing exterior")
        attached[owners[0]].append(hole)
    return attached, holes, records


def prepare(check=False):
    source = ROOT/".cache/natural-earth/ne_10m_land.zip"
    raw = source.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA:
        raise ValueError("Archive differs from the exact rights-reviewed source")
    with zipfile.ZipFile(source) as archive:
        if CRS.from_wkt(archive.read("ne_10m_land.prj").decode()).to_epsg() != 4326:
            raise ValueError("Unexpected Natural Earth CRS")
        rings, holes, records = polygons(archive.read("ne_10m_land.shp"))
    geometries = [(min(p[0][:, 1]), max(p[0][:, 1]), {"type": "Polygon", "coordinates": [r.tolist() for r in p]}) for p in rings]
    inland = [(min(r[:, 1]), max(r[:, 1]), {"type": "Polygon", "coordinates": [r.tolist()]}) for r in holes]
    row_offsets = [0]
    all_runs = []
    counts = np.zeros(3, dtype=np.int64)
    for first in range(0, HEIGHT, 256):
        height = min(256, HEIGHT-first)
        north, south = 90-first*0.01, 90-(first+height)*0.01
        transform = Affine(0.01, 0, -180, 0, -0.01, north)
        # Enclosed inland water is distinct from exterior ocean. Land polygons
        # with retained holes leave these values intact; any source islands can
        # still overwrite inland water as land.
        block = np.ones((height, WIDTH), dtype=np.uint8)
        water = [(g, 2) for lo, hi, g in inland if hi >= south and lo <= north]
        land = [(g, 0) for lo, hi, g in geometries if hi >= south and lo <= north]
        if water:
            rasterize(water, out=block, transform=transform, all_touched=False)
        if land:
            rasterize(land, out=block, transform=transform, all_touched=False)
        counts += np.bincount(block.ravel(), minlength=3)
        for row in block:
            ends = np.r_[np.flatnonzero(row[:-1] != row[1:])+1, WIDTH]
            codes = row[ends-1].astype(np.uint32)
            packed = (ends.astype(np.uint32) << 2) | codes
            all_runs.append(packed)
            row_offsets.append(row_offsets[-1]+len(packed))
    runs = np.concatenate(all_runs).astype("<u4")
    data = (struct.pack("<8sIII", b"AEGLND01", WIDTH, HEIGHT, len(runs))
            + np.asarray(row_offsets, dtype="<u4").tobytes()+runs.tobytes())
    output = ROOT/"assets/boundaries/global-land.ael"
    metadata = {
        "schema": "ancientearth.global-land.v1", "source_name": "Natural Earth 1:10 million land v5.1.1",
        "source_url": "https://naciscdn.org/naturalearth/10m/physical/ne_10m_land.zip",
        "source_page": "https://www.naturalearthdata.com/downloads/10m-physical-vectors/10m-land/",
        "license_url": "https://www.naturalearthdata.com/about/terms-of-use/", "license": "Public domain",
        "source_sha256": SOURCE_SHA, "source_records": records, "outer_rings": len(rings), "inland_holes": len(holes),
        "output_file": output.name, "output_sha256": hashlib.sha256(data).hexdigest(),
        "grid": {"width": WIDTH, "height": HEIGHT, "cell_degrees": 0.01,
                 "cell_center": "longitude=-180+(column+0.5)*0.01; latitude=90-(row+0.5)*0.01"},
        "category_counts_land_ocean_inland_water": counts.tolist(), "run_count": len(runs),
        "encoding": "AEGLND01; u32 width,height,run_count; u32 row_offsets[height+1]; u32 runs[run_count]=(exclusive_end_column<<2)|class; 0land,1ocean,2enclosed-inland-water",
        "modification": "Original polygon vertices parsed in geographic WGS84 degrees; GDAL default center-cell polygon rasterization at0.01degree; inland holes retained separately; exact row run-length encoding. No imagery or elevation input/output.",
        "scope": "Coarse cartographic land/ocean context only. Does not supply surveyed shoreline, water heights, sea-level/tide, bathymetry or DEM coverage. Higher-priority local reconstructed boundaries and downloaded native WBM override it.",
    }
    meta_path = output.with_suffix(".provenance.json")
    if check:
        if output.read_bytes() != data or json.loads(meta_path.read_text()) != metadata:
            raise ValueError("Global land mask regeneration differs")
    else:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(data)
        meta_path.write_text(json.dumps(metadata, indent=2)+"\n")
    print(f"Global land: {len(runs):,} runs, {len(data):,} bytes; counts {counts.tolist()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    prepare(parser.parse_args().check)
