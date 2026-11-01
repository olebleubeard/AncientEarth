//! Separate, simultaneous heliocentric overview. Camera coordinates are ICRF
//! metres, not a geodetic Observer and not a change to first-person navigation.
use crate::{
    CameraFrame, Lens, Projection, Ray, ScreenRect, Vec3d, WGS84_A_M, WGS84_B_M,
    astro::{
        MOON_RADIUS_M, SUN_RADIUS_M,
        orbit::{OrbitFrame, OrbitTrail},
    },
};

pub const AU_M: f64 = 149_597_870_700.0;
pub const MAX_DISTANCE_M: f64 = 4.0 * AU_M;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitBody {
    Earth,
    Sun,
    Moon,
}
impl OrbitBody {
    pub fn name(self) -> &'static str {
        match self {
            Self::Earth => "Earth",
            Self::Sun => "Sun",
            Self::Moon => "Moon",
        }
    }
    pub fn center(self, frame: &OrbitFrame) -> Vec3d {
        match self {
            Self::Earth => frame.earth_position_m,
            Self::Sun => Vec3d::ZERO,
            Self::Moon => frame.moon_position_m,
        }
    }
    pub fn radius(self) -> f64 {
        match self {
            Self::Earth => WGS84_A_M,
            Self::Sun => SUN_RADIUS_M,
            Self::Moon => MOON_RADIUS_M,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    focus: OrbitBody,
    distance: f64,
    offset: Vec3d,
    up_hint: Vec3d,
}
impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            focus: OrbitBody::Earth,
            distance: 3.5 * WGS84_A_M,
            offset: Vec3d::new(1.0, 0.0, 0.0),
            up_hint: Vec3d::new(0.0, 0.0, 1.0),
        }
    }
}
impl OrbitCamera {
    pub fn lens(&self) -> Lens {
        Lens::new(Projection::Rectilinear, 60.0)
    }
    pub fn distance_m(&self) -> f64 {
        self.distance
    }
    pub fn focus_label(&self) -> &'static str {
        self.focus.name()
    }
    pub fn focus(&self) -> OrbitBody {
        self.focus
    }
    pub fn frame(&self, scene: &OrbitFrame) -> Option<CameraFrame> {
        let origin = self.focus.center(scene) + self.offset * self.distance;
        let forward = -self.offset;
        let right = forward.cross(self.up_hint).normalized()?;
        let up = right.cross(forward).normalized()?;
        origin.is_finite().then_some(CameraFrame {
            origin,
            forward,
            right,
            up,
        })
    }
    /// Reposition to a sunlit, oblique Earth close-up, retaining true body scale.
    pub fn fit_earth(&mut self, scene: &OrbitFrame) {
        if let (Some(radial), Some(normal)) = (
            scene.earth_position_m.normalized(),
            scene.orbital_normal_icrf.normalized(),
        ) {
            self.focus = OrbitBody::Earth;
            self.distance = 3.5 * WGS84_A_M;
            self.offset = (-radial + normal * 0.45).normalized().unwrap();
            self.up_hint = normal;
        }
    }
    /// Fit the annual path window approximately, without rescaling any body.
    pub fn fit_orbit(&mut self, scene: &OrbitFrame) {
        if let (Some(radial), Some(normal)) = (
            scene.earth_position_m.normalized(),
            scene.orbital_normal_icrf.normalized(),
        ) {
            self.focus = OrbitBody::Sun;
            self.distance = 2.8 * AU_M;
            self.offset = (radial * 0.6 + normal * 0.8).normalized().unwrap();
            self.up_hint = normal;
        }
    }
    /// Retarget from the same physical eye; reject a target containing the eye.
    pub fn aim(&mut self, body: OrbitBody, scene: &OrbitFrame) -> bool {
        let Some(frame) = self.frame(scene) else {
            return false;
        };
        let offset = frame.origin - body.center(scene);
        let distance = offset.length();
        let local = match body {
            OrbitBody::Earth => scene.earth_to_icrf,
            OrbitBody::Sun => scene.sun_to_icrf,
            OrbitBody::Moon => scene.moon_to_icrf,
        }
        .transpose()
        .transform(offset);
        let radius = body.radius();
        let normalized = Vec3d::new(
            local.x / radius,
            local.y / radius,
            local.z
                / if body == OrbitBody::Earth {
                    WGS84_B_M
                } else {
                    radius
                },
        );
        if distance.is_finite() && distance > 0.0 && normalized.dot(normalized) > 1.0 {
            self.focus = body;
            self.distance = distance;
            self.offset = offset / distance;
            if self.offset.cross(self.up_hint).length() < 1e-8 {
                self.up_hint = frame.up;
            }
            return true;
        }
        false
    }
    pub fn aim_earth(&mut self, frame: &OrbitFrame) -> bool {
        self.aim(OrbitBody::Earth, frame)
    }
    pub fn aim_sun(&mut self, frame: &OrbitFrame) -> bool {
        self.aim(OrbitBody::Sun, frame)
    }
    pub fn dolly(&mut self, wheel_lines: f64) {
        if wheel_lines.is_finite() {
            let lower = self.distance.min(self.focus.radius() * 1.05);
            let upper = self.distance.max(MAX_DISTANCE_M);
            self.distance = (self.distance * (-wheel_lines.clamp(-100.0, 100.0) * 0.18).exp())
                .clamp(lower, upper);
        }
    }
    /// Yaw around the retained orbital normal, pitch limited to avoid the pole.
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        let vertical = self.offset.dot(self.up_hint).clamp(-1.0, 1.0);
        let Some(horizontal) = (self.offset - self.up_hint * vertical).normalized() else {
            return;
        };
        let yaw = (-dx * 0.2).rem_euclid(360.0).to_radians();
        let horizontal = horizontal * yaw.cos() + self.up_hint.cross(horizontal) * yaw.sin();
        let pitch = (vertical.asin().to_degrees() + dy.clamp(-10000.0, 10000.0) * 0.2)
            .clamp(-89.0, 89.0)
            .to_radians();
        self.offset = (horizontal * pitch.cos() + self.up_hint * pitch.sin())
            .normalized()
            .unwrap();
    }
}

/// Independent f64 reference for the analytic body display. No atmosphere,
/// terrain, light time or aberration is included in this geometric overview.
pub fn body_intersection(body: OrbitBody, scene: &OrbitFrame, ray: Ray) -> Option<f64> {
    let direction = ray.direction.normalized()?;
    if !ray.origin.is_finite() {
        return None;
    }
    let rotation = match body {
        OrbitBody::Earth => scene.earth_to_icrf,
        OrbitBody::Sun => scene.sun_to_icrf,
        OrbitBody::Moon => scene.moon_to_icrf,
    }
    .transpose();
    let origin = rotation.transform(ray.origin - body.center(scene));
    let direction = rotation.transform(direction);
    let a = body.radius();
    let b = if body == OrbitBody::Earth {
        WGS84_B_M
    } else {
        a
    };
    let o = Vec3d::new(origin.x / a, origin.y / a, origin.z / b);
    let d = Vec3d::new(direction.x / a, direction.y / a, direction.z / b);
    let scale = d.length();
    let u = d / scale;
    let cross = o.cross(u);
    let discriminant = 1.0 - cross.dot(cross);
    if discriminant < 0.0 {
        return None;
    }
    let along = o.dot(u);
    let root = discriminant.sqrt();
    [(-along - root) / scale, (-along + root) / scale]
        .into_iter()
        .filter(|x| x.is_finite() && *x >= 0.0)
        .min_by(f64::total_cmp)
}

#[derive(Clone, Copy, Debug)]
pub struct OrbitStroke {
    pub a: [f32; 2],
    pub b: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct OrbitLabel {
    pub position: [f32; 2],
    pub label_position: [f32; 2],
    pub body: OrbitBody,
    pub small_body: bool,
}
#[derive(Clone, Debug)]
pub struct OrbitText {
    pub position: [f32; 2],
    pub anchor: [f32; 2],
    pub connect: bool,
    pub text: String,
}
#[derive(Default, Debug)]
pub struct OrbitAnnotations {
    pub trail: Vec<OrbitStroke>,
    pub plane: Vec<OrbitStroke>,
    pub axis: Vec<OrbitStroke>,
    pub normal: Vec<OrbitStroke>,
    pub bodies: Vec<OrbitLabel>,
    pub labels: Vec<OrbitText>,
    pub axis_exaggerated: bool,
}

/// Clamp a measured UI text rectangle without changing its scientific anchor.
pub fn bounded_label(anchor: [f32; 2], text_size: [f32; 2], display: [f32; 2]) -> [f32; 2] {
    std::array::from_fn(|i| {
        if ![anchor[i], text_size[i], display[i]]
            .into_iter()
            .all(f32::is_finite)
        {
            return 0.0;
        }
        anchor[i].clamp(0.0, (display[i] - text_size[i].max(0.0)).max(0.0))
    })
}

/// A measured caption in logical UI pixels. Scientific anchors remain fixed.
#[derive(Clone, Copy, Debug)]
pub struct OrbitCaption {
    pub anchor: [f32; 2],
    pub preferred: [f32; 2],
    pub text_size: [f32; 2],
    pub connect: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct OrbitCaptionPlacement {
    pub text_position: [f32; 2],
    pub rect: ScreenRect,
    pub leader: Option<OrbitStroke>,
}

/// Deterministic, finite candidate packing of measured captions. Backings have
/// four-pixel padding, a two-pixel viewport margin and two-pixel separation.
/// The resulting text positions agree with guide_caption's six-pixel clamp.
/// Connected anchors reserve an eight-pixel square. Infeasible captions are
/// omitted rather than overlapping or moving a physical marker. This is a
/// greedy display layout, not a claim of optimal rectangle packing.
pub fn caption_layout(
    captions: &[OrbitCaption],
    display: [f32; 2],
) -> Vec<Option<OrbitCaptionPlacement>> {
    let valid_display = display
        .into_iter()
        .all(|v| v.is_finite() && (4.0..=4_194_304.0).contains(&v));
    if !valid_display {
        return vec![None; captions.len()];
    }
    let finite_pair = |v: [f32; 2]| v.into_iter().all(f32::is_finite);
    let mut occupied: Vec<ScreenRect> = captions
        .iter()
        .filter(|c| c.connect && finite_pair(c.anchor))
        .map(|c| ScreenRect {
            min: std::array::from_fn(|i| (c.anchor[i] - 4.0).clamp(0.0, display[i])),
            max: std::array::from_fn(|i| (c.anchor[i] + 4.0).clamp(0.0, display[i])),
        })
        .collect();
    let disjoint = |a: ScreenRect, b: ScreenRect| {
        (0..2).any(|i| a.max[i] + 2.0 <= b.min[i] || b.max[i] + 2.0 <= a.min[i])
    };
    let mut result = Vec::with_capacity(captions.len());
    for caption in captions {
        if !finite_pair(caption.anchor)
            || !finite_pair(caption.preferred)
            || !finite_pair(caption.text_size)
            || caption.text_size.into_iter().any(|v| v < 0.0)
            || (0..2).any(|i| f64::from(caption.text_size[i]) + 12.0 > f64::from(display[i]))
        {
            result.push(None);
            continue;
        }
        let size: [f64; 2] = std::array::from_fn(|i| f64::from(caption.text_size[i]) + 8.0);
        let preferred: [f64; 2] = std::array::from_fn(|i| f64::from(caption.preferred[i]) - 4.0);
        let coordinates: [Vec<f64>; 2] = std::array::from_fn(|i| {
            let upper = f64::from(display[i]) - size[i] - 2.0;
            let mut values = vec![preferred[i].clamp(2.0, upper), 2.0, upper];
            for rect in &occupied {
                values.push((f64::from(rect.min[i]) - size[i] - 2.0).clamp(2.0, upper));
                values.push((f64::from(rect.max[i]) + 2.0).clamp(2.0, upper));
            }
            values.sort_by(f64::total_cmp);
            values.dedup();
            values
        });
        let mut best: Option<(f64, OrbitCaptionPlacement)> = None;
        for &x in &coordinates[0] {
            for &y in &coordinates[1] {
                let text_position = [(x + 4.0) as f32, (y + 4.0) as f32];
                // Match the actual UI backing arithmetic before testing fit.
                let rect = ScreenRect {
                    min: text_position.map(|v| v - 4.0),
                    max: std::array::from_fn(|i| text_position[i] + caption.text_size[i] + 4.0),
                };
                if (0..2).any(|i| rect.min[i] < 2.0 || rect.max[i] > display[i] - 2.0)
                    || occupied.iter().any(|r| !disjoint(rect, *r))
                {
                    continue;
                }
                let score = (x - preferred[0]).powi(2) + (y - preferred[1]).powi(2);
                if best
                    .as_ref()
                    .is_some_and(|(previous, _)| score >= *previous)
                {
                    continue;
                }
                let leader = caption.connect.then(|| OrbitStroke {
                    a: caption.anchor,
                    b: std::array::from_fn(|i| caption.anchor[i].clamp(rect.min[i], rect.max[i])),
                });
                best = Some((
                    score,
                    OrbitCaptionPlacement {
                        text_position,
                        rect,
                        leader,
                    },
                ));
            }
        }
        let placement = best.map(|(_, p)| p);
        if let Some(p) = placement {
            occupied.push(p.rect);
        }
        result.push(placement);
    }
    result
}

fn projected(frame: CameraFrame, point: Vec3d, width: f64, height: f64) -> Option<[f32; 2]> {
    let local = point - frame.origin;
    let z = local.dot(frame.forward);
    if z <= 1.0 {
        return None;
    }
    let focal = height / (2.0 * (30.0_f64.to_radians()).tan());
    let p = [
        width * 0.5 + focal * local.dot(frame.right) / z,
        height * 0.5 - focal * local.dot(frame.up) / z,
    ];
    (p[0] >= 0.0 && p[0] <= width && p[1] >= 0.0 && p[1] <= height).then_some(p.map(|x| x as f32))
}
fn stroke(frame: CameraFrame, a: Vec3d, b: Vec3d, width: f64, height: f64) -> Option<OrbitStroke> {
    let local = |p: Vec3d| {
        let d = p - frame.origin;
        Vec3d::new(d.dot(frame.right), d.dot(frame.up), d.dot(frame.forward))
    };
    let a = local(a);
    let b = local(b);
    let delta = b - a;
    let tan_y = (30.0_f64.to_radians()).tan();
    let tan_x = tan_y * width / height;
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for (v, d) in [
        (a.z - 1.0, delta.z),
        (a.z * tan_x + a.x, delta.z * tan_x + delta.x),
        (a.z * tan_x - a.x, delta.z * tan_x - delta.x),
        (a.z * tan_y + a.y, delta.z * tan_y + delta.y),
        (a.z * tan_y - a.y, delta.z * tan_y - delta.y),
    ] {
        if d == 0.0 {
            if v < 0.0 {
                return None;
            }
        } else {
            let t = -v / d;
            if d > 0.0 {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
        }
    }
    if lo > hi {
        return None;
    }
    let focal = height / (2.0 * tan_y);
    let screen = |p: Vec3d| {
        [
            (width * 0.5 + focal * p.x / p.z).clamp(0.0, width) as f32,
            (height * 0.5 - focal * p.y / p.z).clamp(0.0, height) as f32,
        ]
    };
    Some(OrbitStroke {
        a: screen(a + delta * lo),
        b: screen(a + delta * hi),
    })
}

/// Screen-space annotations, deliberately depth-independent. True body sizes
/// stay in the analytic pass. The plane/axis icon is enlarged only when needed
/// to remain legible, with axis_exaggerated reporting that display choice.
pub fn annotations(
    camera: OrbitCamera,
    scene: &OrbitFrame,
    trail: Option<&OrbitTrail>,
    display: [f32; 2],
    framebuffer_scale: [f32; 2],
    show_orbit: bool,
    show_axis: bool,
) -> OrbitAnnotations {
    if !display
        .into_iter()
        .chain(framebuffer_scale)
        .all(|v| v.is_finite() && v > 0.0)
    {
        return OrbitAnnotations::default();
    }
    let physical = std::array::from_fn(|i| display[i] * framebuffer_scale[i]);
    let mut out = annotations_in_pixels(camera, scene, trail, physical, show_orbit, show_axis);
    let logical = |p: [f32; 2]| [p[0] / framebuffer_scale[0], p[1] / framebuffer_scale[1]];
    for stroke in out
        .trail
        .iter_mut()
        .chain(&mut out.plane)
        .chain(&mut out.axis)
        .chain(&mut out.normal)
    {
        stroke.a = logical(stroke.a);
        stroke.b = logical(stroke.b);
    }
    for body in &mut out.bodies {
        body.position = logical(body.position);
        body.label_position = bounded_label(
            [body.position[0] + 8.0, body.position[1] - 24.0],
            [0.0; 2],
            display,
        );
    }
    for label in &mut out.labels {
        label.position = logical(label.position);
        label.anchor = logical(label.anchor);
    }
    // Captions use logical UI pixels, independently of the physical projection.
    // Their backing rectangles are wider/taller than the glyphs themselves.
    if let Some(earth) = out
        .bodies
        .iter()
        .find(|label| label.body == OrbitBody::Earth)
        .copied()
        && let Some(moon) = out
            .bodies
            .iter_mut()
            .find(|label| label.body == OrbitBody::Moon)
        && (moon.position[0] - earth.position[0]).hypot(moon.position[1] - earth.position[1]) < 64.0
    {
        moon.label_position = bounded_label(
            [earth.position[0] + 8.0, earth.position[1] + 48.0],
            [0.0; 2],
            display,
        );
    }
    let mut axis_caption = None;
    for label in &mut out.labels {
        if label.text == "Earth axis: CIP" {
            label.position = bounded_label(
                [label.position[0] + 8.0, label.position[1] - 8.0],
                [0.0, 21.0],
                display,
            );
            axis_caption = Some(label.position);
        } else if label.text == "Orbital-plane normal" {
            label.position = bounded_label(
                [label.position[0] + 8.0, label.position[1] - 8.0],
                [0.0, 21.0],
                display,
            );
            if let Some(axis) = axis_caption
                && (label.position[0] - axis[0]).abs() < 200.0
                && (label.position[1] - axis[1]).abs() < 24.0
            {
                let y = if axis[1] + 45.0 <= display[1] {
                    axis[1] + 24.0
                } else {
                    axis[1] - 24.0
                };
                label.position = bounded_label([label.position[0], y], [0.0, 21.0], display);
            }
        } else if label.text.starts_with("Tilt:") {
            label.position = bounded_label(
                [label.position[0] + 8.0, label.position[1] + 24.0],
                [0.0; 2],
                display,
            );
        } else {
            label.position = bounded_label([12.0, 36.0], [0.0; 2], display);
        }
    }
    out
}

fn annotations_in_pixels(
    camera: OrbitCamera,
    scene: &OrbitFrame,
    trail: Option<&OrbitTrail>,
    display: [f32; 2],
    show_orbit: bool,
    show_axis: bool,
) -> OrbitAnnotations {
    let mut out = OrbitAnnotations::default();
    if !display.into_iter().all(|v| v.is_finite() && v > 0.0) {
        return out;
    }
    let Some(frame) = camera.frame(scene) else {
        return out;
    };
    let (width, height) = (f64::from(display[0]), f64::from(display[1]));
    if show_orbit && let Some(trail) = trail {
        for pair in trail.points.windows(2) {
            let epoch = scene.instant.tt_seconds_j2000();
            let start = pair[0].instant.tt_seconds_j2000();
            let end = pair[1].instant.tt_seconds_j2000();
            let a = pair[0].earth_position_m;
            let b = pair[1].earth_position_m;
            if epoch > start && epoch < end {
                for (a, b) in [(a, scene.earth_position_m), (scene.earth_position_m, b)] {
                    if let Some(s) = stroke(frame, a, b, width, height) {
                        out.trail.push(s);
                    }
                }
            } else if let Some(s) = stroke(frame, a, b, width, height) {
                out.trail.push(s);
            }
        }
    }
    let focal = height / (2.0 * (30.0_f64.to_radians()).tan());
    for body in [OrbitBody::Earth, OrbitBody::Sun, OrbitBody::Moon] {
        if let Some(position) = projected(frame, body.center(scene), width, height) {
            let distance = body.center(scene).distance(frame.origin);
            out.bodies.push(OrbitLabel {
                position,
                label_position: bounded_label(
                    [position[0] + 8.0, position[1] - 18.0],
                    [0.0; 2],
                    display,
                ),
                body,
                small_body: body.radius() * focal / distance < 4.0,
            });
        }
    }
    if show_axis {
        let depth = (scene.earth_position_m - frame.origin).dot(frame.forward);
        if depth > 0.0 {
            let radius = (WGS84_A_M * 1.5).max(depth * 50.0 / focal);
            out.axis_exaggerated = radius > WGS84_A_M * 1.500001;
            let center = scene.earth_position_m;
            for i in 0..96 {
                let point = |i: usize| {
                    let t = i as f64 * std::f64::consts::TAU / 96.0;
                    center
                        + (scene.orbital_radial_icrf * t.cos()
                            + scene.orbital_tangent_icrf * t.sin())
                            * radius
                };
                if let Some(s) = stroke(frame, point(i), point(i + 1), width, height) {
                    out.plane.push(s);
                }
            }
            if let Some(s) = stroke(
                frame,
                center - scene.axis_icrf * radius * 1.3,
                center + scene.axis_icrf * radius * 1.3,
                width,
                height,
            ) {
                out.axis.push(s);
            }
            if let Some(s) = stroke(
                frame,
                center,
                center + scene.orbital_normal_icrf * radius * 1.3,
                width,
                height,
            ) {
                out.normal.push(s);
            }
            // Label the visible positive endpoint, including a clipped tip;
            // falling back to Earth's center would cover its own body label.
            if let Some(line) = out.axis.first() {
                out.labels.push(OrbitText {
                    position: line.b,
                    anchor: line.b,
                    connect: true,
                    text: "Earth axis: CIP".into(),
                });
            }
            if let Some(line) = out.normal.first() {
                out.labels.push(OrbitText {
                    position: line.b,
                    anchor: line.b,
                    connect: true,
                    text: "Orbital-plane normal".into(),
                });
            }
            if let Some(p) = projected(frame, center, width, height) {
                out.labels.push(OrbitText {
                    position: p,
                    anchor: p,
                    connect: true,
                    text: format!(
                        "Tilt: {:.3} deg to normal / {:.3} deg to plane",
                        scene.axial_tilt_deg,
                        (90.0 - scene.axial_tilt_deg).abs()
                    ),
                });
            }
            out.labels.push(OrbitText {
                position: bounded_label([12.0, 36.0], [0.0; 2], display),
                anchor: [12.0, 36.0],
                connect: false,
                text: if out.axis_exaggerated {
                    "CIP axis / instantaneous orbital plane: enlarged direction annotations"
                } else {
                    "CIP axis / instantaneous orbital plane: direction annotations"
                }
                .into(),
            });
        }
    }
    out
}
