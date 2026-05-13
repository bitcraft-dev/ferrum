// Type conversion built-in functions.
//
// SPEC §13.2:
//   to_integer value → Integer   (truncates)
//   to_decimal value → Decimal
//   to_percentage value → Percentage  (clamped 0.0–100.0)
//   to_string value → String     (uses format module)
//
// DESIGN
//   Conversions are explicit in Ferrum — the type system does not
//   coerce automatically. These functions are the only legal way
//   to change a value's type.
//
//   to_string returns a fixed-size stack-allocated string using
//   the format module. The returned &str borrows from a caller-
//   provided buffer — no heap allocation.

#![no_std]

use crate::math::clamp;
use crate::format::{fmt_i32, fmt_f32, FmtBuf};

// ── to_integer ────────────────────────────────────────────────────

/// Convert Decimal → Integer by truncating toward zero.
/// to_integer 3.9  → 3
/// to_integer -3.9 → -3
#[inline]
pub fn to_integer(value: f32) -> i32 {
    value as i32
}

/// Convert Byte → Integer (always exact).
#[inline]
pub fn byte_to_integer(value: u8) -> i32 {
    value as i32
}

// ── to_decimal ───────────────────────────────────────────────────

/// Convert Integer → Decimal.
#[inline]
pub fn integer_to_decimal(value: i32) -> f32 {
    value as f32
}

/// Convert Byte → Decimal.
#[inline]
pub fn byte_to_decimal(value: u8) -> f32 {
    value as f32
}

/// Decimal → Decimal is a no-op, included for completeness.
#[inline]
pub fn to_decimal(value: f32) -> f32 { value }

// ── to_percentage ────────────────────────────────────────────────

/// Convert any numeric value to Percentage (f32 clamped to 0.0–100.0).
/// Spec: to_percentage clamps the input.
#[inline]
pub fn to_percentage(value: f32) -> f32 {
    clamp(value, 0.0_f32, 100.0_f32)
}

#[inline]
pub fn integer_to_percentage(value: i32) -> f32 {
    clamp(value as f32, 0.0, 100.0)
}

// ── to_string ────────────────────────────────────────────────────
//
// These functions write into a caller-provided FmtBuf and return
// a &str slice into that buffer. This is the no_alloc pattern used
// throughout the Ferrum runtime.
//
// Generated PRINT statements and string concatenation call these.

/// Integer → &str
#[inline]
pub fn integer_to_string(value: i32, buf: &mut FmtBuf) -> &str {
    fmt_i32(value, buf)
}

/// Decimal → &str (2 decimal places)
#[inline]
pub fn decimal_to_string(value: f32, buf: &mut FmtBuf) -> &str {
    fmt_f32(value, 2, buf)
}

/// Boolean → "TRUE" or "FALSE"
#[inline]
pub fn boolean_to_string(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

/// Percentage → &str (1 decimal place + "%")
#[inline]
pub fn percentage_to_string(value: f32, buf: &mut FmtBuf) -> &str {
    let s = fmt_f32(value, 1, buf);
    // buf now contains e.g. "50.0" — we cannot append "%" without
    // allocation, so the caller is expected to concatenate separately.
    // The emitter generates:  to_string(moisture) + "%" as two ops.
    s
}

/// Byte → &str (decimal representation)
#[inline]
pub fn byte_to_string(value: u8, buf: &mut FmtBuf) -> &str {
    fmt_i32(value as i32, buf)
}

#[cfg(test)]
mod convert_tests {
    use super::*;
    use crate::format::FmtBuf;

    #[test]
    fn to_integer_truncates() {
        assert_eq!(to_integer(3.9),  3);
        assert_eq!(to_integer(-3.9), -3);
        assert_eq!(to_integer(0.1),  0);
    }

    #[test]
    fn to_percentage_clamps() {
        assert!((to_percentage(50.0) - 50.0).abs() < f32::EPSILON);
        assert!((to_percentage(150.0) - 100.0).abs() < f32::EPSILON);
        assert!((to_percentage(-10.0) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn integer_to_percentage_clamps() {
        assert!((integer_to_percentage(75) - 75.0).abs() < f32::EPSILON);
        assert!((integer_to_percentage(200) - 100.0).abs() < f32::EPSILON);
        assert!((integer_to_percentage(-5) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn boolean_to_string_correct() {
        assert_eq!(boolean_to_string(true),  "TRUE");
        assert_eq!(boolean_to_string(false), "FALSE");
    }

    #[test]
    fn integer_to_string_positive() {
        let mut buf = FmtBuf::new();
        assert_eq!(integer_to_string(42, &mut buf), "42");
    }

    #[test]
    fn integer_to_string_negative() {
        let mut buf = FmtBuf::new();
        assert_eq!(integer_to_string(-7, &mut buf), "-7");
    }

    #[test]
    fn integer_to_string_zero() {
        let mut buf = FmtBuf::new();
        assert_eq!(integer_to_string(0, &mut buf), "0");
    }

    #[test]
    fn decimal_to_string_two_places() {
        let mut buf = FmtBuf::new();
        let s = decimal_to_string(3.14159, &mut buf);
        assert_eq!(s, "3.14");
    }
}