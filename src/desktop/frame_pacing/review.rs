//! Synthetic-clock checks: no sleeping and no claims about OS scheduling.
use super::*;

#[test]
fn foreground_background_and_qa_limits_have_explicit_precedence() {
    for limit in FrameLimit::ALL {
        assert_eq!(effective_rate(limit, true, false, false), limit.fps());
        assert_eq!(effective_rate(limit, false, false, false), Some(15));
        assert_eq!(effective_rate(limit, true, true, false), Some(5));
        assert_eq!(effective_rate(limit, false, true, false), Some(5));
        for focused in [false, true] {
            for minimized in [false, true] {
                assert_eq!(effective_rate(limit, focused, minimized, true), None);
            }
        }
    }
    assert_eq!(FrameLimit::default(), FrameLimit::Fps60);
    for (text, expected) in [
        ("30", FrameLimit::Fps30),
        ("60", FrameLimit::Fps60),
        ("120", FrameLimit::Fps120),
        ("unlimited", FrameLimit::Unlimited),
    ] {
        assert_eq!(FrameLimit::parse(text), Some(expected));
    }
    for text in ["0", "59.9", "-1", "", "Unlimited"] {
        assert_eq!(FrameLimit::parse(text), None);
    }
}

#[test]
fn cadence_uses_actual_starts_without_catch_up_or_elapsed_time_clamping() {
    let base = Instant::now();
    let mut cadence = Cadence::default();
    assert_eq!(cadence.delay(base, Some(60)), Duration::ZERO);
    cadence.started(base);
    for (rate, nanoseconds) in [
        (5, 200_000_000),
        (15, 66_666_667),
        (30, 33_333_334),
        (60, 16_666_667),
        (120, 8_333_334),
    ] {
        let period = Duration::from_nanos(nanoseconds);
        assert_eq!(cadence.delay(base, Some(rate)), period);
        assert_eq!(
            cadence.delay(base + period - Duration::from_nanos(1), Some(rate)),
            Duration::from_nanos(1)
        );
        assert_eq!(cadence.delay(base + period, Some(rate)), Duration::ZERO);
        assert_eq!(
            cadence.delay(base + period + Duration::from_secs(2), Some(rate)),
            Duration::ZERO
        );
    }
    assert_eq!(cadence.delay(base, None), Duration::ZERO);
    assert_eq!(cadence.delay(base, Some(0)), Duration::ZERO);
    let late = base + Duration::from_secs(3);
    cadence.started(late);
    assert_eq!(
        cadence.delay(late, Some(60)),
        Duration::from_nanos(16_666_667)
    );
    // Switching rate is relative to the last actual start, not an old deadline.
    assert_eq!(
        cadence.delay(late + Duration::from_millis(10), Some(120)),
        Duration::ZERO
    );
    assert_eq!(
        cadence.delay(late + Duration::from_millis(10), Some(30)),
        Duration::from_nanos(23_333_334)
    );
    // A later wakeup is retained as the next anchor; it is never subtracted from
    // a future interval to manufacture a catch-up frame.
    let overslept = late + Duration::from_millis(40);
    cadence.started(overslept);
    assert_eq!(
        cadence.delay(overslept + Duration::from_millis(5), Some(60)),
        Duration::from_nanos(11_666_667)
    );
}
