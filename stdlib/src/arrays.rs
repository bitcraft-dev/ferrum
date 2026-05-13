// Array built-in functions.
//
// SPEC §13.4:
//   length array → Integer
//   ADD array, value → void   (appends)
//   REMOVE array, index → void (removes at index)
//
// DESIGN
//   Ferrum arrays are fixed-size stack arrays: [T; N].
//   ADD and REMOVE operate on a (slice, &mut len) pair where
//   `len` tracks the logical fill level within the fixed buffer.
//
//   This is the standard embedded pattern for "growable" arrays
//   without heap allocation. The compiler enforces that N is
//   always known at compile time and the array never overflows.
//
//   The `length` function for arrays is distinct from str_length
//   in strings.rs. The emitter chooses which to call based on
//   the argument type resolved by the semantic pass.

#![no_std]

// ── length ────────────────────────────────────────────────────────

/// Return the logical length of an array (number of filled slots).
/// Spec: length array → Integer
///
/// `len` is the current fill level, not the buffer capacity.
#[inline]
pub fn array_length(len: usize) -> i32 {
    len as i32
}

// ── ADD ───────────────────────────────────────────────────────────

/// Append `value` to the array at position `*len`, then increment `len`.
/// Spec: ADD array, value → void
///
/// If the buffer is full (len >= capacity), the call is a no-op
/// and returns false. The compiler guarantees this does not happen
/// for well-formed programs (semantic constraint on array bounds),
/// but a runtime guard is included for safety.
#[inline]
pub fn array_add<T: Copy>(buf: &mut [T], len: &mut usize, value: T) -> bool {
    if *len >= buf.len() {
        return false; // buffer full
    }
    buf[*len] = value;
    *len += 1;
    true
}

// ── REMOVE ───────────────────────────────────────────────────────

/// Remove the element at `index` by shifting subsequent elements left.
/// Spec: REMOVE array, index → void
///
/// Returns false if `index >= len` (out of bounds).
/// Elements after `index` are shifted down; `*len` is decremented.
#[inline]
pub fn array_remove<T: Copy + Default>(
    buf: &mut [T],
    len: &mut usize,
    index: usize,
) -> bool {
    if index >= *len {
        return false;
    }
    for i in index..*len - 1 {
        buf[i] = buf[i + 1];
    }
    buf[*len - 1] = T::default();
    *len -= 1;
    true
}

#[cfg(test)]
mod array_tests {
    use super::*;

    #[test]
    fn length_empty() {
        assert_eq!(array_length(0), 0);
    }

    #[test]
    fn length_three_elements() {
        assert_eq!(array_length(3), 3);
    }

    #[test]
    fn add_appends_value() {
        let mut buf = [0i32; 5];
        let mut len = 0usize;
        assert!(array_add(&mut buf, &mut len, 42));
        assert_eq!(len, 1);
        assert_eq!(buf[0], 42);
    }

    #[test]
    fn add_fills_buffer() {
        let mut buf = [0i32; 3];
        let mut len = 0usize;
        assert!(array_add(&mut buf, &mut len, 1));
        assert!(array_add(&mut buf, &mut len, 2));
        assert!(array_add(&mut buf, &mut len, 3));
        assert!(!array_add(&mut buf, &mut len, 4)); // full
        assert_eq!(len, 3);
    }

    #[test]
    fn remove_shifts_elements() {
        let mut buf = [10i32, 20, 30, 0, 0];
        let mut len = 3usize;
        assert!(array_remove(&mut buf, &mut len, 1)); // remove 20
        assert_eq!(len, 2);
        assert_eq!(buf[0], 10);
        assert_eq!(buf[1], 30);
    }

    #[test]
    fn remove_first_element() {
        let mut buf = [1i32, 2, 3, 0, 0];
        let mut len = 3usize;
        array_remove(&mut buf, &mut len, 0);
        assert_eq!(buf[0], 2);
        assert_eq!(buf[1], 3);
        assert_eq!(len, 2);
    }

    #[test]
    fn remove_last_element() {
        let mut buf = [1i32, 2, 3, 0, 0];
        let mut len = 3usize;
        array_remove(&mut buf, &mut len, 2);
        assert_eq!(len, 2);
        assert_eq!(buf[0], 1);
        assert_eq!(buf[1], 2);
    }

    #[test]
    fn remove_out_of_bounds_returns_false() {
        let mut buf = [1i32, 2, 3, 0, 0];
        let mut len = 3usize;
        assert!(!array_remove(&mut buf, &mut len, 5));
        assert_eq!(len, 3); // unchanged
    }
}
