// Mathematical built-in functions.
//
// SPEC §13.1:
//   abs value
//   min value, value, ...
//   max value, value, ...
//   clamp value, min, max
//   map value, from_low, from_high, to_low, to_high
//
// DESIGN
//   All functions are generic over a Numeric trait bound so that
//   abs/min/max/clamp work on Integer (i32), Decimal (f32),
//   Percentage (f32), and Byte (u8) without code duplication.
//   The `map` function always returns f32 (Decimal) as per the spec.
//
//   All functions are #[inline] — the compiler inlines them at the
//   call site, producing zero overhead vs writing the operation by hand.

#![no_std]

use core::cmp::Ordering;

// ── Numeric trait ─────────────────────────────────────────────────

/// Bound that covers every Ferrum numeric type:
///   i32 (Integer), f32 (Decimal / Percentage), u8 (Byte)
pub trait Numeric:
    Copy
    + PartialOrd
    + core::ops::Add<Output = Self>
    + core::ops::Sub<Output = Self>
    + core::ops::Mul<Output = Self>
    + core::ops::Div<Output = Self>
{
    fn zero() -> Self;
}

impl Numeric for i32 { fn zero() -> Self { 0 } }
impl Numeric for f32 { fn zero() -> Self { 0.0 } }
impl Numeric for u8  { fn zero() -> Self { 0 } }

/// Bound for numeric types that support negation.
/// Needed only for abs() since negation is not defined for u8.
pub trait SignedNumeric: Numeric + core::ops::Neg<Output = Self> {}

impl SignedNumeric for i32 {}
impl SignedNumeric for f32 {}

// ── abs ───────────────────────────────────────────────────────────

/// Return the absolute value of `value`.
/// Spec: abs value → same type
/// Note: only available for signed types (i32, f32)
#[inline]
pub fn abs<T: SignedNumeric>(value: T) -> T {
    if value < T::zero() { -value } else { value }
}

// ── min / max ─────────────────────────────────────────────────────

/// Return the smaller of two values.
/// Spec: min value, value → same type
#[inline]
pub fn min<T: Numeric>(a: T, b: T) -> T {
    if a <= b { a } else { b }
}

/// Return the larger of two values.
/// Spec: max value, value → same type
#[inline]
pub fn max<T: Numeric>(a: T, b: T) -> T {
    if a >= b { a } else { b }
}

/// Return the smallest of three values.
/// Variadic version used when three args are passed.
#[inline]
pub fn min3<T: Numeric>(a: T, b: T, c: T) -> T {
    min(min(a, b), c)
}

/// Return the largest of three values.
#[inline]
pub fn max3<T: Numeric>(a: T, b: T, c: T) -> T {
    max(max(a, b), c)
}

// ── clamp ─────────────────────────────────────────────────────────

/// Constrain `value` to the range [min_val, max_val].
/// Spec: clamp value, min, max → same type
///
/// If value < min_val → returns min_val
/// If value > max_val → returns max_val
/// Otherwise         → returns value unchanged
#[inline]
pub fn clamp<T: Numeric>(value: T, min_val: T, max_val: T) -> T {
    if value < min_val {
        min_val
    } else if value > max_val {
        max_val
    } else {
        value
    }
}

// ── map ───────────────────────────────────────────────────────────

/// Linear range mapping.
/// Spec: map value, from_low, from_high, to_low, to_high → Decimal
///
/// Maps `value` from the range [from_low, from_high]
///                  to the range [to_low,   to_high].
///
/// Formula: to_low + (value - from_low) * (to_high - to_low)
///                                       / (from_high - from_low)
///
/// Returns f32 (Decimal) regardless of input types.
/// Division by zero (from_high == from_low) returns to_low.
#[inline]
pub fn map(
    value:     f32,
    from_low:  f32,
    from_high: f32,
    to_low:    f32,
    to_high:   f32,
) -> f32 {
    let from_range = from_high - from_low;
    if from_range == 0.0 {
        return to_low;
    }
    to_low + (value - from_low) * (to_high - to_low) / from_range
}

#[cfg(test)]
mod math_tests {
    use super::*;

    // ── abs ──────────────────────────────────────────────────────

    #[test]
    fn abs_positive_unchanged() {
        assert_eq!(abs(5_i32), 5);
        assert!((abs(3.14_f32) - 3.14).abs() < f32::EPSILON);
    }

    #[test]
    fn abs_negative_inverted() {
        assert_eq!(abs(-7_i32), 7);
        assert!((abs(-2.5_f32) - 2.5).abs() < f32::EPSILON);
    }

    #[test]
    fn abs_zero() {
        assert_eq!(abs(0_i32), 0);
        assert_eq!(abs(0.0_f32), 0.0);
    }

    // ── min / max ─────────────────────────────────────────────────

    #[test]
    fn min_returns_smaller() {
        assert_eq!(min(3_i32, 7), 3);
        assert_eq!(min(7_i32, 3), 3);
        assert_eq!(min(5_i32, 5), 5);
    }

    #[test]
    fn max_returns_larger() {
        assert_eq!(max(3_i32, 7), 7);
        assert_eq!(max(7_i32, 3), 7);
    }

    #[test]
    fn min_float() {
        assert!((min(1.5_f32, 2.5) - 1.5).abs() < f32::EPSILON);
    }

    // ── clamp ─────────────────────────────────────────────────────

    #[test]
    fn clamp_within_range_unchanged() {
        assert_eq!(clamp(50_i32, 0, 100), 50);
        assert_eq!(clamp(50.0_f32, 0.0, 100.0), 50.0);
    }

    #[test]
    fn clamp_below_min_returns_min() {
        assert_eq!(clamp(-5_i32, 0, 100), 0);
        assert_eq!(clamp(-1.0_f32, 0.0, 100.0), 0.0);
    }

    #[test]
    fn clamp_above_max_returns_max() {
        assert_eq!(clamp(150_i32, 0, 100), 100);
        assert_eq!(clamp(105.0_f32, 0.0, 100.0), 100.0);
    }

    #[test]
    fn clamp_at_boundary_unchanged() {
        assert_eq!(clamp(0_i32, 0, 100), 0);
        assert_eq!(clamp(100_i32, 0, 100), 100);
    }

    #[test]
    fn clamp_byte_range() {
        assert_eq!(clamp(200_u8, 0_u8, 255_u8), 200);
        assert_eq!(clamp(0_u8, 0_u8, 255_u8), 0);
    }

    // ── map ───────────────────────────────────────────────────────

    #[test]
    fn map_midpoint() {
        // midpoint of [0, 100] mapped to [0.0, 1.0] → 0.5
        let result = map(50.0, 0.0, 100.0, 0.0, 1.0);
        assert!((result - 0.5).abs() < 1e-5);
    }

    #[test]
    fn map_low_bound() {
        let result = map(0.0, 0.0, 100.0, 0.0, 1.0);
        assert!((result - 0.0).abs() < 1e-5);
    }

    #[test]
    fn map_high_bound() {
        let result = map(100.0, 0.0, 100.0, 0.0, 1.0);
        assert!((result - 1.0).abs() < 1e-5);
    }

    #[test]
    fn map_inverted_output_range() {
        // map 50 from [0,100] to [100,0] → 50
        let result = map(50.0, 0.0, 100.0, 100.0, 0.0);
        assert!((result - 50.0).abs() < 1e-5);
    }

    #[test]
    fn map_zero_input_range_returns_to_low() {
        // division by zero guard
        let result = map(5.0, 5.0, 5.0, 10.0, 20.0);
        assert!((result - 10.0).abs() < 1e-5);
    }

    #[test]
    fn map_pump_speed_example_from_spec() {
        // From soil moisture example:
        // get_pump_speed: map(threshold - moisture, 0.0, threshold, 0.0, 100.0)
        // threshold=30.0, moisture=15.0 → gap=15.0
        // map(15.0, 0.0, 30.0, 0.0, 100.0) → 50.0
        let result = map(15.0, 0.0, 30.0, 0.0, 100.0);
        assert!((result - 50.0).abs() < 1e-4);
    }
}