//! Screen layout only; these rectangles have no optical or geodetic meaning.
//!
//! Allocate in whole framebuffer pixels, then return logical ImGui coordinates.
//! Content can disappear below its usable size, but independently restorable
//! handles retain disjoint hit boxes. At microscopic viewport sizes a bounded
//! hit box does not imply that a complete text arrow can remain legible.

use crate::ScreenRect;

// Headroom below the f32 integer limit keeps adjacent physical-pixel endpoints
// distinct after division by a finite (possibly anisotropic) scale and rounding
// back to logical f32 coordinates.
const MAX_PHYSICAL_EXTENT: f64 = 4_194_304.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgePanelPlacement {
    pub handle: ScreenRect,
    pub panel: Option<ScreenRect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgePanelLayout {
    pub left: EdgePanelPlacement,
    pub right: EdgePanelPlacement,
    pub bottom: EdgePanelPlacement,
}

/// Size of an already ordered logical screen rectangle.
pub fn rect_size(rect: ScreenRect) -> [f32; 2] {
    [rect.max[0] - rect.min[0], rect.max[1] - rect.min[1]]
}

#[derive(Clone, Copy)]
struct Viewport {
    width: u32,
    height: u32,
    scale: [f64; 2],
}

impl Viewport {
    fn new(display: [f32; 2], scale: [f32; 2]) -> Option<Self> {
        if !display
            .into_iter()
            .chain(scale)
            .all(|x| x.is_finite() && x > 0.0)
        {
            return None;
        }
        let dimensions = std::array::from_fn::<_, 2, _>(|axis| {
            (f64::from(display[axis]) * f64::from(scale[axis])).floor()
        });
        // Three distinct physical columns are needed for the three handles.
        if !(3.0..=MAX_PHYSICAL_EXTENT).contains(&dimensions[0])
            || !(1.0..=MAX_PHYSICAL_EXTENT).contains(&dimensions[1])
        {
            return None;
        }
        Some(Self {
            width: dimensions[0] as u32,
            height: dimensions[1] as u32,
            scale: scale.map(f64::from),
        })
    }

    fn pixels(self, logical: f64, axis: usize) -> u32 {
        (logical * self.scale[axis])
            .round()
            .clamp(1.0, MAX_PHYSICAL_EXTENT) as u32
    }

    fn rect(self, x: u32, y: u32, width: u32, height: u32) -> ScreenRect {
        ScreenRect {
            min: [
                (f64::from(x) / self.scale[0]) as f32,
                (f64::from(y) / self.scale[1]) as f32,
            ],
            max: [
                (f64::from(x + width) / self.scale[0]) as f32,
                (f64::from(y + height) / self.scale[1]) as f32,
            ],
        }
    }
}

/// Compact handles attach just outside the inner edge of a visible panel.
/// When content is collapsed or too small to show, its handle stays at the
/// corresponding screen edge. All rectangles are disjoint in physical pixels.
/// A top row is reserved for the body toolbar. Desired content sizes are
/// 380x820, 420x860 and 1050x188 logical units, with minimum usable sizes
/// 160x120 for sides and 240x100 for the bottom.
pub fn edge_panel_layout(
    display: [f32; 2],
    framebuffer_scale: [f32; 2],
    expanded: [bool; 3],
) -> Option<EdgePanelLayout> {
    let view = Viewport::new(display, framebuffer_scale)?;
    let side_width = view.pixels(16.0, 0).min(view.width / 3);
    let side_height = view.pixels(36.0, 1).min(view.height);
    let bottom_width = view.pixels(36.0, 0).min(view.width - 2 * side_width);
    let bottom_height = view.pixels(16.0, 1).min(view.height);
    let side_y = (view.height - side_height) / 2;
    let mut layout = EdgePanelLayout {
        left: EdgePanelPlacement {
            handle: view.rect(0, side_y, side_width, side_height),
            panel: None,
        },
        right: EdgePanelPlacement {
            handle: view.rect(view.width - side_width, side_y, side_width, side_height),
            panel: None,
        },
        bottom: EdgePanelPlacement {
            handle: view.rect(
                (view.width - bottom_width) / 2,
                view.height - bottom_height,
                bottom_width,
                bottom_height,
            ),
            panel: None,
        },
    };
    let gap_x = view.pixels(8.0, 0);
    let gap_y = view.pixels(8.0, 1);
    let left = side_width.saturating_add(gap_x).min(view.width);
    let right = view.width.saturating_sub(side_width + gap_x);
    let width = right.saturating_sub(left);
    let top = view.pixels(48.0, 1).min(view.height);
    let bottom = view.height.saturating_sub(bottom_height + gap_y);
    let mut side_bottom = bottom;
    if expanded[2] {
        let w = width.min(view.pixels(1050.0, 0));
        let h = bottom
            .saturating_sub(top + bottom_height)
            .min(view.pixels(188.0, 1));
        if w >= view.pixels(240.0, 0) && h >= view.pixels(100.0, 1) {
            let y = bottom - h;
            layout.bottom.panel = Some(view.rect((view.width - w) / 2, y, w, h));
            layout.bottom.handle = view.rect(
                (view.width - bottom_width) / 2,
                y - bottom_height,
                bottom_width,
                bottom_height,
            );
            side_bottom = y.saturating_sub(bottom_height + gap_y);
        }
    }
    let desired = [view.pixels(380.0, 0), view.pixels(420.0, 0)];
    let mut widths = [0; 2];
    // Reserve both interior handle widths even if a candidate panel is omitted.
    let available = width.saturating_sub(2 * side_width + gap_x);
    match [expanded[0], expanded[1]] {
        [true, true] => {
            let total = desired[0] + desired[1];
            if total <= available {
                widths = desired;
            } else {
                widths[0] =
                    (u64::from(available) * u64::from(desired[0]) / u64::from(total)) as u32;
                widths[1] = available - widths[0];
            }
        }
        [true, false] => widths[0] = desired[0].min(available),
        [false, true] => widths[1] = desired[1].min(available),
        [false, false] => {}
    }
    for (index, placement) in [&mut layout.left, &mut layout.right]
        .into_iter()
        .enumerate()
    {
        let h = side_bottom
            .saturating_sub(top)
            .min(view.pixels(if index == 0 { 820.0 } else { 860.0 }, 1));
        let w = widths[index];
        if w >= view.pixels(160.0, 0) && h >= view.pixels(120.0, 1) {
            let x = if index == 0 { left } else { right - w };
            placement.panel = Some(view.rect(x, top, w, h));
            let handle_x = if index == 0 { x + w } else { x - side_width };
            placement.handle = view.rect(
                handle_x,
                top + (h - side_height) / 2,
                side_width,
                side_height,
            );
        }
    }
    Some(layout)
}

/// Top-center toolbar. Omit it if four usable buttons cannot fit or if any
/// supplied panel/handle intersects it; never cover another interaction target.
pub fn body_toolbar(
    display: [f32; 2],
    framebuffer_scale: [f32; 2],
    layout: Option<&EdgePanelLayout>,
) -> Option<ScreenRect> {
    let view = Viewport::new(display, framebuffer_scale)?;
    let w = view.pixels(176.0, 0);
    let h = view.pixels(34.0, 1);
    let y = view.pixels(6.0, 1);
    if w > view.width || y + h > view.height {
        return None;
    }
    let rect = view.rect((view.width - w) / 2, y, w, h);
    if let Some(layout) = layout {
        for item in [layout.left, layout.right, layout.bottom] {
            for other in std::iter::once(item.handle).chain(item.panel) {
                if (0..2).all(|i| rect.min[i] < other.max[i] && other.min[i] < rect.max[i]) {
                    return None;
                }
            }
        }
    }
    Some(rect)
}

/// Divide the supplied toolbar into four disjoint button rectangles, keeping
/// physical-pixel edges and a four-logical-unit nominal gap.
pub fn body_toolbar_buttons(
    rect: ScreenRect,
    framebuffer_scale: [f32; 2],
) -> Option<[ScreenRect; 4]> {
    if !rect
        .min
        .into_iter()
        .chain(rect.max)
        .chain(framebuffer_scale)
        .all(f32::is_finite)
        || !framebuffer_scale.into_iter().all(|s| s > 0.0)
        || !(0..2).all(|i| rect.min[i] >= 0.0 && rect.max[i] > rect.min[i])
    {
        return None;
    }
    let view = Viewport::new(rect.max, framebuffer_scale)?;
    let x = (f64::from(rect.min[0]) * view.scale[0]).round() as u32;
    let y = (f64::from(rect.min[1]) * view.scale[1]).round() as u32;
    let end_x = (f64::from(rect.max[0]) * view.scale[0]).round() as u32;
    let end_y = (f64::from(rect.max[1]) * view.scale[1]).round() as u32;
    let width = end_x.checked_sub(x)?;
    let height = end_y.checked_sub(y)?;
    if width < 4 || height == 0 {
        return None;
    }
    let gap = view.pixels(4.0, 0).min((width - 4) / 3);
    let content = width - 3 * gap;
    Some(std::array::from_fn(|i| {
        let start = (u64::from(content) * i as u64 / 4) as u32;
        let end = (u64::from(content) * (i + 1) as u64 / 4) as u32;
        view.rect(x + start + i as u32 * gap, y, end - start, height)
    }))
}

/// Original decorative outline icons, not maps or an instantaneous lunar phase.
/// Stroke points are logical coordinates; circles are circular in physical pixels.
pub fn body_icon(
    body: crate::astro::aim::ViewBody,
    button: ScreenRect,
    framebuffer_scale: [f32; 2],
) -> Option<Vec<Vec<[f32; 2]>>> {
    if !button
        .min
        .into_iter()
        .chain(button.max)
        .chain(framebuffer_scale)
        .all(f32::is_finite)
        || !framebuffer_scale.into_iter().all(|s| s > 0.0)
        || !(0..2).all(|i| button.min[i] >= 0.0 && button.max[i] > button.min[i])
    {
        return None;
    }
    let scale = framebuffer_scale.map(f64::from);
    let lower: [f64; 2] = std::array::from_fn(|i| f64::from(button.min[i]) * scale[i]);
    let upper: [f64; 2] = std::array::from_fn(|i| f64::from(button.max[i]) * scale[i]);
    if upper.into_iter().any(|x| x > MAX_PHYSICAL_EXTENT) {
        return None;
    }
    let center: [f64; 2] = std::array::from_fn(|i| (lower[i] + upper[i]) * 0.5);
    let radius =
        (upper[0] - lower[0]).min(upper[1] - lower[1]) * 0.5 - 4.0 * scale[0].min(scale[1]);
    if radius < 2.0 {
        return None;
    }
    let point = |x: f64, y: f64| {
        [
            ((center[0] + radius * x) / scale[0]) as f32,
            ((center[1] + radius * y) / scale[1]) as f32,
        ]
    };
    let ellipse = |x: f64, y: f64| {
        (0..=48)
            .map(|i| {
                let angle = std::f64::consts::TAU * f64::from(i) / 48.0;
                point(x * angle.cos(), y * angle.sin())
            })
            .collect::<Vec<_>>()
    };
    use crate::astro::aim::ViewBody;
    Some(match body {
        ViewBody::Earth => vec![
            ellipse(0.92, 0.92),
            ellipse(0.42, 0.92),
            vec![point(-0.92, 0.0), point(0.92, 0.0)],
        ],
        ViewBody::Moon => {
            let mut outline = Vec::with_capacity(66);
            for i in 0..=32 {
                let angle =
                    -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / 32.0;
                outline.push(point(-angle.cos(), angle.sin()));
            }
            for i in (0..=32).rev() {
                let angle =
                    -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / 32.0;
                outline.push(point(-0.3 * angle.cos(), angle.sin()));
            }
            vec![outline]
        }
        ViewBody::Sun => {
            let mut strokes = vec![ellipse(0.55, 0.55)];
            for i in 0..8 {
                let angle = std::f64::consts::TAU * f64::from(i) / 8.0;
                strokes.push(vec![
                    point(0.73 * angle.cos(), 0.73 * angle.sin()),
                    point(angle.cos(), angle.sin()),
                ]);
            }
            strokes
        }
    })
}

/// Decorative circle/level-bar control symbol, not a measured horizon curve.
/// It shares the reviewed physical-pixel registration of the body icons.
pub fn horizon_lock_icon(
    button: ScreenRect,
    framebuffer_scale: [f32; 2],
) -> Option<Vec<Vec<[f32; 2]>>> {
    let mut strokes = body_icon(
        crate::astro::aim::ViewBody::Earth,
        button,
        framebuffer_scale,
    )?;
    strokes.remove(1); // Omit the Earth's meridian; keep circle and level bar.
    Some(strokes)
}

/// Centered auxiliary information window, with nominal 18-logical-unit margins
/// rounded to physical pixels and reduced when necessary. Minuscule views
/// retain a positive bounded rectangle; text legibility and scrolling are the
/// caller's responsibility.
pub fn centered_info_panel(
    display: [f32; 2],
    framebuffer_scale: [f32; 2],
    desired: [f32; 2],
) -> Option<ScreenRect> {
    let view = Viewport::new(display, framebuffer_scale)?;
    if !desired.into_iter().all(|x| x.is_finite() && x > 0.0) {
        return None;
    }
    let dimensions = [view.width, view.height];
    let size: [u32; 2] = std::array::from_fn(|axis| {
        let margin = view.pixels(18.0, axis).min((dimensions[axis] - 1) / 2);
        view.pixels(f64::from(desired[axis]), axis)
            .min(dimensions[axis] - 2 * margin)
    });
    Some(view.rect(
        (view.width - size[0]) / 2,
        (view.height - size[1]) / 2,
        size[0],
        size[1],
    ))
}
