// Ferrum standard library crate root.
//
// Provides all built-in functions from spec §13.
// No heap allocation. No std dependency. Pure no_std Rust.

#![no_std]

pub mod arrays;
pub mod convert;
pub mod format;
pub mod math;
pub mod strings;

// Re-export everything at the crate root for convenient use
// in generated code: `use ferrum_stdlib::*;`
pub use arrays::{array_add, array_length, array_remove};
pub use convert::{
    boolean_to_string, byte_to_decimal, byte_to_integer, byte_to_string,
    decimal_to_string, integer_to_decimal, integer_to_percentage,
    integer_to_string, percentage_to_string, to_decimal, to_integer,
    to_percentage,
};
pub use format::{fmt_f32, fmt_i32, FmtBuf};
pub use math::{abs, clamp, map, max, max3, min, min3};
pub use strings::{concat, concat3, includes, str_length};