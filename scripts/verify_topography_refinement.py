"""Independent GDAL/PROJ and integer-topology audit of the cached import fixture.

First run the explicitly ignored Rust export fixture with
ANCIENT_EARTH_TOPOGRAPHY_CACHE set to the completed source cache. This script
reads that cache only; it neither downloads nor rewrites source data. The Rust
fixture uses constant N=-30m to isolate source registration/height preservation
from the separately tested native EGM2008 implementation.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

import numpy as np
import rasterio
from pyproj import Transformer

ROOT = Path(__file__).resolve().parents[1]


def verify(cache, mesh):
    data = mesh.read_bytes()
    if data[:8] != b"REFAUD01":
        raise ValueError("Unexpected review export")
    patches, = struct.unpack_from("<I", data, 8)
    cursor = 12
    inverse = Transformer.from_crs(4978, 4979, always_xy=True)
    all_points, all_xyz, all_heights = [], [], []
    summary = []
    max_lattice_error = 0.0
    for patch in range(patches):
        width, height, count, face_count = struct.unpack_from("<4I", data, cursor)
        cursor += 16
        xyz = np.frombuffer(data, dtype="<f8", count=count * 3, offset=cursor).reshape(-1, 3)
        cursor += count * 24
        faces = np.frombuffer(data, dtype="<u4", count=face_count * 3, offset=cursor).reshape(-1, 3)
        cursor += face_count * 12
        longitude, latitude, h = inverse.transform(*xyz.T)
        scaled = np.column_stack((longitude, latitude)) * 7200
        points = np.rint(scaled).astype(np.int64)
        lattice_error = float(np.max(np.abs(scaled - points)))
        max_lattice_error = max(max_lattice_error, lattice_error)
        assert lattice_error < 2e-7, (patch, lattice_error)
        assert np.all(faces < count) and len(np.unique(faces)) == count
        assert len(np.unique(points, axis=0)) == count
        # Exact integer geographic parameter area: positive faces and total
        # rectangle area, with a disk topology and no internal unpaired edges.
        triangle = points[faces]
        a, b = triangle[:, 1] - triangle[:, 0], triangle[:, 2] - triangle[:, 0]
        area2 = a[:, 0] * b[:, 1] - a[:, 1] * b[:, 0]
        assert np.all(area2 > 0), (patch, "reversed/degenerate face")
        lower, upper = points.min(axis=0), points.max(axis=0)
        assert int(area2.sum()) == 2 * int(np.prod(upper - lower))
        directed = np.concatenate((faces[:, [0, 1]], faces[:, [1, 2]], faces[:, [2, 0]]))
        edges = np.sort(directed, axis=1)
        unique, inverse_edges, counts = np.unique(edges, axis=0, return_inverse=True, return_counts=True)
        assert np.all((counts == 1) | (counts == 2))
        directed_balance = np.bincount(inverse_edges, weights=np.where(directed[:, 0] < directed[:, 1], 1, -1))
        assert np.all(directed_balance[counts == 2] == 0)
        assert count - len(unique) + face_count == 1
        edge_points = points[unique[counts == 1]]
        on_perimeter = np.zeros(len(edge_points), dtype=bool)
        for axis in [0, 1]:
            for boundary in [lower[axis], upper[axis]]:
                on_perimeter |= np.all(edge_points[:, :, axis] == boundary, axis=1)
        assert np.all(on_perimeter), (patch, "interior crack or T junction")
        outward = np.einsum("ij,ij->i", np.cross(xyz[faces[:, 1]]-xyz[faces[:, 0]], xyz[faces[:, 2]]-xyz[faces[:, 0]]), xyz[faces[:, 0]])
        assert np.all(outward > 0)
        # The original background grid stays the canonical vertex prefix.
        background = points[:width * height].reshape(height, width, 2)
        dx, dy = np.diff(background, axis=1), np.diff(background, axis=0)
        assert np.all(dx[:, :, 0] == 60) and np.all(dx[:, :, 1] == 0)
        assert np.all(dy[:, :, 0] == 0) and np.all(dy[:, :, 1] == -60)
        summary.append({"vertices": count, "faces": face_count, "background_vertices": width * height,
                        "additional_native_posts": count - width * height, "perimeter_edges": len(edge_points)})
        all_points.append(points)
        all_xyz.append(xyz)
        all_heights.append(h)
    assert cursor == len(data)
    points, xyz, heights = map(np.concatenate, (all_points, all_xyz, all_heights))
    # Shared tile posts must have bit-identical ECEF values, not separately
    # extrapolated elevations. Compare every duplicate coordinate.
    order = np.lexsort((points[:, 1], points[:, 0]))
    sorted_points, sorted_xyz = points[order], xyz[order]
    duplicated = np.all(sorted_points[1:] == sorted_points[:-1], axis=1)
    assert np.array_equal(sorted_xyz[1:][duplicated].view(np.uint64), sorted_xyz[:-1][duplicated].view(np.uint64))
    # North-edge posts are owned by the southern source file, east-edge posts
    # by the eastern file, matching GDAL's actual native point registration.
    south = (points[:, 1] - 1) // 7200
    west = points[:, 0] // 7200
    max_height_error = 0.0
    source_range = [float("inf"), -float("inf")]
    sources = []
    for s, w in np.unique(np.column_stack((south, west)), axis=0):
        name = f"Copernicus_DSM_COG_10_{'N' if s >= 0 else 'S'}{abs(s):02}_00_{'E' if w >= 0 else 'W'}{abs(w):03}_00_DEM.tif"
        path = cache / "assets" / name
        selected = (south == s) & (west == w)
        with rasterio.open(path) as source:
            assert source.crs.to_epsg() == 4326 and source.tags().get("AREA_OR_POINT") == "Point"
            assert source.height == 3600 and 7200 % source.width == 0
            row_units = (s+1)*7200-points[selected, 1]
            column_units = points[selected, 0]-w*7200
            assert np.all(row_units % 2 == 0)
            assert np.all(column_units % (7200//source.width) == 0)
            rows = row_units//2
            columns = column_units//(7200//source.width)
            x, y = source.xy(rows, columns)
            assert np.max(np.abs(np.asarray(x)-points[selected, 0]/7200)) < 2e-12
            assert np.max(np.abs(np.asarray(y)-points[selected, 1]/7200)) < 2e-12
            actual = source.read(1)[rows, columns].astype(np.float64)
        error = float(np.max(np.abs(heights[selected]-(actual-30.0))))
        assert error < 4e-8, (name, error)
        max_height_error = max(max_height_error, error)
        source_range[0] = min(source_range[0], float(actual.min()))
        source_range[1] = max(source_range[1], float(actual.max()))
        sources.append({"source": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "retained_occurrences": int(selected.sum())})
    return {"patches": summary, "retained_vertices_including_tile_duplicates": len(points),
            "retained_faces": sum(p["faces"] for p in summary), "shared_duplicate_posts": int(duplicated.sum()),
            "max_half_arcsecond_lattice_residual": max_lattice_error,
            "max_independent_proj_height_error_m": max_height_error, "raw_source_height_range_m": source_range,
            "datum_fixture": "constant N=-30m; not a second native EGM2008 accuracy claim", "sources": sources}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--mesh", type=Path, default=ROOT/".cache/topography-refinement-review.bin")
    parser.add_argument("--output", type=Path, default=ROOT/".cache/topography-refinement-verification.json")
    args = parser.parse_args()
    report = verify(args.cache, args.mesh)
    args.output.write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in report.items() if key not in ["patches", "sources"]}, indent=2))
