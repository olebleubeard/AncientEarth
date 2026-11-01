//! Bounded source-lattice refinement, with conforming transition fans.
use super::*;
use crate::Geodetic;

pub(super) struct MeshPlan {
    pub width: usize,
    pub height: usize,
    pub first_row: usize,
    /// Original regular-grid vertices first, then additional actual source posts.
    pub points: Vec<(usize, usize)>,
    pub faces: Vec<[usize; 3]>,
    pub refined: bool,
}
pub(super) struct Detail {
    pub radius_m: f64,
    pub step: usize,
}
type Plans = BTreeMap<(i32, i32), MeshPlan>;

fn shape(
    tile: &CopernicusTile,
    available: &BTreeSet<(i32, i32)>,
    n: usize,
) -> (usize, usize, usize) {
    let width = n + usize::from(available.contains(&(tile.south_deg, east(tile.west_deg))));
    let south = available.contains(&(tile.south_deg - 1, tile.west_deg));
    let diagonal = available.contains(&(tile.south_deg - 1, east(tile.west_deg)));
    (
        width,
        n + usize::from(south && (width == n || diagonal)),
        usize::from(tile.south_deg == 89),
    )
}
fn selected_cells(
    tiles: &[CopernicusTile],
    available: &BTreeSet<(i32, i32)>,
    region: &DownloadRegion,
    step: usize,
    radius: f64,
) -> Result<BTreeSet<(usize, usize)>, TopographyError> {
    let bounds = DownloadRegion::new(region.center, radius)?.query_boxes;
    let n = UNITS_PER_DEGREE / step;
    let mut cells = BTreeSet::new();
    for tile in tiles
        .iter()
        .filter(|t| t.core && available.contains(&(t.south_deg, t.west_deg)))
    {
        let (w, h, first) = shape(tile, available, n);
        for row in first..h - 1 {
            let north =
                f64::from(tile.south_deg + 1) - row as f64 * step as f64 / UNITS_PER_DEGREE as f64;
            let south = north - step as f64 / UNITS_PER_DEGREE as f64;
            for column in 0..w - 1 {
                let west = f64::from(tile.west_deg)
                    + column as f64 * step as f64 / UNITS_PER_DEGREE as f64;
                let east = west + step as f64 / UNITS_PER_DEGREE as f64;
                if bounds.iter().any(|b| {
                    west <= b.east_deg
                        && east >= b.west_deg
                        && south <= b.north_deg
                        && north >= b.south_deg
                }) {
                    cells.insert((
                        (89 - tile.south_deg) as usize * n + row,
                        (tile.west_deg + 180) as usize * n + column,
                    ));
                }
            }
        }
    }
    Ok(cells)
}
pub(super) fn plans(
    tiles: &[CopernicusTile],
    available: &BTreeSet<(i32, i32)>,
    region: &DownloadRegion,
    step: usize,
    cancel: &AtomicBool,
) -> Result<(Plans, Detail), TopographyError> {
    let n = UNITS_PER_DEGREE / step;
    let fine = (2..=step)
        .find(|fine| {
            step.is_multiple_of(*fine)
                && fine.is_multiple_of(2)
                && tiles.iter().all(|tile| {
                    fine.is_multiple_of(UNITS_PER_DEGREE / native_width(tile.south_deg))
                })
        })
        .unwrap();
    let ratio = step / fine;
    let mut radius = region.radius_m.min(5000.0);
    let mut selected = selected_cells(tiles, available, region, step, radius)?;
    // This intentionally overcounts every fine-cell edge and all neighboring
    // transition edges. It bounds additional unique vertices without assuming
    // deduplication savings; no source height is averaged to fit the budget.
    let bound = |count: usize| count.saturating_mul((ratio + 1).pow(2) + 4 * (ratio + 1) + 4);
    for _ in 0..12 {
        if bound(selected.len()) <= MAX_LOCAL_VERTICES || ratio == 1 {
            break;
        }
        radius *= 0.5;
        selected = selected_cells(tiles, available, region, step, radius)?;
    }
    if bound(selected.len()) > MAX_LOCAL_VERTICES || ratio == 1 || selected.is_empty() {
        selected.clear();
        radius = 0.0;
    }
    let mut result = BTreeMap::new();
    for tile in tiles
        .iter()
        .filter(|t| t.core && available.contains(&(t.south_deg, t.west_deg)))
    {
        cancelled(cancel)?;
        let (width, height, first_row) = shape(tile, available, n);
        let mut points = Vec::with_capacity(width * (height - first_row));
        let mut indices = BTreeMap::new();
        for row in first_row..height {
            for column in 0..width {
                indices.insert((row * step, column * step), points.len());
                points.push((row * step, column * step));
            }
        }
        let mut faces = Vec::new();
        let mut refined = false;
        let mut index = |p: (usize, usize)| {
            *indices.entry(p).or_insert_with(|| {
                let id = points.len();
                points.push(p);
                id
            })
        };
        for row in first_row..height - 1 {
            cancelled(cancel)?;
            for column in 0..width - 1 {
                let global = (
                    (89 - tile.south_deg) as usize * n + row,
                    (tile.west_deg + 180) as usize * n + column,
                );
                let neighbors = [
                    (global.0, (global.1 + 360 * n - 1) % (360 * n)),
                    (global.0 + 1, global.1),
                    (global.0, (global.1 + 1) % (360 * n)),
                    (global.0.saturating_sub(1), global.1),
                ];
                let edges = neighbors.map(|g| selected.contains(&g));
                let r = row * step;
                let c = column * step;
                if selected.contains(&global) {
                    refined = true;
                    for y in 0..ratio {
                        for x in 0..ratio {
                            let nw = index((r + y * fine, c + x * fine));
                            let sw = index((r + (y + 1) * fine, c + x * fine));
                            let se = index((r + (y + 1) * fine, c + (x + 1) * fine));
                            let ne = index((r + y * fine, c + (x + 1) * fine));
                            faces.extend([[nw, sw, se], [nw, se, ne]]);
                        }
                    }
                } else if edges.into_iter().any(|v| v) {
                    refined = true;
                    let corners = [(r, c), (r + step, c), (r + step, c + step), (r, c + step)];
                    let mut perimeter = Vec::new();
                    for edge in 0..4 {
                        let a = corners[edge];
                        let b = corners[(edge + 1) % 4];
                        let divisions = if edges[edge] { ratio } else { 1 };
                        for k in 0..divisions {
                            let interpolate = |a: usize, b: usize| {
                                if b >= a {
                                    a + (b - a) * k / divisions
                                } else {
                                    a - (a - b) * k / divisions
                                }
                            };
                            perimeter.push(index((interpolate(a.0, b.0), interpolate(a.1, b.1))));
                        }
                    }
                    let center = index((r + (ratio / 2) * fine, c + (ratio / 2) * fine));
                    for i in 0..perimeter.len() {
                        faces.push([center, perimeter[i], perimeter[(i + 1) % perimeter.len()]]);
                    }
                } else {
                    let nw = index((r, c));
                    let sw = index((r + step, c));
                    let se = index((r + step, c + step));
                    let ne = index((r, c + step));
                    faces.extend([[nw, sw, se], [nw, se, ne]]);
                }
            }
        }
        result.insert(
            (tile.south_deg, tile.west_deg),
            MeshPlan {
                width,
                height: height - first_row,
                first_row,
                points,
                faces,
                refined,
            },
        );
    }
    let count: usize = result.values().map(|p| p.points.len()).sum();
    if count > MAX_RETAINED_VERTICES {
        return Err(TopographyError::TooManyTiles);
    }
    Ok((
        result,
        Detail {
            radius_m: radius,
            step: fine,
        },
    ))
}

/// A duplicated south/east post belongs to the neighboring official raster.
pub(super) fn source_point(
    tile: (i32, i32),
    point: (usize, usize),
) -> ((i32, i32), (usize, usize)) {
    let south = tile.0 - i32::from(point.0 == UNITS_PER_DEGREE);
    let west = if point.1 == UNITS_PER_DEGREE {
        east(tile.1)
    } else {
        tile.1
    };
    (
        (south, west),
        (point.0 % UNITS_PER_DEGREE, point.1 % UNITS_PER_DEGREE),
    )
}
pub(super) fn position(
    tile: (i32, i32),
    point: (usize, usize),
    height: f64,
) -> Result<Geodetic, TopographyError> {
    Geodetic::new(
        f64::from(tile.0 + 1) - point.0 as f64 / UNITS_PER_DEGREE as f64,
        f64::from(tile.1) + point.1 as f64 / UNITS_PER_DEGREE as f64,
        height,
    )
    .map_err(TopographyError::InvalidCoordinate)
}
