"""Independent GDAL decode and PROJ registration of the original HRRR subset.

Uses the existing optional scientific Python environment. No source data is
modified. All required source values and native post identities are recorded.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path

os.environ["GDAL_PAM_ENABLED"] = "NO"
import pyproj
import rasterio

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "assets/weather/hrrr-reference.grib2"
EXPECTED = "c790d1e60909bfbf86c8b0060e7ace3e5f1fc83026b86a2a8aab411621c3b21f"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    assert hashlib.sha256(SOURCE.read_bytes()).hexdigest() == EXPECTED
    with rasterio.open(SOURCE) as src:
        assert (src.width, src.height, src.count) == (6, 8, 5)
        projection = pyproj.Proj(src.crs)
        first_x, first_y = projection(-122.719528, 21.138123)
        bands = {}
        for index in range(1, src.count + 1):
            tags = src.tags(index)
            assert tags["GRIB_REF_TIME"] == "1792908000"
            assert tags["GRIB_VALID_TIME"] == "1792918800"
            bands[(tags["GRIB_ELEMENT"], tags["GRIB_SHORT_NAME"])] = src.read(index)
        p = bands[("PRES", "0-SFC")]
        h = bands[("HGT", "0-SFC")]
        t = bands[("TMP", "2-HTGL")] + 273.15  # GDAL explicitly returns Celsius.
        q = bands[("SPFH", "2-HTGL")]
        rows = []
        registration_error = 0.0
        for row in range(src.height):
            for col in range(src.width):
                x, y = src.xy(row, col)
                i, j = round((x - first_x) / 3000), round((y - first_y) / 3000)
                canonical_x, canonical_y = first_x + i * 3000, first_y + j * 3000
                registration_error = max(registration_error, ((x-canonical_x)**2+(y-canonical_y)**2)**0.5)
                lon, lat = projection(canonical_x, canonical_y, inverse=True)
                rows.append([i, j, lat, lon, float(p[row,col]), float(h[row,col]), float(t[row,col]), float(q[row,col])])
        assert registration_error < 0.25
        points = []
        for lat, lon in [(29.305549,-94.767015),(38.829149,-106.415415),(40.7,-74.0),(47.6,-122.3),(25.8,-80.2),(49.0,-100.0)]:
            x,y = projection(lon,lat)
            i,j = int((x-first_x)/3000+0.5),int((y-first_y)/3000+0.5)
            selected_lon,selected_lat = projection(first_x+i*3000,first_y+j*3000,inverse=True)
            points.append([lat,lon,i,j,selected_lat,selected_lon])
        result = {"source_sha256":EXPECTED,"independent_decoder":"GDAL via Rasterio; PROJ via pyproj", "source_crs":src.crs.to_wkt(), "source_shape":[src.height,src.width], "maximum_native_registration_error_m":registration_error, "rows":rows,"selection_cases":points}
    gfs_source = ROOT / "assets/weather/gfs-regional-reference.grib2"
    gfs_hash = "275eff0d7ad21b3d14dbeaac821aa4f5f154558c419374b838a2f89c56e827fd"
    assert hashlib.sha256(gfs_source.read_bytes()).hexdigest() == gfs_hash
    fields = {}
    with rasterio.Env(GRIB_NORMALIZE_UNITS="NO"):
        with rasterio.open(gfs_source) as src:
            assert (src.width,src.height)==(3,3)
            assert src.xy(1,1)==(-94.75,29.25)
            for band in range(1,src.count+1):
                tags=src.tags(band)
                level,element=tags["GRIB_SHORT_NAME"],tags["GRIB_ELEMENT"]
                if not (level.endswith("-ISBL") or level in ("0-SFC","2-HTGL","80-HTGL")):
                    continue
                assert tags["GRIB_REF_TIME"]=="1792886400"
                assert tags["GRIB_VALID_TIME"]=="1792918800"
                assert (level,element) not in fields
                fields[level,element]=float(src.read(band)[1,1])
    surface=[fields['0-SFC','PRES'],fields['0-SFC','HGT'],fields['2-HTGL','TMP'],fields['2-HTGL','SPFH'],fields['80-HTGL','TMP'],fields['80-HTGL','PRES'],fields['80-HTGL','SPFH']]
    pressures=sorted({int(level.removesuffix('-ISBL')) for level,_ in fields if level.endswith('-ISBL')},reverse=True)
    levels=[[float(p),fields[f'{p}-ISBL','HGT'],fields[f'{p}-ISBL','TMP'],fields[f'{p}-ISBL','SPFH']] for p in pressures]
    result.update(gfs_source_sha256=gfs_hash,gfs_surface=surface,gfs_pressure_levels=levels)
    target = ROOT / "artifacts/weather-hrrr-source-review.json"
    report = json.dumps(result,indent=2)+"\n"
    fixture = "// Generated independently by scripts/verify_hrrr_column.py using GDAL/PROJ.\n// i,j,latitude,longitude,surface_Pa,surface_gpm,2m_K,2m_kg_per_kg\n#[rustfmt::skip]\npub const POSTS: &[[f64;8]] = &[\n"
    fixture += "".join("    ["+", ".join(repr(float(v)) for v in row)+"],\n" for row in rows)+"];\n"
    fixture += "// requested latitude/longitude, native i/j, selected latitude/longitude\n#[rustfmt::skip]\npub const SELECTIONS: &[[f64;6]] = &[\n"
    fixture += "".join("    ["+", ".join(repr(float(v)) for v in row)+"],\n" for row in points)+"];\n"
    fixture += "// GFS: surface pressure/height,2m temperature/humidity,80m temperature/pressure/humidity\n#[rustfmt::skip]\npub const GFS_SURFACE:[f64;7] = ["+", ".join(repr(v) for v in surface)+"];\n"
    fixture += "#[rustfmt::skip]\npub const GFS_LEVELS:&[[f64;4]] = &[\n"+"".join("    ["+", ".join(repr(v) for v in row)+"],\n" for row in levels)+"];\n"
    fixture_path = ROOT / "crates/earth-math/tests/fixtures/weather_hrrr_reference.rs"
    if args.check:
        if fixture_path.read_text(encoding="utf-8") != fixture:
            raise SystemExit("independent HRRR/GFS fixture differs; review before regeneration")
        if target.exists() and target.read_text(encoding="utf-8") != report:
            raise SystemExit("independent HRRR/GFS audit report differs; review before regeneration")
    else:
        target.write_text(report,encoding="utf-8",newline="\n")
        fixture_path.write_text(fixture,encoding="utf-8",newline="\n")
    print(f"48 native posts, 4 required fields each; registration error {registration_error:.9f} m")
    print(target)


if __name__ == "__main__":
    main()
