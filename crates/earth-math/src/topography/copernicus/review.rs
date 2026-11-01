//! Independent integer-lattice topology checks without large raster fixtures.
use super::*;
use crate::Geodetic;

#[test]
fn refined_plans_conform_across_latitude_bands_dateline_and_missing_support() {
    for (latitude, longitude, radius, omit) in [
        (49.999, 179.999, 500.0, false),
        (-60.001, -179.999, 500.0, false),
        (29.5, -95.0, 1.0, true),
        (89.99, 10.0, 10.0, false),
    ] {
        let region =
            DownloadRegion::new(Geodetic::new(latitude, longitude, 0.0).unwrap(), radius).unwrap();
        let tiles = copernicus_tiles(&region).unwrap();
        let available: BTreeSet<_> = tiles
            .iter()
            .filter(|t| !omit || t.core)
            .map(|t| (t.south_deg, t.west_deg))
            .collect();
        let step = retained_step(&tiles).unwrap();
        let (plans, detail) =
            refinement::plans(&tiles, &available, &region, step, &AtomicBool::new(false)).unwrap();
        assert!(plans.values().map(|p| p.points.len()).sum::<usize>() <= 500_000);
        if latitude == 49.999 {
            assert_eq!(detail.step, 6, "common1/1.5arcsec source lattice");
        }
        for (&tile, plan) in &plans {
            let count = plan.points.len();
            assert_eq!(
                plan.points.iter().copied().collect::<BTreeSet<_>>().len(),
                count
            );
            let mut incidence: BTreeMap<(usize, usize), (usize, i32)> = BTreeMap::new();
            let mut used = BTreeSet::new();
            let mut doubled_area = 0_i64;
            for face in &plan.faces {
                let p = face.map(|i| {
                    assert!(i < count);
                    used.insert(i);
                    plan.points[i]
                });
                // Coordinates are (column,equatorward-negative row), so this
                // is the signed geographic area in exact half-arcsecond units.
                let ax = p[1].1 as i64 - p[0].1 as i64;
                let ay = p[0].0 as i64 - p[1].0 as i64;
                let bx = p[2].1 as i64 - p[0].1 as i64;
                let by = p[0].0 as i64 - p[2].0 as i64;
                let area = ax * by - ay * bx;
                assert!(area > 0);
                doubled_area += area;
                for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                    let entry = incidence.entry((a.min(b), a.max(b))).or_default();
                    entry.0 += 1;
                    entry.1 += if a < b { 1 } else { -1 };
                }
            }
            assert_eq!(used.len(), count);
            assert_eq!(count + plan.faces.len(), incidence.len() + 1);
            assert_eq!(
                doubled_area,
                2 * ((plan.width - 1) * (plan.height - 1) * step * step) as i64
            );
            let rmin = plan.first_row * step;
            let rmax = rmin + (plan.height - 1) * step;
            let cmax = (plan.width - 1) * step;
            for ((a, b), (n, balance)) in incidence {
                if n == 2 {
                    assert_eq!(balance, 0);
                } else {
                    assert_eq!(n, 1);
                    let (a, b) = (plan.points[a], plan.points[b]);
                    assert!(
                        (a.0 == b.0 && (a.0 == rmin || a.0 == rmax))
                            || (a.1 == b.1 && (a.1 == 0 || a.1 == cmax)),
                        "unpaired interior edge/T junction"
                    );
                }
            }
            for &(row, column) in &plan.points {
                let owner = (
                    tile.0 - i32::from(row == 7200),
                    if column == 7200 {
                        (tile.1 + 181).rem_euclid(360) - 180
                    } else {
                        tile.1
                    },
                );
                assert!(available.contains(&owner));
                let abs_lat = (f64::from(owner.0) + 0.5).abs();
                let native_half_arcseconds = if abs_lat < 50.0 {
                    2
                } else if abs_lat < 60.0 {
                    3
                } else if abs_lat < 70.0 {
                    4
                } else if abs_lat < 80.0 {
                    6
                } else if abs_lat < 85.0 {
                    10
                } else {
                    20
                };
                assert_eq!(row % 2, 0);
                assert_eq!(column % native_half_arcseconds, 0);
            }
            // Every actually shared tile boundary includes exactly the same
            // fine and coarse posts, even across longitude wrap/latitude bands.
            if cmax == 7200
                && let Some(east_plan) = plans.get(&(tile.0, (tile.1 + 181).rem_euclid(360) - 180))
            {
                let west_edge: BTreeSet<_> = plan
                    .points
                    .iter()
                    .filter(|p| p.1 == 7200)
                    .map(|p| p.0)
                    .collect();
                let east_edge: BTreeSet<_> = east_plan
                    .points
                    .iter()
                    .filter(|p| p.1 == 0)
                    .map(|p| p.0)
                    .collect();
                assert_eq!(west_edge, east_edge);
            }
            if rmax == 7200
                && let Some(south_plan) = plans.get(&(tile.0 - 1, tile.1))
            {
                let north_edge: BTreeSet<_> = plan
                    .points
                    .iter()
                    .filter(|p| p.0 == 7200)
                    .map(|p| p.1)
                    .collect();
                let south_edge: BTreeSet<_> = south_plan
                    .points
                    .iter()
                    .filter(|p| p.0 == 0)
                    .map(|p| p.1)
                    .collect();
                assert_eq!(north_edge, south_edge);
            }
        }
    }
}
