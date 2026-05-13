// String built-in functions.
//
// SPEC §13.3:
//   length string → Integer
//   includes string, substring → Boolean
//
// DESIGN
//   Ferrum strings are &'static str — string literals baked into
//   the binary. The stdlib operates on &str slices directly.
//   No heap allocation anywhere in this module.

#![no_std]

// ── length ────────────────────────────────────────────────────────

/// Return the number of characters in a string.
/// Spec: length string → Integer
///
/// Uses char count (not byte count) for correct Unicode handling.
/// For ASCII-only Ferrum strings this is identical to byte count.
#[inline]
pub fn str_length(s: &str) -> i32 {
    s.chars().count() as i32
}

// ── includes ─────────────────────────────────────────────────────

/// Return TRUE if `s` contains `substring`.
/// Spec: includes string, substring → Boolean
#[inline]
pub fn includes(s: &str, substring: &str) -> bool {
    s.contains(substring)
}

// ── concat ────────────────────────────────────────────────────────
//
// String concatenation with + is handled by the emitter using a
// concat_into helper that writes into a fixed stack buffer.
// This is not a user-visible function — it is an emitter helper.

use crate::format::FmtBuf;

/// Write `a` then `b` into `buf`, return the combined &str.
/// Used by the emitter for: "hello" + " world"
pub fn concat<'a>(a: &str, b: &str, buf: &'a mut FmtBuf) -> &'a str {
    buf.clear();
    buf.push_str(a);
    buf.push_str(b);
    buf.as_str()
}

/// Write `a`, `b`, and `c` into `buf`.
/// Used for three-way concatenation: a + b + c
pub fn concat3<'a>(a: &str, b: &str, c: &str, buf: &'a mut FmtBuf) -> &'a str {
    buf.clear();
    buf.push_str(a);
    buf.push_str(b);
    buf.push_str(c);
    buf.as_str()
}

#[cfg(test)]
mod string_tests {
    use super::*;
    use crate::format::FmtBuf;

    #[test]
    fn length_empty() {
        assert_eq!(str_length(""), 0);
    }

    #[test]
    fn length_ascii() {
        assert_eq!(str_length("hello"), 5);
    }

    #[test]
    fn length_longer() {
        assert_eq!(str_length("microbit"), 8);
    }

    #[test]
    fn includes_present() {
        assert!(includes("sensor_1", "sensor"));
        assert!(includes("hello world", "world"));
    }

    #[test]
    fn includes_absent() {
        assert!(!includes("sensor_1", "pump"));
        assert!(!includes("hello", "xyz"));
    }

    #[test]
    fn includes_empty_substring() {
        // Empty string is always a substring
        assert!(includes("hello", ""));
    }

    #[test]
    fn includes_exact_match() {
        assert!(includes("hello", "hello"));
    }

    #[test]
    fn concat_two_strings() {
        let mut buf = FmtBuf::new();
        let result = concat("hello ", "world", &mut buf);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn concat_three_strings() {
        let mut buf = FmtBuf::new();
        let result = concat3("a", "b", "c", &mut buf);
        assert_eq!(result, "abc");
    }
}
