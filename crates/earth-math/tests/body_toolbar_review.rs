//! Independent control placement/physical-pixel icon checks, not optics.
use earth_math::{ScreenRect, astro::aim::ViewBody, rendering::layout::*};

fn overlaps(a: ScreenRect, b: ScreenRect) -> bool {
    (0..2).all(|i| a.min[i] < b.max[i] && b.min[i] < a.max[i])
}
fn contains(outer: ScreenRect, inner: ScreenRect) -> bool {
    (0..2).all(|i| inner.min[i] >= outer.min[i] && inner.max[i] <= outer.max[i])
}
fn physical_rounding(rect: ScreenRect, scale: [f32; 2]) {
    for point in [rect.min, rect.max] {
        for i in 0..2 {
            let p = f64::from(point[i]) * f64::from(scale[i]);
            assert!((p - p.round()).abs() <= 2.0 * f64::from(f32::EPSILON) * p.abs().max(1.0));
        }
    }
}

#[test]
fn expanded_handles_attach_to_panel_edges_and_toolbar_never_covers_controls() {
    for display in [
        [3.0, 1.0],
        [80.0, 60.0],
        [320.0, 180.0],
        [320.0, 1080.0],
        [640.0, 480.0],
        [1920.0, 1080.0],
        [3440.0, 1440.0],
        [1001.25, 700.75],
    ] {
        for scale in [[1.0; 2], [1.5; 2], [1.25, 1.75], [0.75, 2.0], [2.0; 2]] {
            let Some(closed) = edge_panel_layout(display, scale, [false; 3]) else {
                continue;
            };
            for bits in 0..8 {
                let open = std::array::from_fn(|i| bits & (1 << i) != 0);
                let layout = edge_panel_layout(display, scale, open).unwrap();
                let items = [layout.left, layout.right, layout.bottom];
                for (i, item) in items.iter().enumerate() {
                    physical_rounding(item.handle, scale);
                    if let Some(panel) = item.panel {
                        match i {
                            0 => assert_eq!(item.handle.min[0], panel.max[0]),
                            1 => assert_eq!(item.handle.max[0], panel.min[0]),
                            _ => assert_eq!(item.handle.max[1], panel.min[1]),
                        }
                        let transverse = if i == 2 { 0 } else { 1 };
                        assert!(item.handle.min[transverse] >= panel.min[transverse]);
                        assert!(item.handle.max[transverse] <= panel.max[transverse]);
                        assert!(!overlaps(item.handle, panel));
                    } else {
                        assert_eq!(
                            item.handle,
                            [closed.left, closed.right, closed.bottom][i].handle
                        );
                    }
                }
                if let Some(toolbar) = body_toolbar(display, scale, Some(&layout)) {
                    assert!(contains(
                        ScreenRect {
                            min: [0.0; 2],
                            max: display
                        },
                        toolbar
                    ));
                    physical_rounding(toolbar, scale);
                    for item in items {
                        for other in std::iter::once(item.handle).chain(item.panel) {
                            assert!(!overlaps(toolbar, other));
                        }
                    }
                    let buttons = body_toolbar_buttons(toolbar, scale).unwrap();
                    assert_eq!(buttons.len(), 4);
                    for (i, button) in buttons.iter().enumerate() {
                        assert!(contains(toolbar, *button));
                        physical_rounding(*button, scale);
                        assert!(button.max[0] > button.min[0] && button.max[1] > button.min[1]);
                        for other in &buttons[i + 1..] {
                            assert!(!overlaps(*button, *other));
                        }
                    }
                    let widths = buttons.map(|b| (b.max[0] - b.min[0]) * scale[0]);
                    assert!(
                        widths.into_iter().fold(f32::NEG_INFINITY, f32::max)
                            - widths.into_iter().fold(f32::INFINITY, f32::min)
                            < 1.001
                    );
                }
            }
        }
    }
    let layout = edge_panel_layout([1920.0, 1080.0], [1.0; 2], [true; 3]).unwrap();
    let toolbar = body_toolbar([1920.0, 1080.0], [1.0; 2], Some(&layout)).unwrap();
    assert_eq!(
        toolbar,
        ScreenRect {
            min: [872.0, 6.0],
            max: [1048.0, 40.0]
        }
    );
    // Independently supplied occupied region must also suppress the toolbar.
    let mut obstruction = layout;
    obstruction.left.panel = Some(toolbar);
    assert!(body_toolbar([1920.0, 1080.0], [1.0; 2], Some(&obstruction)).is_none());
}

#[test]
fn icon_strokes_fit_buttons_and_physical_circles_survive_anisotropic_scaling() {
    for scale in [[1.0; 2], [1.5; 2], [1.25, 2.0], [3.0, 0.75]] {
        let bar = body_toolbar([1920.0, 1080.0], scale, None).unwrap();
        let buttons = body_toolbar_buttons(bar, scale).unwrap();
        for (body, button) in [ViewBody::Earth, ViewBody::Moon, ViewBody::Sun]
            .into_iter()
            .zip(buttons)
        {
            let strokes = body_icon(body, button, scale).unwrap();
            let c = [
                f64::from(button.min[0] + button.max[0]) * f64::from(scale[0]) * 0.5,
                f64::from(button.min[1] + button.max[1]) * f64::from(scale[1]) * 0.5,
            ];
            for stroke in &strokes {
                for p in stroke {
                    assert!(p.iter().all(|x| x.is_finite()));
                    assert!((0..2).all(|i| p[i] >= button.min[i] && p[i] <= button.max[i]));
                }
            }
            if body != ViewBody::Moon {
                let radii: Vec<_> = strokes[0]
                    .iter()
                    .map(|p| {
                        (f64::from(p[0]) * f64::from(scale[0]) - c[0])
                            .hypot(f64::from(p[1]) * f64::from(scale[1]) - c[1])
                    })
                    .collect();
                let min = radii.iter().copied().fold(f64::INFINITY, f64::min);
                let max = radii.iter().copied().fold(0.0, f64::max);
                assert!(max - min < 0.002, "{body:?} {scale:?} {min}..{max}");
            }
        }
        let button = buttons[3];
        let strokes = horizon_lock_icon(button, scale).unwrap();
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[1].len(), 2);
        let center: [f64; 2] = std::array::from_fn(|i| {
            (f64::from(button.min[i]) + f64::from(button.max[i])) * f64::from(scale[i]) * 0.5
        });
        let mut radii = Vec::new();
        for (index, stroke) in strokes.iter().enumerate() {
            for point in stroke {
                assert!((0..2).all(|i| point[i].is_finite()
                    && point[i] >= button.min[i]
                    && point[i] <= button.max[i]));
                let dx = f64::from(point[0]) * f64::from(scale[0]) - center[0];
                let dy = f64::from(point[1]) * f64::from(scale[1]) - center[1];
                if index == 0 {
                    radii.push(dx.hypot(dy));
                } else {
                    assert!(dy.abs() < 0.002);
                }
            }
        }
        let min = radii.iter().copied().fold(f64::INFINITY, f64::min);
        let max = radii.iter().copied().fold(0.0, f64::max);
        assert!(max - min < 0.002);
        assert!(strokes[1][0][0] < strokes[1][1][0]);
    }
    for bad in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
        assert!(body_toolbar([1920.0, 1080.0], [bad, 1.0], None).is_none());
        let rect = ScreenRect {
            min: [0.0; 2],
            max: [132.0, 34.0],
        };
        assert!(body_toolbar_buttons(rect, [1.0, bad]).is_none());
        assert!(body_icon(ViewBody::Earth, rect, [bad, 1.0]).is_none());
        assert!(horizon_lock_icon(rect, [bad, 1.0]).is_none());
    }
    assert!(
        body_icon(
            ViewBody::Moon,
            ScreenRect {
                min: [0.0; 2],
                max: [1.0; 2]
            },
            [1.0; 2]
        )
        .is_none()
    );
}
