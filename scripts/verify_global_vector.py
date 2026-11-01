"""Independent OGR/scanline audit of the original-vector category index.

The oracle uses full source rings and horizontal scanlines, not the runtime's
cell seed-to-query crossing. The edge audit clips original segments directly
against each leaf with slab intervals, independently of recursive preprocessing.
No downloads or height data. Generated query fixtures are local QA artifacts.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import struct

import numpy as np

from verify_global_land import source_polygons

ROOT = Path(__file__).resolve().parents[1]
SOURCE_SHA = "e547d749445eaa0964aba76738090ec88f5e63c4585122170f98c67a7ea922dc"


def read_index():
    path = ROOT / "assets/boundaries/global-land.aev"
    raw = path.read_bytes()
    magic, width, height, count, edge_count, cap, depth = struct.unpack_from("<8s6I", raw)
    if magic != b"AEGLND02" or (width, height, cap, depth) != (360, 180, 32, 20):
        raise ValueError("Unexpected vector-index header")
    if len(raw) != 32 + (count + edge_count) * 36:
        raise ValueError("Wrong vector-index length")
    nodes = [struct.unpack_from("<3I2i2d", raw, 32 + i * 36) for i in range(count)]
    edges = np.asarray([struct.unpack_from("<4dI", raw, 32 + (count + i) * 36)
                        for i in range(edge_count)])
    leaves = []
    visited = set()
    stack = [(r * 360 + c, c - 180.0, r - 90.0, 1.0, 0, r * 360 + c)
             for r in range(180) for c in range(360)]
    while stack:
        index, west, south, size, level, root = stack.pop()
        if index in visited or level > depth:
            raise ValueError("Index is cyclic, shared or overdeep")
        visited.add(index)
        children, start, length, winding, hole, sx, sy = nodes[index]
        if children:
            for child in range(4):
                stack.append((children + child, west + (child & 1) * size / 2,
                              south + (child >> 1) * size / 2, size / 2, level + 1, root))
        else:
            if length > cap:
                raise ValueError("Unbounded leaf")
            leaves.append((index, west, south, size, root))
    if len(visited) != len(nodes):
        raise ValueError("Unreachable index nodes")
    return raw, nodes, edges, leaves


def original_edges(polygons):
    rows = []
    for polygon in polygons:
        for hole, ring in enumerate(polygon):
            points = np.asarray(ring)
            for a, b in zip(points, points[1:]):
                if np.any(a != b):
                    rows.append((*a, *b, int(hole != 0)))
    return np.asarray(rows)


def clipped(source, west, south, size):
    """Intersection of two independent one-dimensional slab intervals."""
    if not len(source):
        return np.empty((0, 5))
    a, d = source[:, :2], source[:, 2:4] - source[:, :2]
    lower, upper = np.zeros(len(source)), np.ones(len(source))
    valid = np.ones(len(source), dtype=bool)
    for axis, origin in enumerate((west, south)):
        moving = d[:, axis] != 0
        valid &= moving | ((a[:, axis] >= origin) & (a[:, axis] <= origin + size))
        entry = np.full(len(source), -np.inf)
        exit_ = np.full(len(source), np.inf)
        x = (origin - a[moving, axis]) / d[moving, axis]
        y = (origin + size - a[moving, axis]) / d[moving, axis]
        entry[moving], exit_[moving] = np.minimum(x, y), np.maximum(x, y)
        lower, upper = np.maximum(lower, entry), np.minimum(upper, exit_)
    valid &= lower < upper
    origin = np.array([west, south])
    return np.column_stack(((a[valid] + lower[valid, None] * d[valid] - origin) / size,
                            (a[valid] + upper[valid, None] * d[valid] - origin) / size,
                            source[valid, 4]))


def scanline_oracle(points, source, latitude_buckets):
    """Signed horizontal-ray oracle over complete original OGR ring segments."""
    result = np.zeros((len(points), 2), dtype=np.int32)
    reliable = np.ones(len(points), dtype=bool)
    groups = {}
    for i, (_, y) in enumerate(points):
        groups.setdefault(float(y), []).append(i)
    for y, indices in groups.items():
        row = max(0, min(179, int(np.floor(y + 90))))
        selected = source[latitude_buckets[row]]
        crossing = ((selected[:, 1] <= y) & (y < selected[:, 3])) | ((selected[:, 3] <= y) & (y < selected[:, 1]))
        selected = selected[crossing]
        if not len(selected):
            continue
        x = selected[:, 0] + (y - selected[:, 1]) * (selected[:, 2] - selected[:, 0]) / (selected[:, 3] - selected[:, 1])
        delta = np.where(selected[:, 3] > selected[:, 1], -1, 1)
        order = np.argsort(x)
        x, delta, holes = x[order], delta[order], selected[order, 4].astype(np.int32)
        cumulative = np.column_stack((np.r_[0, np.cumsum(delta)], np.r_[0, np.cumsum(delta * holes)]))
        query_x = points[indices, 0]
        offset = np.searchsorted(x, query_x, side="right")
        result[indices] = cumulative[offset]
        nearest_left = np.maximum(offset - 1, 0)
        nearest_right = np.minimum(offset, len(x) - 1)
        separation = np.minimum(np.abs(query_x - x[nearest_left]), np.abs(query_x - x[nearest_right]))
        reliable[indices] &= separation > 2e-10
    return result, reliable


def verify():
    source_path = ROOT / ".cache/natural-earth/ne_10m_land.zip"
    if hashlib.sha256(source_path.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Original reviewed source identity differs")
    polygons, records = source_polygons(source_path)
    source = original_edges(polygons)
    raw, nodes, edges, leaves = read_index()
    buckets = [[] for _ in range(64800)]
    latitude_buckets = [[] for _ in range(180)]
    for index, (ax, ay, bx, by, _) in enumerate(source):
        left = max(0, int(np.floor(min(ax, bx) + 180)) - 1)
        right = min(359, int(np.floor(max(ax, bx) + 180)))
        bottom = max(0, int(np.floor(min(ay, by) + 90)) - 1)
        top = min(179, int(np.floor(max(ay, by) + 90)))
        for row in range(bottom, top + 1):
            latitude_buckets[row].append(index)
            for column in range(left, right + 1):
                buckets[row * 360 + column].append(index)
    buckets = [source[indices].reshape(-1, 5) for indices in buckets]
    points, seed_expected = [], []
    maximum_error, audited_edges = 0.0, 0
    coverage = np.zeros(len(edges), dtype=np.uint8)
    for index, west, south, size, root in leaves:
        _, start, count, winding, hole, sx, sy = nodes[index]
        expected = clipped(buckets[root], west, south, size)
        actual = edges[start:start + count]
        if len(expected) != count:
            raise ValueError(f"Source fragment count differs in leaf{index}: {len(expected)} != {count}")
        coverage[start:start + count] += 1
        unused = list(range(count))
        for fragment in actual:
            errors = np.max(np.abs(expected[unused] - fragment), axis=1)
            best = int(np.argmin(errors))
            difference = float(errors[best])
            if difference > 2e-9:
                raise ValueError(f"Source segment altered/omitted in leaf{index}: {difference}")
            maximum_error = max(maximum_error, difference)
            unused.pop(best)
        audited_edges += count
        seed_expected.append((len(points), winding, hole))
        points.append((west + size * sx, south + size * sy))
        # Interior probes and points on both sides of adaptive boundaries.
        points.extend((west + size * x, south + size * y)
                      for x, y in [(0.137, 0.731), (0.829, 0.193), (-1e-5, 0.371), (1e-5, 0.371),
                                   (0.613, -1e-5), (0.613, 1e-5)])
    if not np.all(coverage == 1):
        raise ValueError("Stored edge multiplicity is not exactly one leaf")
    points = np.asarray(points)
    points[:, 0] = (points[:, 0] + 180) % 360 - 180
    points[:, 1] = np.clip(points[:, 1], -89.999999999, 89.999999999)
    expected, reliable = scanline_oracle(points, source, latitude_buckets)
    for index, winding, hole in seed_expected:
        if not reliable[index] or tuple(expected[index]) != (winding, hole):
            raise ValueError(f"Seed disagrees with original horizontal source oracle: {points[index]}, {expected[index]}, {(winding, hole)}")
    categories = np.where(expected[:, 0] < 0, 0, np.where(expected[:, 1] > 0, 2, 1)).astype(np.uint8)
    fixture = ROOT / ".cache/global-vector-query-reference.bin"
    with fixture.open("wb") as output:
        output.write(struct.pack("<8sI", b"AEGVQ001", int(reliable.sum())))
        for (longitude, latitude), category in zip(points[reliable], categories[reliable]):
            output.write(struct.pack("<ddB", latitude, longitude, int(category)))
    report = dict(source_sha256=SOURCE_SHA, output_sha256=hashlib.sha256(raw).hexdigest(),
                  source_records=records, original_segments=len(source), leaves=len(leaves),
                  all_indexed_fragments_checked=audited_edges,
                  maximum_OGR_fragment_difference_cell_units=maximum_error,
                  seeds_checked=len(seed_expected), independent_query_cases=int(reliable.sum()),
                  excluded_source_boundary_cases=int((~reliable).sum()),
                  query_reference_file=fixture.relative_to(ROOT).as_posix())
    (ROOT / ".cache/global-vector-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    verify()
