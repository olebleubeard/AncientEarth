use super::{ACCENT, Explorer};
use ancient_earth::{orbit::OrbitOverview, simulation::AstronomicalScene};
use dear_imgui_rs::{StyleColor, TreeNodeFlags, Ui};

pub(super) fn controls(ui: &Ui, overview: &mut OrbitOverview) {
    if !ui.collapsing_header("ORBITS & AXIS GUIDES", TreeNodeFlags::empty()) {
        return;
    }
    ui.checkbox("Earth orbital path", &mut overview.show_orbit);
    ui.checkbox("Earth axis / tilt / orbital plane", &mut overview.show_axis);
    ui.checkbox(
        "Moon axis / tilt / orbital plane",
        &mut overview.show_moon_axis,
    );
    ui.text_wrapped("Earth guides appear as you fly far enough to see the globe. Moon guides also work in telescope view. Straight-ray guides are hidden during weather refraction.");
    if let Some(frame) = &overview.frame {
        for row in frame.information() {
            ui.separator();
            ui.text_wrapped(row.label);
            let _color = ui.push_style_color(StyleColor::Text, ACCENT);
            ui.text_wrapped(row.value);
        }
    }
    if overview.visible
        && let Some(trail) = &overview.trail
    {
        ui.text_wrapped(trail.description());
    }
    if let Some(error) = &overview.error {
        ui.text_wrapped(error);
    }
}

pub(super) fn draw_annotations(
    ui: &Ui,
    state: &Explorer,
    overview: &OrbitOverview,
    astronomy: &AstronomicalScene,
    scene: &earth_math::TerrainScene,
    geometric: bool,
) {
    if !geometric {
        return;
    }
    let Some(astronomy_frame) = astronomy.frame.as_ref() else {
        return;
    };
    use earth_math::rendering::orbit::{OrbitAnnotations, OrbitCaption, caption_layout};
    let mut annotation = OrbitAnnotations::default();
    if overview.visible
        && let Some(frame) = &overview.frame
    {
        annotation = earth_math::rendering::continuous::continuous_annotations(
            state.observer,
            state.lens(),
            frame,
            astronomy_frame,
            overview.trail.as_ref(),
            ui.io().display_size(),
            ui.io().display_framebuffer_scale(),
            overview.show_orbit,
            overview.show_axis,
        );
    }
    if overview.show_moon_axis {
        let moon = earth_math::rendering::lunar::annotations(
            state.observer,
            state.lens(),
            astronomy_frame,
            scene,
            ui.io().display_size(),
            ui.io().display_framebuffer_scale(),
        );
        annotation.axis.extend(moon.axis);
        annotation.plane.extend(moon.plane);
        annotation.normal.extend(moon.normal);
        annotation.labels.extend(moon.labels);
    }
    let draw = ui.get_background_draw_list();
    for (strokes, color) in [
        (&annotation.trail, [0.35, 0.82, 1.0, 0.9]),
        (&annotation.plane, [0.35, 0.82, 1.0, 0.45]),
        (&annotation.axis, [1.0, 0.74, 0.3, 1.0]),
        (&annotation.normal, [0.6, 0.95, 0.8, 0.8]),
    ] {
        for line in strokes {
            draw.add_line(line.a, line.b, color).thickness(1.0).build();
        }
    }
    let _font = ui.push_font_with_size(None, 13.0);
    let mut captions = Vec::new();
    for label in &annotation.bodies {
        if label.small_body {
            draw.add_circle(label.position, 4.0, [0.9, 0.95, 1.0, 1.0])
                .build();
        }
        let text = format!(
            "{}{}",
            label.body.name(),
            if label.small_body { " (locator)" } else { "" }
        );
        captions.push((
            OrbitCaption {
                anchor: label.position,
                preferred: label.label_position,
                text_size: ui.calc_text_size(&text),
                connect: true,
            },
            [0.9, 0.95, 1.0, 1.0],
            text,
        ));
    }
    for label in &annotation.labels {
        captions.push((
            OrbitCaption {
                anchor: label.anchor,
                preferred: label.position,
                text_size: ui.calc_text_size(&label.text),
                connect: label.connect,
            },
            [1.0, 0.85, 0.56, 1.0],
            label.text.clone(),
        ));
    }
    let requests: Vec<_> = captions.iter().map(|(request, _, _)| *request).collect();
    let placements = caption_layout(&requests, ui.io().display_size());
    for ((_, color, _), placement) in captions.iter().zip(&placements) {
        if let Some(line) = placement.as_ref().and_then(|p| p.leader) {
            draw.add_line(line.a, line.b, *color).thickness(1.0).build();
        }
    }
    for ((_, color, text), placement) in captions.iter().zip(&placements) {
        if let Some(placement) = placement {
            super::guide_caption(ui, &draw, placement.text_position, *color, text);
        }
    }
}
