//! Unused whole-cell scalar first/second-variation majorants.
//!
//! For a C2 scalar f on the normalized convex square [-1,1]^2, `value`
//! encloses f, `gradient` bounds ||Df||_2, and `hessian` bounds ||D2f||_op.
//! Bounds apply throughout the square, not only at its center/corners.
//! Product and unary chain rules need two positive scalars per node instead
//! of a full interval Jacobian. Value intervals retain their own dependency
//! overestimation; this foundation makes no cell-admission or cost claim.
//!
//! The linear Taylor remainder about the center is at most hessian on this
//! square, since ||xi||^2/2 <= 1. It is NOT permission to reset an incoming
//! remainder: a future stepper must propagate full-cell derivatives, center
//! and linear-coefficient roundoff, state uncertainty, and the numerical
//! geometric-baseline consistency term before publishing a correction.
//! `constant_interval` represents an angle-independent uncertain parameter;
//! `from_parts` requires already proved full-cell derivative bounds.
//!
//! All arithmetic is diagnostic CPU binary64 with gradual underflow, using
//! the previously reviewed outward Interval primitives. This is not a WGSL
//! precision/FTZ contract. Overflow or an unproved smooth domain declines.
use super::optical_taylor::Interval as I;
use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug)]
pub struct NormJet {
    pub value: I,
    pub gradient: f64,
    pub hessian: f64,
}

fn nonnegative(x: f64) -> bool {
    x.is_finite() && x >= 0.
}

/// Upward positive arithmetic. A known exact zero is preserved only after
/// validating both operands; zero must never erase an invalid bound.
fn up_add(a: f64, b: f64) -> f64 {
    if !nonnegative(a) || !nonnegative(b) {
        return f64::INFINITY;
    }
    if a == 0. { return b; }
    if b == 0. { return a; }
    (a + b).next_up()
}

fn up_mul(a: f64, b: f64) -> f64 {
    if !nonnegative(a) || !nonnegative(b) {
        return f64::INFINITY;
    }
    if a == 0. || b == 0. { return 0.; }
    (a * b).next_up()
}

fn upper_nonnegative(i: I) -> f64 {
    if i.valid() && i.hi >= 0. { i.hi } else { f64::INFINITY }
}

impl NormJet {
    pub fn from_parts(value: I, gradient: f64, hessian: f64) -> Self {
        let result = Self { value, gradient, hessian };
        if result.valid() { result } else { Self::invalid() }
    }

    pub fn point(value: f64) -> Self {
        Self::constant_interval(I::point(value))
    }

    pub fn constant_interval(value: I) -> Self {
        Self::from_parts(value, 0., 0.)
    }

    pub fn invalid() -> Self {
        Self { value: I::invalid(), gradient: f64::INFINITY, hessian: f64::INFINITY }
    }

    pub fn valid(self) -> bool {
        self.value.valid() && nonnegative(self.gradient) && nonnegative(self.hessian)
    }

    /// The exact real affine function with these encoded binary64 inputs.
    pub fn affine(center: f64, linear: [f64; 2]) -> Self {
        if !center.is_finite() || linear.iter().any(|x| !x.is_finite()) {
            return Self::invalid();
        }
        let radius = up_add(linear[0].abs(), linear[1].abs());
        let value = I::point(center) + I { lo: -radius, hi: radius };
        let sum = I::point(linear[0]) * I::point(linear[0])
            + I::point(linear[1]) * I::point(linear[1]);
        if !sum.valid() { return Self::invalid(); }
        // The exact sum of squares is nonnegative even when outward
        // underflow makes its lower endpoint slightly negative.
        let norm = I { lo: sum.lo.max(0.), hi: sum.hi }.sqrt();
        Self::from_parts(value, upper_nonnegative(norm), 0.)
    }

    fn unary(self, value: I, first: f64, second: f64) -> Self {
        if !self.valid() || !nonnegative(first) || !nonnegative(second) {
            return Self::invalid();
        }
        let gradient = up_mul(first, self.gradient);
        let hessian = up_add(up_mul(first, self.hessian),
            up_mul(second, up_mul(self.gradient, self.gradient)));
        Self::from_parts(value, gradient, hessian)
    }

    pub fn reciprocal(self) -> Self {
        if !self.valid() || self.value.abs_lower() <= 0. { return Self::invalid(); }
        let inverse = I::point(1.) / self.value;
        let m = I::point(self.value.abs_lower());
        let reciprocal_m = I::point(1.) / m;
        let first = reciprocal_m * reciprocal_m;
        let second = I::point(2.) * first * reciprocal_m;
        self.unary(inverse, upper_nonnegative(first), upper_nonnegative(second))
    }

    pub fn sqrt(self) -> Self {
        if !self.valid() { return Self::invalid(); }
        if self.value.lo == 0. && self.value.hi == 0.
            && self.gradient == 0. && self.hessian == 0. {
            return Self::point(0.);
        }
        if self.value.lo <= 0. { return Self::invalid(); }
        let lower = I::point(self.value.lo);
        let root_lower = lower.sqrt();
        let first = I::point(0.5) / root_lower;
        let second = I::point(0.25) / (lower * root_lower);
        self.unary(self.value.sqrt(), upper_nonnegative(first), upper_nonnegative(second))
    }

    pub fn inverse_sqrt(self) -> Self {
        if !self.valid() || self.value.lo <= 0. { return Self::invalid(); }
        let lower = I::point(self.value.lo);
        let root_lower = lower.sqrt();
        let first = I::point(0.5) / (lower * root_lower);
        let second = I::point(0.75) / (lower * lower * root_lower);
        self.unary(I::point(1.) / self.value.sqrt(),
            upper_nonnegative(first), upper_nonnegative(second))
    }

    /// Only the real Taylor remainder. Floating center/linear evaluation,
    /// incoming state error and output encoding require separate charges.
    pub fn square_remainder(self) -> f64 {
        if self.valid() { self.hessian } else { f64::INFINITY }
    }
}

impl Neg for NormJet {
    type Output = Self;
    fn neg(self) -> Self {
        if !self.valid() { return Self::invalid(); }
        Self { value: -self.value, ..self }
    }
}

impl Add for NormJet {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        if !self.valid() || !other.valid() { return Self::invalid(); }
        Self::from_parts(self.value + other.value,
            up_add(self.gradient, other.gradient), up_add(self.hessian, other.hessian))
    }
}

impl Sub for NormJet {
    type Output = Self;
    fn sub(self, other: Self) -> Self { self + -other }
}

impl Mul for NormJet {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        if !self.valid() || !other.valid() { return Self::invalid(); }
        let a = self.value.abs_upper();
        let b = other.value.abs_upper();
        let gradient = up_add(up_mul(a, other.gradient), up_mul(b, self.gradient));
        // D2(fg) = f D2g + g D2f + Df(Dg)^T + Dg(Df)^T.
        let hessian = up_add(up_add(up_mul(a, other.hessian), up_mul(b, self.hessian)),
            up_mul(2., up_mul(self.gradient, other.gradient)));
        Self::from_parts(self.value * other.value, gradient, hessian)
    }
}

impl Div for NormJet {
    type Output = Self;
    fn div(self, other: Self) -> Self { self * other.reciprocal() }
}

#[cfg(test)]
#[path = "optical_second_variation_review.rs"]
mod independent_review;
