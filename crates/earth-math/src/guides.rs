//! Canonical Earth silhouette guides and explicitly defined world-space rulers.
//! Runtime geometric guides project the analytic ellipsoid tangent contour;
//! optical and archived terrain searches retain explicit finite-search limits.
use crate::{CameraFrame, Geodetic, Lens, Observer, Ray, ScreenRect, SurfaceHit, TerrainScene};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};
mod analytic;

const COLUMNS: usize = 64;
const ROWS: usize = 36;
const BRACKET_PIXELS: f64 = 0.025;
const OPTICAL_COLUMNS: usize = 8;
const OPTICAL_ROWS: usize = 4;
const OPTICAL_QUERY_BUDGET: usize = 512;
static NEVER_CANCELLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideViewport {
    pub display: [f32; 2],
    pub scale: [f32; 2],
    pub pixels: [f64; 2],
}
impl GuideViewport {
    pub fn new(display: [f32; 2], scale: [f32; 2]) -> Option<Self> {
        if !display
            .into_iter()
            .chain(scale)
            .all(|v| v.is_finite() && v > 0.0)
        {
            return None;
        }
        let pixels = std::array::from_fn(|i| f64::from(display[i]) * f64::from(scale[i]));
        if !pixels.iter().all(|p| (1.0..=16_777_216.0).contains(p)) {
            return None;
        }
        Some(Self {
            display,
            scale,
            pixels,
        })
    }
    pub fn logical(self, point: [f64; 2]) -> [f32; 2] {
        std::array::from_fn(|i| (point[i] / f64::from(self.scale[i])) as f32)
    }
    fn contains(self, point: [f64; 2]) -> bool {
        (0..2).all(|i| point[i].is_finite() && (0.0..=self.pixels[i]).contains(&point[i]))
    }
    fn direction(self, frame: CameraFrame, lens: Lens, point: [f64; 2]) -> Option<crate::Vec3d> {
        self.contains(point).then_some(())?;
        lens.ray(
            frame,
            2.0 * point[0] / self.pixels[0] - 1.0,
            1.0 - 2.0 * point[1] / self.pixels[1],
            self.pixels[0] / self.pixels[1],
        )
    }
    fn project(self, frame: CameraFrame, lens: Lens, point: crate::Vec3d) -> Option<[f64; 2]> {
        let ndc = lens.project(frame, point - frame.origin, self.pixels[0] / self.pixels[1])?;
        Some([
            (ndc[0] + 1.0) * self.pixels[0] * 0.5,
            (1.0 - ndc[1]) * self.pixels[1] * 0.5,
        ])
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HorizonPoint {
    /// Physical framebuffer coordinates, including subpixel refinement.
    pub pixel: [f64; 2],
    pub hit: SurfaceHit,
    /// Width of this local hit/miss bracket, not a global error bound. Zero for
    /// constructed analytic tangent points does not mean zero numerical error.
    pub bracket_pixels: f64,
}

#[derive(Clone, Debug)]
pub struct HorizonProfile {
    pub apex: HorizonPoint,
    pub left: HorizonPoint,
    pub right: HorizonPoint,
    pub chord_width_m: Option<f64>,
    /// Shortest WGS84 ellipsoid geodesic between the selected endpoints'
    /// latitudes and longitudes; terrain heights are projected to zero.
    pub surface_width_m: Option<f64>,
    /// Screen search spacing, or zero when constructing the analytic contour.
    pub coarse_spacing_pixels: [f64; 2],
    pub viewport: GuideViewport,
    frame: CameraFrame,
    lens: Lens,
    atmosphere: Option<crate::atmosphere::AtmosphereProfile>,
}

#[derive(Clone, Copy, Debug)]
pub struct GuideRuler {
    pub ground: HorizonPoint,
    /// Constructed guide reference at the measured ground's camera depth.
    /// This is not a ray hit in the sky or a geodetic height reference.
    pub reference_ecef_m: crate::Vec3d,
    pub view_plane_gap_m: f64,
    /// Euclidean eye-to-selected-endpoint chord, including in refracted mode;
    /// this is neither the curved geometric path nor the phase optical path.
    pub eye_range_m: f64,
    pub top_pixel: [f64; 2],
}

#[derive(Clone, Copy)]
struct Sample {
    pixel: [f64; 2],
    hit: bool,
    optical_surface: Option<SurfaceHit>,
}
#[derive(Clone, Copy)]
struct Boundary {
    pixel: [f64; 2],
    bracket_pixels: f64,
}
struct Search<'a> {
    scene: &'a TerrainScene,
    atmosphere: Option<&'a crate::atmosphere::AtmosphereProfile>,
    frame: CameraFrame,
    lens: Lens,
    viewport: GuideViewport,
    samples: RefCell<HashMap<[u64; 2], Option<Sample>>>,
    cancelled: &'a AtomicBool,
    queries: Cell<usize>,
}
impl Search<'_> {
    fn sample(&self, pixel: [f64; 2]) -> Option<Sample> {
        if self.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let key = pixel.map(f64::to_bits);
        if let Some(sample) = self.samples.borrow().get(&key) {
            return *sample;
        }
        let sample = self
            .viewport
            .direction(self.frame, self.lens, pixel)
            .and_then(|direction| {
                let ray = Ray {
                    origin: self.frame.origin,
                    direction,
                };
                let (hit, optical_surface) = if let Some(profile) = self.atmosphere {
                    if self.queries.get() >= OPTICAL_QUERY_BUDGET {
                        return None;
                    }
                    self.queries.set(self.queries.get() + 1);
                    // An unresolved ray is not a valid sky sample or a contour.
                    let surface = profile.trace(ray, self.scene).ok()?.surface;
                    (surface.is_some(), surface)
                } else {
                    (self.scene.intersects_any(ray), None)
                };
                Some(Sample {
                    pixel,
                    hit,
                    optical_surface,
                })
            });
        self.samples.borrow_mut().insert(key, sample);
        sample
    }
    fn surface(&self, pixel: [f64; 2]) -> Option<SurfaceHit> {
        if self.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        if self.atmosphere.is_some() {
            return self.sample(pixel)?.optical_surface;
        }
        let direction = self.viewport.direction(self.frame, self.lens, pixel)?;
        let ray = Ray {
            origin: self.frame.origin,
            direction,
        };
        self.scene.intersect(ray)
    }
    fn resolved(&self, boundary: Boundary) -> Option<HorizonPoint> {
        Some(HorizonPoint {
            pixel: boundary.pixel,
            bracket_pixels: boundary.bracket_pixels,
            hit: self.surface(boundary.pixel)?,
        })
    }
    fn boundary(&self, a: Sample, b: Sample) -> Option<Boundary> {
        if a.hit == b.hit {
            return None;
        }
        let (mut hit, mut miss) = if a.hit { (a, b) } else { (b, a) };
        for _ in 0..40 {
            if pixel_distance(hit.pixel, miss.pixel) <= BRACKET_PIXELS {
                break;
            }
            let middle = std::array::from_fn(|i| (hit.pixel[i] + miss.pixel[i]) * 0.5);
            let sample = self.sample(middle)?;
            if sample.hit {
                hit = sample;
            } else {
                miss = sample;
            }
        }
        Some(Boundary {
            pixel: hit.pixel,
            bracket_pixels: pixel_distance(hit.pixel, miss.pixel),
        })
    }
    fn segment_boundaries(&self, a: [f64; 2], b: [f64; 2], steps: usize) -> Vec<Boundary> {
        let mut result = Vec::new();
        let mut previous = None;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let point = std::array::from_fn(|axis| a[axis] + (b[axis] - a[axis]) * t);
            let sample = self.sample(point);
            if let (Some(old), Some(new)) = (previous, sample)
                && let Some(boundary) = self.boundary(old, new)
            {
                result.push(boundary);
            }
            previous = sample;
        }
        result
    }
    fn refine_extreme(&self, mut best: Boundary, axis: usize, minimize: bool) -> Boundary {
        // A detected crossing on this viewport edge is already the clipped
        // extremum; searching beyond it cannot improve a visible endpoint.
        if (minimize && best.pixel[axis] == 0.0)
            || (!minimize && best.pixel[axis] == self.viewport.pixels[axis])
        {
            return best;
        }
        let columns = if self.atmosphere.is_some() {
            OPTICAL_COLUMNS
        } else {
            COLUMNS
        };
        let rows = if self.atmosphere.is_some() {
            OPTICAL_ROWS
        } else {
            ROWS
        };
        let tangent_axis = 1 - axis;
        let mut step = self.viewport.pixels[tangent_axis]
            / if tangent_axis == 0 {
                columns as f64
            } else {
                rows as f64
            };
        let span = self.viewport.pixels[axis]
            / if axis == 0 {
                columns as f64
            } else {
                rows as f64
            };
        for _ in 0..if self.atmosphere.is_some() { 4 } else { 12 } {
            if self.cancelled.load(Ordering::Relaxed) {
                break;
            }
            for sign in [-1.0, 1.0] {
                let mut a = best.pixel;
                a[tangent_axis] += sign * step;
                a[axis] = (best.pixel[axis] - span).max(0.0);
                let mut b = a;
                b[axis] = (best.pixel[axis] + span).min(self.viewport.pixels[axis]);
                for point in self.segment_boundaries(a, b, 4) {
                    let improves = if minimize {
                        point.pixel[axis] < best.pixel[axis]
                    } else {
                        point.pixel[axis] > best.pixel[axis]
                    };
                    if improves {
                        best = point;
                    }
                }
            }
            step *= 0.5;
        }
        best
    }
}

fn pixel_distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl HorizonProfile {
    /// Find the visible canonical Earth outline, excluding grass, bodies and UI.
    /// Smooth Earth uses its analytic tangent contour; archived terrain retains
    /// a finite screen search with no global-maximum guarantee.
    pub fn measure(
        scene: &TerrainScene,
        frame: CameraFrame,
        lens: Lens,
        viewport: GuideViewport,
    ) -> Option<Self> {
        Self::measure_with_atmosphere(scene, frame, lens, viewport, None)
    }
    /// Bounded visible-screen search using the validated optical profile.
    pub fn measure_refracted(
        scene: &TerrainScene,
        frame: CameraFrame,
        lens: Lens,
        viewport: GuideViewport,
        profile: &crate::atmosphere::AtmosphereProfile,
    ) -> Option<Self> {
        Self::measure_with_atmosphere(scene, frame, lens, viewport, Some(profile))
    }
    pub fn measure_with_atmosphere(
        scene: &TerrainScene,
        frame: CameraFrame,
        lens: Lens,
        viewport: GuideViewport,
        atmosphere: Option<&crate::atmosphere::AtmosphereProfile>,
    ) -> Option<Self> {
        Self::measure_cancellable(scene, frame, lens, viewport, atmosphere, &NEVER_CANCELLED)
    }
    /// Cancellation is checked between queries. An individual bounded optical
    /// trace is allowed to finish; a cancelled result is never published.
    pub fn measure_cancellable(
        scene: &TerrainScene,
        frame: CameraFrame,
        lens: Lens,
        viewport: GuideViewport,
        atmosphere: Option<&crate::atmosphere::AtmosphereProfile>,
        cancelled: &AtomicBool,
    ) -> Option<Self> {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        if !frame.origin.is_finite() {
            return None;
        }
        if atmosphere.is_none() && (scene.is_boundary_only() || scene.patches().is_empty()) {
            let points = analytic::TangentContour::new(frame, lens, viewport)?.points(scene, None);
            let apex = *points
                .iter()
                .min_by(|a, b| a.pixel[1].total_cmp(&b.pixel[1]))?;
            let left = *points
                .iter()
                .min_by(|a, b| a.pixel[0].total_cmp(&b.pixel[0]))?;
            let right = *points
                .iter()
                .max_by(|a, b| a.pixel[0].total_cmp(&b.pixel[0]))?;
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            return Some(Self::from_points(
                apex, left, right, viewport, frame, lens, None, [0.0; 2],
            ));
        }
        let search = Search {
            scene,
            atmosphere,
            frame,
            lens,
            viewport,
            samples: RefCell::default(),
            cancelled,
            queries: Cell::new(0),
        };
        let columns = if atmosphere.is_some() {
            OPTICAL_COLUMNS
        } else {
            COLUMNS
        };
        let rows = if atmosphere.is_some() {
            OPTICAL_ROWS
        } else {
            ROWS
        };
        let mut points = Vec::new();
        let mut previous_row = vec![None; columns + 1];
        for row in 0..=rows {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            let mut previous = None;
            for (column, above) in previous_row.iter_mut().enumerate() {
                let sample = search.sample([
                    viewport.pixels[0] * column as f64 / columns as f64,
                    viewport.pixels[1] * row as f64 / rows as f64,
                ]);
                for neighbor in [previous, *above] {
                    if let (Some(a), Some(b)) = (neighbor, sample)
                        && let Some(point) = search.boundary(a, b)
                    {
                        points.push(point);
                    }
                }
                previous = sample;
                *above = sample;
            }
        }
        // Seed sharp terrain summits directly from canonical posts. This avoids
        // relying exclusively on the coarse image lattice for narrow peaks.
        let mut summits: Vec<[f64; 2]> = Vec::with_capacity(8);
        for patch in scene.patches().iter().filter(|_| !scene.is_boundary_only()) {
            for &vertex in patch.canonical_vertices() {
                if cancelled.load(Ordering::Relaxed) {
                    return None;
                }
                if let Some(pixel) = viewport.project(frame, lens, vertex)
                    && (summits.len() < 8 || pixel[1] < summits[7][1])
                {
                    let index = summits.partition_point(|p| p[1] <= pixel[1]);
                    summits.insert(index, pixel);
                    summits.truncate(8);
                }
            }
        }
        let vertical_step = viewport.pixels[1] / rows as f64;
        for summit in summits {
            points.extend(search.segment_boundaries(
                [summit[0], (summit[1] - vertical_step).max(0.0)],
                [summit[0], (summit[1] + 0.1).min(viewport.pixels[1])],
                4,
            ));
        }
        let apex = *points
            .iter()
            .min_by(|a, b| a.pixel[1].total_cmp(&b.pixel[1]))?;
        let left = *points
            .iter()
            .min_by(|a, b| a.pixel[0].total_cmp(&b.pixel[0]))?;
        let right = *points
            .iter()
            .max_by(|a, b| a.pixel[0].total_cmp(&b.pixel[0]))?;
        let apex = search.resolved(search.refine_extreme(apex, 1, true))?;
        let left = search.resolved(search.refine_extreme(left, 0, true))?;
        let right = search.resolved(search.refine_extreme(right, 0, false))?;
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        Some(Self::from_points(
            apex,
            left,
            right,
            viewport,
            frame,
            lens,
            atmosphere,
            [viewport.pixels[0] / columns as f64, vertical_step],
        ))
    }
    #[allow(clippy::too_many_arguments)]
    fn from_points(
        apex: HorizonPoint,
        left: HorizonPoint,
        right: HorizonPoint,
        viewport: GuideViewport,
        frame: CameraFrame,
        lens: Lens,
        atmosphere: Option<&crate::atmosphere::AtmosphereProfile>,
        coarse_spacing_pixels: [f64; 2],
    ) -> Self {
        let chord_width_m = (right.pixel[0] - left.pixel[0] > BRACKET_PIXELS)
            .then(|| left.hit.hit.position.distance(right.hit.hit.position));
        let surface_width_m = chord_width_m.and_then(|_| {
            crate::wgs84_surface_distance(left.hit.hit.geodetic, right.hit.hit.geodetic).ok()
        });
        Self {
            apex,
            left,
            right,
            chord_width_m,
            surface_width_m,
            viewport,
            frame,
            lens,
            coarse_spacing_pixels,
            atmosphere: atmosphere.cloned(),
        }
    }
    pub fn ruler(&self, scene: &TerrainScene, column: f64) -> Option<GuideRuler> {
        self.ruler_with_atmosphere(scene, column, self.atmosphere.as_ref())
    }
    pub fn ruler_refracted(
        &self,
        scene: &TerrainScene,
        column: f64,
        profile: &crate::atmosphere::AtmosphereProfile,
    ) -> Option<GuideRuler> {
        self.ruler_with_atmosphere(scene, column, Some(profile))
    }
    pub fn ruler_with_atmosphere(
        &self,
        scene: &TerrainScene,
        column: f64,
        atmosphere: Option<&crate::atmosphere::AtmosphereProfile>,
    ) -> Option<GuideRuler> {
        self.ruler_cancellable(scene, column, atmosphere, &NEVER_CANCELLED)
    }
    pub fn ruler_cancellable(
        &self,
        scene: &TerrainScene,
        column: f64,
        atmosphere: Option<&crate::atmosphere::AtmosphereProfile>,
        cancelled: &AtomicBool,
    ) -> Option<GuideRuler> {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        if self.atmosphere.as_ref().map(|profile| profile.layers())
            != atmosphere.map(|profile| profile.layers())
        {
            return None;
        }
        if !column.is_finite() || !(0.0..=1.0).contains(&column) {
            return None;
        }
        let x = column * self.viewport.pixels[0];
        let search = Search {
            scene,
            atmosphere,
            frame: self.frame,
            lens: self.lens,
            viewport: self.viewport,
            samples: RefCell::default(),
            cancelled,
            queries: Cell::new(0),
        };
        let analytic =
            atmosphere.is_none() && (scene.is_boundary_only() || scene.patches().is_empty());
        let intervals = if atmosphere.is_some() { 16 } else { 128 };
        let mut rows: Vec<f64> = (0..=intervals)
            .map(|row| self.viewport.pixels[1] * row as f64 / intervals as f64)
            .collect();
        // A thin terrain band may fall between coarse rows. Probe the known
        // global summit's height explicitly; every seed still needs a real
        // hit at this column, rather than assuming the summit extends here.
        for offset in [-0.1, -BRACKET_PIXELS, 0.0, BRACKET_PIXELS, 0.1] {
            rows.push((self.apex.pixel[1] + offset).clamp(0.0, self.viewport.pixels[1]));
        }
        rows.sort_by(f64::total_cmp);
        rows.dedup();
        let mut previous: Option<Sample> = None;
        let mut ground = None;
        for row in rows.into_iter().filter(|_| !analytic) {
            let current = search.sample([x, row]);
            if let (Some(sky), Some(earth)) = (previous, current)
                && !sky.hit
                && earth.hit
            {
                ground = search.boundary(sky, earth);
                break;
            }
            previous = current;
        }
        let mut ground = if analytic {
            // Select a sky-to-Earth crossing, not a lower Earth-to-sky limb.
            let contour = analytic::TangentContour::new(self.frame, self.lens, self.viewport)?;
            contour
                .points(scene, Some(column))
                .into_iter()
                .filter(|point| contour.sky_to_earth(*point))
                .min_by(|a, b| a.pixel[1].total_cmp(&b.pixel[1]))?
        } else {
            search.resolved(ground?)?
        };
        // A different column may reveal a missed summit above the sampled apex.
        // Never draw a reversed ruler or silently promote it to a global bound.
        if ground.pixel[1] < self.apex.pixel[1] && !analytic {
            let expanded = ground.bracket_pixels + self.apex.pixel[1] - ground.pixel[1];
            if expanded > BRACKET_PIXELS {
                return None;
            }
            // Preserve the original sky endpoint and extend the local bracket
            // to an actual hit at the guide. This reconciles independently
            // refined subpixel crossings without drawing an upward ruler.
            let pixel = [x, self.apex.pixel[1]];
            ground = HorizonPoint {
                pixel,
                hit: search.surface(pixel)?,
                bracket_pixels: expanded,
            };
        }
        if atmosphere.is_some() && ground.pixel[1] >= self.apex.pixel[1] {
            let difference = ground.pixel[1] - self.apex.pixel[1];
            // If the guide lies inside this column's existing miss/hit bracket,
            // an actual hit on that exact launch ray can shorten the bracket.
            // Overlapping uncertainty intervals alone never imply zero gap.
            if difference <= ground.bracket_pixels {
                let pixel = [x, self.apex.pixel[1]];
                if let Some(hit) = search.surface(pixel) {
                    ground = HorizonPoint {
                        pixel,
                        hit,
                        bracket_pixels: ground.bracket_pixels - difference,
                    };
                }
            }
        }
        let top_pixel = [x, self.apex.pixel[1]];
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let direction = self.viewport.direction(self.frame, self.lens, top_pixel)?;
        let depth = (ground.hit.hit.position - self.frame.origin).dot(self.frame.forward);
        let denominator = direction.dot(self.frame.forward);
        if depth <= 0.0 || denominator <= 1e-10 {
            return None;
        }
        let reference_ecef_m = if top_pixel == ground.pixel {
            // Same launch ray and measured endpoint: its first surface hit
            // lies on the defining plane, without a second numerical trace.
            ground.hit.hit.position
        } else if let Some(profile) = atmosphere {
            profile
                .trace_to_plane(
                    Ray {
                        origin: self.frame.origin,
                        direction,
                    },
                    scene,
                    ground.hit.hit.position,
                    self.frame.forward,
                )
                .ok()??
        } else {
            self.frame.origin + direction * (depth / denominator)
        };
        let view_plane_gap_m = reference_ecef_m.distance(ground.hit.hit.position);
        if !view_plane_gap_m.is_finite() {
            return None;
        }
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        Some(GuideRuler {
            ground,
            reference_ecef_m,
            view_plane_gap_m,
            eye_range_m: ground.hit.hit.distance_m,
            top_pixel,
        })
    }
    /// Filled, non-antialiased row, exactly one physical framebuffer pixel.
    pub fn horizontal_rect(&self) -> ScreenRect {
        let row = self.apex.pixel[1]
            .floor()
            .clamp(0.0, self.viewport.pixels[1].floor() - 1.0);
        ScreenRect {
            min: self.viewport.logical([0.0, row]),
            max: self.viewport.logical([self.viewport.pixels[0], row + 1.0]),
        }
    }
    pub fn vertical_rect(&self, ruler: GuideRuler) -> ScreenRect {
        let column = ruler.top_pixel[0]
            .floor()
            .clamp(0.0, self.viewport.pixels[0].floor() - 1.0);
        ScreenRect {
            min: self.viewport.logical([column, ruler.top_pixel[1]]),
            max: self.viewport.logical([column + 1.0, ruler.ground.pixel[1]]),
        }
    }
    pub fn width_label_position(&self) -> [f32; 2] {
        let apex = self.viewport.logical(self.apex.pixel);
        [
            ((self.viewport.display[0] - 440.0) * 0.5).max(12.0),
            (apex[1] - 34.0).max(4.0),
        ]
    }
    pub fn ruler_label_position(&self, ruler: GuideRuler) -> [f32; 2] {
        let top = self.viewport.logical(ruler.top_pixel);
        let bottom = self.viewport.logical(ruler.ground.pixel);
        [
            (top[0] + 10.0).min((self.viewport.display[0] - 360.0).max(4.0)),
            ((top[1] + bottom[1]) * 0.5 + 8.0).min((self.viewport.display[1] - 40.0).max(4.0)),
        ]
    }
    pub fn marker_position(&self, column: f64) -> [f32; 2] {
        self.viewport.logical([
            column.clamp(0.0, 1.0) * self.viewport.pixels[0],
            self.apex.pixel[1],
        ])
    }
}

pub fn move_column(current: f64, axis: f64, elapsed_seconds: f64) -> f64 {
    if !current.is_finite() {
        return 0.5;
    }
    if !axis.is_finite() || !elapsed_seconds.is_finite() {
        return current.clamp(0.0, 1.0);
    }
    (current + axis.clamp(-1.0, 1.0) * elapsed_seconds.clamp(0.0, 0.25) * 0.35).clamp(0.0, 1.0)
}
pub fn column_from_screen(mouse_x: f32, width: f32) -> Option<f64> {
    (mouse_x.is_finite() && width.is_finite() && width > 0.0)
        .then(|| (f64::from(mouse_x) / f64::from(width)).clamp(0.0, 1.0))
}

#[derive(Clone, Copy, Debug)]
pub enum GuideDemo {
    HighAltitude,
    Rolled,
    SkyOnly,
}
pub fn demo_observer(demo: GuideDemo) -> Observer {
    let mut observer = Observer::new(Geodetic::new(0.0, 0.0, 1_000_000.0).unwrap());
    observer.heading_deg = 90.0;
    observer.pitch_deg = if matches!(demo, GuideDemo::SkyOnly) {
        90.0
    } else {
        -30.0
    };
    observer.roll_deg = if matches!(demo, GuideDemo::Rolled) {
        30.0
    } else {
        0.0
    };
    observer
}
