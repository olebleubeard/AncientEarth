"""Independent original-vector audit of the coarse global categorical fallback.

Uses the GDAL/OGR C API already shipped with the reviewed Rasterio dependency to
decode the shapefile, rather than the preparation script's ESRI reader. Checks
every RLE row and deterministic source-raster windows; these finite windows do
not constitute a geographical accuracy bound. No downloads or new dependencies.
"""
from __future__ import annotations
import ctypes
import hashlib
import json
from pathlib import Path
import struct
import zipfile

import numpy as np
import rasterio
from rasterio.features import rasterize
from rasterio.transform import Affine

ROOT = Path(__file__).resolve().parents[1]


def source_polygons(source: Path):
    library_dir = Path(rasterio.__file__).resolve().parents[1] / "rasterio.libs"
    libraries = list(library_dir.glob("gdal-*.dll")) + list(library_dir.glob("libgdal*.so*"))
    if len(libraries) != 1:
        raise ValueError("Locate the existing Rasterio GDAL library; do not download another decoder")
    library = ctypes.CDLL(str(libraries[0]))
    pointer = ctypes.c_void_p

    def function(name, arguments, result):
        value = getattr(library, name)
        value.argtypes, value.restype = arguments, result
        return value

    function("GDALAllRegister", [], None)()
    open_dataset = function("GDALOpenEx", [ctypes.c_char_p, ctypes.c_uint, pointer, pointer, pointer], pointer)
    dataset = open_dataset(("/vsizip/" + source.resolve().as_posix() + "/ne_10m_land.shp").encode(), 4, None, None, None)
    if not dataset:
        raise ValueError("Independent OGR source open failed")
    get_layer = function("GDALDatasetGetLayer", [pointer, ctypes.c_int], pointer)
    get_feature = function("OGR_L_GetNextFeature", [pointer], pointer)
    get_geometry = function("OGR_F_GetGeometryRef", [pointer], pointer)
    export_json = function("OGR_G_ExportToJson", [pointer], pointer)
    free = function("VSIFree", [pointer], None)
    destroy = function("OGR_F_Destroy", [pointer], None)
    polygons, records = [], 0
    layer = get_layer(dataset, 0)
    try:
        while True:
            feature = get_feature(layer)
            if not feature:
                break
            text = export_json(get_geometry(feature))
            try:
                geometry = json.loads(ctypes.string_at(text))
                records += 1
                if geometry["type"] == "MultiPolygon":
                    polygons.extend(geometry["coordinates"])
                elif geometry["type"] == "Polygon":
                    polygons.append(geometry["coordinates"])
                else:
                    raise ValueError("Unexpected OGR geometry type")
            finally:
                free(text)
                destroy(feature)
    finally:
        function("GDALClose", [pointer], None)(dataset)
    return polygons, records


def verify():
    metadata = json.loads((ROOT / "assets/boundaries/global-land.provenance.json").read_text())
    source = ROOT / ".cache/natural-earth/ne_10m_land.zip"
    if hashlib.sha256(source.read_bytes()).hexdigest() != metadata["source_sha256"]:
        raise ValueError("Original source SHA identity differs")
    polygons, records = source_polygons(source)
    # This compares the complete custom parser output, including hole ownership,
    # with a different, established decoder; the raster tests below use OGR data.
    from prepare_global_land import polygons as preparation_parser
    with zipfile.ZipFile(source) as archive:
        prepared, _, _ = preparation_parser(archive.read("ne_10m_land.shp"))
    def key(polygon):
        return tuple(tuple(tuple(np.round(pair, 9)) for pair in ring) for ring in polygon)
    expected = {key(polygon): polygon for polygon in polygons}
    actual = {key(polygon): polygon for polygon in prepared}
    if len(expected) != len(polygons) or actual.keys() != expected.keys():
        raise ValueError("Custom shapefile reader differs from independent OGR polygons")
    coordinate_error = max(float(np.max(np.abs(a - np.asarray(b))))
                           for identifier, polygon in actual.items()
                           for a, b in zip(polygon, expected[identifier]))
    if coordinate_error > 2e-12:
        raise ValueError("Custom source coordinates differ beyond OGR JSON roundoff")
    raw = (ROOT / "assets/boundaries/global-land.ael").read_bytes()
    if hashlib.sha256(raw).hexdigest() != metadata["output_sha256"]:
        raise ValueError("Global asset SHA identity differs")
    magic, width, height, count = struct.unpack_from("<8sIII", raw)
    if magic != b"AEGLND01" or (width, height) != (36000, 18000):
        raise ValueError("Unexpected global format")
    offsets = np.frombuffer(raw, dtype="<u4", count=height + 1, offset=20)
    runs = np.frombuffer(raw, dtype="<u4", count=count, offset=20 + 4 * (height + 1))
    if offsets[0] != 0 or offsets[-1] != count or len(raw) != 20 + 4 * (height + 1 + count):
        raise ValueError("Malformed global row stream")
    totals = np.zeros(3, dtype=np.int64)
    for row in range(height):
        segment = runs[offsets[row]:offsets[row + 1]]
        ends = (segment >> 2).astype(np.int64)
        codes = segment & 3
        lengths = np.diff(np.r_[0, ends])
        if np.any(lengths <= 0) or ends[-1] != width or np.any(codes > 2):
            raise ValueError("Invalid global category row")
        for code in range(3):
            totals[code] += int(lengths[codes == code].sum())
    if totals.tolist() != metadata["category_counts_land_ocean_inland_water"]:
        raise ValueError("Global category totals differ")
    shapes = []
    holes = []
    for polygon in polygons:
        coordinates = np.asarray(polygon[0])
        bounds = (coordinates.min(axis=0), coordinates.max(axis=0))
        shapes.append((bounds, {"type": "Polygon", "coordinates": polygon}))
        for hole in polygon[1:]:
            points = np.asarray(hole)
            holes.append(((points.min(axis=0), points.max(axis=0)), {"type": "Polygon", "coordinates": [hole]}))
    # Coast, missing-DEM ocean, both dateline sides, poles, island archipelagos,
    # source-feature seams, and the archive's enclosed Caspian-water hole.
    centers = [(29.3,-94.77),(28,-93),(40,-100),(42,50),(49,50),(0,-140),
               (64,-171),(65,179),(65,-179),(-89,0),(89,0),(0,0),
               (36,139),(-42,-74),(75,100),(-18,178),(-18,-179),(0,120),
               (54,10),(20,-75),(-35,20),(0,36),(60,-45),(25,55)]
    tested = 0
    for latitude, longitude in centers:
        first_row = max(0, min(height - 256, int((90 - latitude) * 100) - 128))
        first_column = max(0, min(width - 256, int((longitude + 180) * 100) - 128))
        north, west = 90 - first_row / 100, -180 + first_column / 100
        south, east = north - 2.56, west + 2.56
        def intersects(bounds):
            low, high = bounds
            return low[0] <= east and high[0] >= west and low[1] <= north and high[1] >= south
        original = np.ones((256, 256), dtype=np.uint8)
        transform = Affine(0.01, 0, west, 0, -0.01, north)
        water = [(shape, 2) for bounds, shape in holes if intersects(bounds)]
        land = [(shape, 0) for bounds, shape in shapes if intersects(bounds)]
        if water:
            rasterize(water, out=original, transform=transform, all_touched=False)
        if land:
            rasterize(land, out=original, transform=transform, all_touched=False)
        columns = np.arange(first_column, first_column + 256)
        for index, row in enumerate(range(first_row, first_row + 256)):
            segment = runs[offsets[row]:offsets[row + 1]]
            values = segment[np.searchsorted(segment >> 2, columns, side="right")] & 3
            if not np.array_equal(values, original[index]):
                raise ValueError(f"Independent original-vector raster mismatch near {latitude},{longitude}, row{row}")
        tested += original.size
    report = dict(source_sha256=metadata["source_sha256"], source_records=records,
                  exterior_polygons=len(polygons), inland_holes=len(holes),
                  source_coordinate_max_difference_degrees=coordinate_error,
                  runs=count, all_cell_category_counts=totals.tolist(),
                  independent_raster_windows=len(centers), independently_compared_cells=tested)
    (ROOT / ".cache/global-land-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    verify()
