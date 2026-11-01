//! Independent parity with the original complete-line edge filters.
use super::posts_in_interval;

#[test]
fn sorted_subranges_preserve_all_four_original_edge_sequences_bit_for_bit() {
    let mut posts = vec![
        -f64::MAX,
        -180.0,
        -90.0,
        -1.0,
        -f64::MIN_POSITIVE,
        -f64::from_bits(1),
        -0.0,
        0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        1.0,
        90.0,
        180.0,
        f64::MAX,
    ];
    // Duplicate source posts, exact source/cell coincidences and near endpoints
    // matter: numeric equality alone would miss a signed-zero reordering.
    for index in -48..=48 {
        let post = f64::from(index) / 8.0;
        posts.extend([post.next_down(), post, post, post.next_up()]);
    }
    posts.sort_by(f64::total_cmp);
    let mut bounds = vec![-f64::MAX, f64::MAX, -0.0, 0.0];
    for index in -12..=12 {
        let bound = f64::from(index) / 4.0;
        bounds.extend([bound.next_down(), bound, bound.next_up()]);
    }
    let bits = |values: Vec<f64>| values.into_iter().map(f64::to_bits).collect::<Vec<_>>();
    for source in [&[][..], &posts[..1], &posts[..]] {
        for &lower in &bounds {
            for &upper in &bounds {
                if lower > upper {
                    continue;
                }
                // South/east: original forward [lower, upper) traversal.
                let expected = source
                    .iter()
                    .copied()
                    .filter(|value| *value >= lower && *value < upper)
                    .collect();
                let actual = posts_in_interval(source, lower, upper, true).to_vec();
                assert_eq!(bits(actual), bits(expected), "forward {lower}..{upper}");
                // North/west: original reversed (lower, upper] traversal.
                let expected = source
                    .iter()
                    .rev()
                    .copied()
                    .filter(|value| *value > lower && *value <= upper)
                    .collect();
                let actual = posts_in_interval(source, lower, upper, false)
                    .iter()
                    .rev()
                    .copied()
                    .collect();
                assert_eq!(bits(actual), bits(expected), "reverse {lower}..{upper}");
            }
        }
    }
}
