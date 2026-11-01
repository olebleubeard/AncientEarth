"""Bounded original-vector global categories; no height or photographic input.

The reviewed Natural Earth polygon segments remain straight in their original
longitude/latitude coordinates. Adaptive cells only index those segments: they
do not replace the source coastline with pixel edges or invent finer detail.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import zipfile

import numpy as np
from rasterio.crs import CRS

from prepare_global_land import SOURCE_SHA, polygons

ROOT = Path(__file__).resolve().parents[1]
ROOTS = 360 * 180
EDGE_LIMIT = 32
MAX_DEPTH = 20
MAX_GPU_WORDS = 16_000_000


def clip(edge, west, south, size):
    """Liang--Barsky clipping; new endpoints stay on an original segment."""
    ax, ay, bx, by, hole = edge
    dx, dy = bx - ax, by - ay
    lower, upper = 0.0, 1.0
    for p, q in [(-dx, ax-west), (dx, west+size-ax),
                 (-dy, ay-south), (dy, south+size-ay)]:
        if p == 0:
            if q < 0:
                return None
        elif p < 0:
            lower = max(lower, q/p)
        else:
            upper = min(upper, q/p)
        if lower >= upper:
            return None
    return (ax+lower*dx, ay+lower*dy, ax+upper*dx, ay+upper*dy, hole)


def winding(point, edges):
    """Standard signed winding of the original directed polygon rings."""
    x, y = point
    ax, ay, bx, by, hole = edges.T
    left = (bx-ax)*(y-ay) - (by-ay)*(x-ax)
    up = (ay <= y) & (by > y) & (left > 0)
    down = (ay > y) & (by <= y) & (left < 0)
    return (int(np.count_nonzero(up)-np.count_nonzero(down)),
            int(np.count_nonzero(up & (hole != 0))-np.count_nonzero(down & (hole != 0))))


def seed_for(edges):
    """Choose a deterministic interior seed separated from every local edge."""
    if not len(edges):
        return (0.5, 0.5), 1.0
    a, delta = edges[:, :2], edges[:, 2:4]-edges[:, :2]
    squared = np.einsum('ij,ij->i', delta, delta)
    best, clearance = None, -1.0
    for x in (0.5, 0.25, 0.75, 0.125, 0.875):
        for y in (0.5, 0.25, 0.75, 0.125, 0.875):
            offset = np.array([x, y])-a
            t = np.clip(np.einsum('ij,ij->i', offset, delta)/squared, 0, 1)
            distance = float(np.min(np.linalg.norm(offset-t[:, None]*delta, axis=1)))
            if distance > clearance:
                best, clearance = (x, y), distance
    if clearance <= 1e-6:
        raise ValueError('No numerically separated interior seed')
    return best, clearance


def prepare(check=False):
    source = ROOT/'.cache/natural-earth/ne_10m_land.zip'
    raw = source.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA:
        raise ValueError('Source differs from the exact rights-reviewed archive')
    with zipfile.ZipFile(source) as archive:
        if CRS.from_wkt(archive.read('ne_10m_land.prj').decode()).to_epsg() != 4326:
            raise ValueError('Unexpected Natural Earth CRS')
        shapes, holes, records = polygons(archive.read('ne_10m_land.shp'))
    segments = []
    for shape in shapes:
        for ring_index, ring in enumerate(shape):
            for a, b in zip(ring, ring[1:]):
                if np.any(a != b):
                    segments.append((*a, *b, int(ring_index != 0)))
    original = np.asarray(segments, dtype=np.float64)
    buckets = [[] for _ in range(ROOTS)]
    latitude_buckets = [[] for _ in range(180)]
    for index, edge in enumerate(segments):
        ax, ay, bx, by, _ = edge
        left = max(0, min(359, math.floor(min(ax, bx)+180)))
        right = max(0, min(359, math.floor(max(ax, bx)+180)))
        low = max(0, min(179, math.floor(min(ay, by)+90)))
        high = max(0, min(179, math.floor(max(ay, by)+90)))
        # Include the western/southern neighbor for edges exactly on its border.
        if min(ax, bx)+180 == left and left > 0:
            left -= 1
        if min(ay, by)+90 == low and low > 0:
            low -= 1
        for row in range(low, high+1):
            latitude_buckets[row].append(index)
            for column in range(left, right+1):
                item = clip(edge, column-180, row-90, 1.0)
                if item is not None:
                    buckets[row*360+column].append(item)
    latitude_edges = [original[indices] for indices in latitude_buckets]
    nodes = [None]*ROOTS
    stored_edges = []
    histogram = [0]*(EDGE_LIMIT+1)
    deepest = 0
    minimum_clearance = 1.0

    def build(index, west, south, size, candidates, depth):
        nonlocal deepest, minimum_clearance
        deepest = max(deepest, depth)
        if len(candidates) > EDGE_LIMIT:
            if depth >= MAX_DEPTH:
                raise ValueError('Coastline exceeds bounded vector subdivision depth')
            first = len(nodes)
            nodes.extend([None]*4)
            nodes[index] = (first, 0, 0, 0, 0, 0.5, 0.5)
            half = size/2
            for child in range(4):
                x, y = west+(child & 1)*half, south+(child >> 1)*half
                clipped = [item for edge in candidates if (item := clip(edge, x, y, half)) is not None]
                build(first+child, x, y, half, clipped, depth+1)
            return
        local = np.asarray([((a-west)/size, (b-south)/size,
                             (c-west)/size, (d-south)/size, flag)
                            for a, b, c, d, flag in candidates], dtype=np.float64).reshape(-1, 5)
        seed, clearance = seed_for(local)
        minimum_clearance = min(minimum_clearance, clearance)
        position = (west+seed[0]*size, south+seed[1]*size)
        row = max(0, min(179, math.floor(position[1]+90)))
        total, inland = winding(position, latitude_edges[row])
        if total > 0 or inland < 0:
            raise ValueError('Unexpected source ring winding convention')
        start = len(stored_edges)
        stored_edges.extend(local.tolist())
        nodes[index] = (0, start, len(local), total, inland, *seed)
        histogram[len(local)] += 1

    for row in range(180):
        for column in range(360):
            index = row*360+column
            build(index, column-180, row-90, 1.0, buckets[index], 0)
        if row % 30 == 29:
            print(f'Indexed latitude through {row-89} degrees', flush=True)
    if 8*len(nodes)+5*len(stored_edges) > MAX_GPU_WORDS:
        raise ValueError('Vector index exceeds the reviewed GPU storage bound')
    data = bytearray(struct.pack('<8s6I', b'AEGLND02', 360, 180, len(nodes),
                                 len(stored_edges), EDGE_LIMIT, MAX_DEPTH))
    for node in nodes:
        data.extend(struct.pack('<3I2i2d', *node))
    for a, b, c, d, hole in stored_edges:
        data.extend(struct.pack('<4dI', a, b, c, d, int(hole)))
    output = ROOT/'assets/boundaries/global-land.aev'
    metadata = dict(
        schema='ancientearth.global-vector.v1',
        source_name='Natural Earth 1:10 million land v5.1.1',
        source_url='https://naciscdn.org/naturalearth/10m/physical/ne_10m_land.zip',
        source_page='https://www.naturalearthdata.com/downloads/10m-physical-vectors/10m-land/',
        license_url='https://www.naturalearthdata.com/about/terms-of-use/',
        license='Public domain', source_sha256=SOURCE_SHA,
        source_records=records, exterior_rings=len(shapes), inland_holes=len(holes),
        source_nondegenerate_segments=len(segments), output_file=output.name,
        output_sha256=hashlib.sha256(data).hexdigest(), nodes=len(nodes),
        indexed_segment_fragments=len(stored_edges), root_grid=[360, 180],
        leaf_edge_limit=EDGE_LIMIT, maximum_depth=MAX_DEPTH, deepest_leaf=deepest,
        leaf_edge_count_histogram=histogram,
        minimum_seed_clearance_cell_units=minimum_clearance,
        encoding='AEGLND02; root width,height,node count,edge count,edge cap,depth cap; '
                 'nodes=(children,start,count,winding,hole winding,seed x,y); '
                 'edges=(ax,ay,bx,by,hole flag), all coordinates float64 normalized in the leaf',
        modification='Original directed longitude/latitude polygon segments are clipped only for '
                     'adaptive indexing. Signed winding from an interior source-classified seed '
                     'evaluates the original vector boundary. No raster coastline, invented '
                     'smoothing, height, geoid or photographic input.',
        scope='Source-scale cartographic context, not surveyed shoreline or water height. '
              'Original regional reconstructed boundaries and native WBM retain priority. '
              'The land product is not a complete inland-water inventory; finite arithmetic '
              'can choose either adjacent category on an exact source boundary.')
    meta = output.with_suffix('.vector.provenance.json')
    if check:
        if output.read_bytes() != data or json.loads(meta.read_text()) != metadata:
            raise ValueError('Global vector regeneration differs')
    else:
        output.write_bytes(data)
        meta.write_text(json.dumps(metadata, indent=2)+'\n')
    print(f'Global vector: {len(nodes):,} nodes, {len(stored_edges):,} fragments, '
          f'{len(data):,} bytes, deepest {deepest}', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    prepare(parser.parse_args().check)
