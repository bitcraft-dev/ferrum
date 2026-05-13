// No-alloc integer and float formatter.
//
// This module provides the stack-allocated string buffer and
// formatting functions used by:
//   - to_string conversions (convert.rs)
//   - PRINT statement emission
//   - String concatenation helper (strings.rs)
//
// FmtBuf is a fixed 64-byte stack buffer. This covers:
//   - Any i32 (-2147483648 = 11 chars including sign)
//   - Any f32 with 2 decimal places (max ~38 digits + dot + 2 = 42)
//   - Typical PRINT messages used in student programs
//
// If a string exceeds 64 bytes it is silently truncated.
// This is acceptable for the educational context.

#![no_std]

/// Stack-allocated 64-byte string buffer.
/// Used wherever Ferrum needs to format a value as text
/// without heap allocation.
pub struct FmtBuf {
    data: [u8; 64],
    len:  usize,
}

impl FmtBuf {
    pub const fn new() -> Self {
        FmtBuf { data: [0u8; 64], len: 0 }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn push_str(&mut self, s: &str) {
        for byte in s.bytes() {
            if self.len < 64 {
                self.data[self.len] = byte;
                self.len += 1;
            }
        }
    }

    pub fn push_byte(&mut self, b: u8) {
        if self.len < 64 {
            self.data[self.len] = b;
            self.len += 1;
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn len(&self) -> usize { self.len }
}

impl Default for FmtBuf {
    fn default() -> Self { Self::new() }
}

// ── fmt_i32 ───────────────────────────────────────────────────────

/// Format an i32 into a FmtBuf. Returns a &str into the buffer.
pub fn fmt_i32(n: i32, buf: &mut FmtBuf) -> &str {
    buf.clear();
    if n == 0 {
        buf.push_byte(b'0');
        return buf.as_str();
    }
    let negative = n < 0;
    let mut val  = if negative { -(n as i64) } else { n as i64 };
    let mut tmp  = [0u8; 11];
    let mut pos  = 10usize;
    while val > 0 {
        tmp[pos] = b'0' + (val % 10) as u8;
        val /= 10;
        pos -= 1;
    }
    if negative { tmp[pos] = b'-'; pos -= 1; }
    for &b in &tmp[pos + 1..] {
        buf.push_byte(b);
    }
    buf.as_str()
}

// ── fmt_f32 ───────────────────────────────────────────────────────

/// Format an f32 into a FmtBuf with `decimals` decimal places.
/// Returns a &str into the buffer.
///
/// Handles: negative, zero, infinity, NaN.
/// Does not use any floating-point formatting from std.
pub fn fmt_f32(value: f32, decimals: u8, buf: &mut FmtBuf) -> &str {
    buf.clear();

    if value.is_nan() {
        buf.push_str("NaN");
        return buf.as_str();
    }
    if value.is_infinite() {
        buf.push_str(if value > 0.0 { "inf" } else { "-inf" });
        return buf.as_str();
    }

    let negative = value < 0.0;
    let abs_val  = if negative { -value } else { value };

    // Integer part
    let int_part = abs_val as i64;
    if negative { buf.push_byte(b'-'); }

    let mut tmp = [0u8; 20];
    if int_part == 0 {
        buf.push_byte(b'0');
    } else {
        let mut v   = int_part;
        let mut pos = 19usize;
        while v > 0 {
            tmp[pos] = b'0' + (v % 10) as u8;
            v /= 10;
            pos -= 1;
        }
        for &b in &tmp[pos + 1..] {
            buf.push_byte(b);
        }
    }

    if decimals == 0 {
        return buf.as_str();
    }

    // Decimal part
    buf.push_byte(b'.');
    let mut frac = abs_val - int_part as f32;
    for _ in 0..decimals {
        frac *= 10.0;
        let digit = frac as u8;
        buf.push_byte(b'0' + digit);
        frac -= digit as f32;
    }

    buf.as_str()
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn fmt_i32_zero() {
        let mut b = FmtBuf::new();
        assert_eq!(fmt_i32(0, &mut b), "0");
    }

    #[test]
    fn fmt_i32_positive() {
        let mut b = FmtBuf::new();
        assert_eq!(fmt_i32(42, &mut b), "42");
    }

    #[test]
    fn fmt_i32_negative() {
        let mut b = FmtBuf::new();
        assert_eq!(fmt_i32(-99, &mut b), "-99");
    }

    #[test]
    fn fmt_i32_large() {
        let mut b = FmtBuf::new();
        assert_eq!(fmt_i32(1000000, &mut b), "1000000");
    }

    #[test]
    fn fmt_f32_zero() {
        let mut b = FmtBuf::new();
        assert_eq!(fmt_f32(0.0, 2, &mut b), "0.00");
    }

    #[test]
    fn fmt_f32_two_decimals() {
        let mut b = FmtBuf::new();
        let s = fmt_f32(3.14, 2, &mut b);
        assert_eq!(s, "3.14");
    }

    #[test]
    fn fmt_f32_negative() {
        let mut b = FmtBuf::new();
        let s = fmt_f32(-1.5, 1, &mut b);
        assert_eq!(s, "-1.5");
    }

    #[test]
    fn fmt_f32_no_decimals() {
        let mut b = FmtBuf::new();
        assert_eq!(fmt_f32(42.9, 0, &mut b), "42");
    }

    #[test]
    fn fmt_buf_push_str_concat() {
        let mut b = FmtBuf::new();
        b.push_str("hello");
        b.push_str(" ");
        b.push_str("world");
        assert_eq!(b.as_str(), "hello world");
    }

    #[test]
    fn fmt_buf_clear_reuses() {
        let mut b = FmtBuf::new();
        b.push_str("old");
        b.clear();
        b.push_str("new");
        assert_eq!(b.as_str(), "new");
    }
}