"""Independent high-precision audit of saved canonical scene-ceiling metadata.

The complete terrain mesh is not duplicated in this report: all saved marker
corners and each saved terrain maximizing vertex are checked, while aggregate
terrain coverage also relies on the separately reviewed preparation loop.
"""
from decimal import Decimal, localcontext
import hashlib
import json
from pathlib import Path
import struct
import sys


A = 6378137.0
B = A * (1.0 - 1.0 / 298.257223563)


def radial_upper(point):
    """Exact encoded inputs with 80-digit arithmetic, not f64 interval steps."""
    with localcontext() as context:
        context.prec = 80
        p = list(map(Decimal.from_float, point))
        axes = list(map(Decimal.from_float, (A, A, B)))
        s = sum((x / axis) ** 2 for x, axis in zip(p, axes))
        if s <= 1:
            return Decimal(0)
        return sum(x*x for x in p).sqrt() * (s-1) / (s.sqrt() * (s.sqrt()+1))


def review(path):
    report = json.loads(path.read_text(encoding="utf-8"))
    terrain, markers = report["terrain"], report["markers"]
    assert report["h0_fallback_included"] is True
    assert len({row["patch"] for row in terrain}) == len(terrain)
    assert len({row["marker_id"] for row in markers}) == len(markers)
    saved_points = 0
    minimum_margin = Decimal("Infinity")
    for row in terrain + markers:
        ceiling = Decimal.from_float(row["canonical_height_ceiling_m"])
        assert ceiling >= 0
        packed = row["packed_canonical_ceiling_m"]
        assert struct.unpack("<f", struct.pack("<f", packed))[0] == packed
        assert Decimal.from_float(packed) >= ceiling
        if "canonical_prism_then_sign_corners_ecef_m" in row:
            points = row["canonical_prism_then_sign_corners_ecef_m"]
            assert len(points) == row["canonical_corner_count"] == 12
            assert row["maximizing_vertex_ecef_m"] in points
        else:
            points = [] if row["maximizing_vertex_ecef_m"] is None else [row["maximizing_vertex_ecef_m"]]
        for point in points:
            margin = ceiling - radial_upper(point)
            # The retained decimal coordinates are treated as exact binary64
            # inputs here; no inverse-geodesy solver shares the bound formula.
            assert margin >= 0, (row, point, margin)
            minimum_margin = min(minimum_margin, margin)
            saved_points += 1
    count = sum(row["vertices"] for row in terrain) + sum(row["canonical_corner_count"] for row in markers)
    assert count == report["canonical_vertex_count_including_marker_sign_corners"]
    assert report["canonical_height_ceiling_m"] == max(row["canonical_height_ceiling_m"] for row in terrain + markers)
    assert report["packed_canonical_ceiling_m"] >= report["canonical_height_ceiling_m"]
    return {
        "scope": __doc__.strip(),
        "input_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "terrain_patches": len(terrain), "terrain_vertices": sum(row["vertices"] for row in terrain),
        "markers": len(markers), "prism_and_sign_corners": sum(row["canonical_corner_count"] for row in markers),
        "total_vertices": count, "saved_points_checked_at_80_digits": saved_points,
        "minimum_recorded_ceiling_minus_exact_radial_bound_m": str(minimum_margin),
        "canonical_ceiling_m": report["canonical_height_ceiling_m"],
        "packed_canonical_ceiling_m": report["packed_canonical_ceiling_m"],
        "limitation": "Canonical height metadata only. Dynamic sign geometry and explicit representation/query/integration allowances remain required before any GPU skip. This does not certify all terrain vertices by independently reading the original mesh.",
    }


if __name__ == "__main__":
    path = Path(sys.argv[1])
    result = review(path)
    path.with_name(path.stem + "-independent-review.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
