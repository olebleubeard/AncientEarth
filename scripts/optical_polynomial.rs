//! Original quartic-polynomial helpers for unused optical diagnostics.
const BINOMIAL: [[f64; 5]; 5] = [
    [1.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 0.0, 0.0, 0.0],
    [1.0, 2.0, 1.0, 0.0, 0.0],
    [1.0, 3.0, 3.0, 1.0, 0.0],
    [1.0, 4.0, 6.0, 4.0, 1.0],
];

/// Coefficients in powers of t to degree-four Bernstein controls on [0,1].
pub fn power_to_bernstein(power: [f64; 5]) -> [f64; 5] {
    std::array::from_fn(|i| {
        (0..=i)
            .map(|k| power[k] * BINOMIAL[i][k] / BINOMIAL[4][k])
            .sum()
    })
}

/// Substitute t=offset+scale*s in a degree-four power polynomial.
pub fn restrict_power(power: [f64; 5], offset: f64, scale: f64) -> [f64; 5] {
    std::array::from_fn(|k| {
        (k..5)
            .map(|j| power[j] * BINOMIAL[j][k] * offset.powi((j - k) as i32) * scale.powi(k as i32))
            .sum()
    })
}
