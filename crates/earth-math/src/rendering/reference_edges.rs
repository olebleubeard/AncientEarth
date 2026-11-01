//! Range lookup on finite, ascending geographic posts. This avoids scanning
//! an entire latitude/longitude line for every small adaptive cell.

/// Preserve the original edge predicates exactly: [lower, upper) for bottom/
/// right edges, (lower, upper] for top/left edges (which callers then reverse).
/// Inputs come from the sorted finite cell/perimeter coordinate registry.
pub(super) fn posts_in_interval(
    posts: &[f64],
    lower: f64,
    upper: f64,
    include_lower: bool,
) -> &[f64] {
    let (start, end) = if include_lower {
        (
            posts.partition_point(|value| *value < lower),
            posts.partition_point(|value| *value < upper),
        )
    } else {
        (
            posts.partition_point(|value| *value <= lower),
            posts.partition_point(|value| *value <= upper),
        )
    };
    &posts[start..end]
}

#[cfg(test)]
mod review;
