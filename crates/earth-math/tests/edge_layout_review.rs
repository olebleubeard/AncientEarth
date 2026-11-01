//! Independent bounds, disjointness and pixel-coordinate checks for UI layout.
use earth_math::ScreenRect;
use earth_math::rendering::layout::{centered_info_panel, edge_panel_layout, rect_size};

fn overlaps(a: ScreenRect, b: ScreenRect) -> bool {
    (0..2).all(|i| a.min[i] < b.max[i] && b.min[i] < a.max[i])
}

fn checked_rect(rect: ScreenRect, display: [f32; 2], scale: [f32; 2]) {
    for axis in 0..2 {
        assert!(rect.min[axis].is_finite() && rect.max[axis].is_finite());
        assert!(rect.min[axis] >= 0.0 && rect.max[axis] <= display[axis]);
        assert!(
            rect.max[axis] > rect.min[axis],
            "collapsed {rect:?}, {display:?}, {scale:?}"
        );
        for coordinate in [rect.min[axis], rect.max[axis]] {
            let physical = f64::from(coordinate) * f64::from(scale[axis]);
            let roundoff = 2.0 * f64::from(f32::EPSILON) * physical.abs().max(1.0);
            assert!((physical - physical.round()).abs() <= roundoff);
        }
    }
    assert!(rect_size(rect).into_iter().all(|x| x > 0.0));
}

#[test]
fn all_panel_states_keep_handles_reachable_and_rectangles_disjoint() {
    let displays = [
        [3.0, 1.0],
        [4.0, 2.0],
        [20.0, 12.0],
        [80.0, 60.0],
        [320.0, 180.0],
        [320.0, 1080.0],
        [800.0, 250.0],
        [640.0, 480.0],
        [1920.0, 1080.0],
        [3440.0, 1440.0],
        [1001.25, 700.75],
    ];
    for display in displays {
        for scale in [
            [1.0, 1.0],
            [1.5, 1.5],
            [2.0, 2.0],
            [1.25, 1.75],
            [0.75, 2.0],
        ] {
            if f64::from(display[0]) * f64::from(scale[0]) < 3.0 {
                continue;
            }
            let closed = edge_panel_layout(display, scale, [false; 3]).unwrap();
            for bits in 0..8 {
                let expanded = std::array::from_fn(|i| bits & (1 << i) != 0);
                let layout = edge_panel_layout(display, scale, expanded).unwrap();
                let placements = [layout.left, layout.right, layout.bottom];
                let controls = [
                    closed.left.handle,
                    closed.right.handle,
                    closed.bottom.handle,
                ];
                let mut rectangles = Vec::new();
                for (i, placement) in placements.into_iter().enumerate() {
                    if placement.panel.is_none() {
                        assert_eq!(placement.handle, controls[i]);
                    }
                    checked_rect(placement.handle, display, scale);
                    rectangles.push(placement.handle);
                    if let Some(panel) = placement.panel {
                        assert!(expanded[i]);
                        checked_rect(panel, display, scale);
                        rectangles.push(panel);
                    }
                }
                for (i, a) in rectangles.iter().enumerate() {
                    for b in &rectangles[i + 1..] {
                        assert!(!overlaps(*a, *b));
                    }
                }
            }
        }
    }
    let tiny = edge_panel_layout([3.0, 1.0], [1.0; 2], [true; 3]).unwrap();
    assert_eq!(
        tiny.left.handle,
        ScreenRect {
            min: [0.0, 0.0],
            max: [1.0, 1.0]
        }
    );
    assert_eq!(
        tiny.bottom.handle,
        ScreenRect {
            min: [1.0, 0.0],
            max: [2.0, 1.0]
        }
    );
    assert_eq!(
        tiny.right.handle,
        ScreenRect {
            min: [2.0, 0.0],
            max: [3.0, 1.0]
        }
    );
    assert!(
        [tiny.left, tiny.right, tiny.bottom]
            .iter()
            .all(|p| p.panel.is_none())
    );
}

#[test]
fn normal_layout_has_expected_pixel_lanes_and_auxiliary_window_stays_centered() {
    let layout = edge_panel_layout([1920.0, 1080.0], [1.0; 2], [true; 3]).unwrap();
    assert_eq!(
        layout.left.handle,
        ScreenRect {
            min: [404.0, 428.0],
            max: [420.0, 464.0]
        }
    );
    assert_eq!(
        layout.right.handle,
        ScreenRect {
            min: [1460.0, 428.0],
            max: [1476.0, 464.0]
        }
    );
    assert_eq!(
        layout.bottom.handle,
        ScreenRect {
            min: [942.0, 852.0],
            max: [978.0, 868.0]
        }
    );
    assert_eq!(
        layout.left.panel.unwrap(),
        ScreenRect {
            min: [24.0, 48.0],
            max: [404.0, 844.0]
        }
    );
    assert_eq!(
        layout.right.panel.unwrap(),
        ScreenRect {
            min: [1476.0, 48.0],
            max: [1896.0, 844.0]
        }
    );
    assert_eq!(
        layout.bottom.panel.unwrap(),
        ScreenRect {
            min: [435.0, 868.0],
            max: [1485.0, 1056.0]
        }
    );
    for display in [
        [3.0, 1.0],
        [320.0, 180.0],
        [1001.25, 700.75],
        [1920.0, 1080.0],
    ] {
        for scale in [[1.0, 1.0], [1.5, 1.25], [2.0, 2.0]] {
            for desired in [[1.0, 1.0], [800.0, 680.0], [10000.0, 10000.0]] {
                let rect = centered_info_panel(display, scale, desired).unwrap();
                checked_rect(rect, display, scale);
                for axis in 0..2 {
                    let size = (f64::from(display[axis]) * f64::from(scale[axis])).floor();
                    let center_sum =
                        f64::from(rect.min[axis] + rect.max[axis]) * f64::from(scale[axis]);
                    assert!((center_sum - size).abs() <= 1.001);
                }
            }
        }
    }
}

#[test]
fn invalid_viewports_fail_closed_and_extreme_accepted_rectangles_remain_representable() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
        for axis in 0..2 {
            let mut display = [1920.0, 1080.0];
            display[axis] = bad;
            assert!(edge_panel_layout(display, [1.0; 2], [true; 3]).is_none());
            let mut scale = [1.0; 2];
            scale[axis] = bad;
            assert!(edge_panel_layout([1920.0, 1080.0], scale, [true; 3]).is_none());
            let mut desired = [800.0, 680.0];
            desired[axis] = bad;
            assert!(centered_info_panel([1920.0, 1080.0], [1.0; 2], desired).is_none());
        }
    }
    for display in [
        [2.999, 1080.0],
        [1920.0, 0.999],
        [4_194_305.0, 1080.0],
        [1920.0, 4_194_305.0],
    ] {
        assert!(edge_panel_layout(display, [1.0; 2], [true; 3]).is_none());
    }
    for scale_value in [0.0001, 0.1, 0.7, 1.0, 1.5, 3.0, 10000.0] {
        let scale = [scale_value; 2];
        for physical in [4096.0_f32, 1_048_576.0, 4_194_302.0] {
            let display = [physical / scale_value; 2];
            if let Some(layout) = edge_panel_layout(display, scale, [true; 3]) {
                for p in [layout.left, layout.right, layout.bottom] {
                    checked_rect(p.handle, display, scale);
                    if let Some(rect) = p.panel {
                        checked_rect(rect, display, scale);
                    }
                }
            }
        }
    }
}
