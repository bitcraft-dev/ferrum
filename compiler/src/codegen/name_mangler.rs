// Name mangling — Ferrum identifier → valid Rust identifier.
//
// RULES
//   1. Ferrum identifiers are case-insensitive. Rust is case-sensitive.
//      Generated names use snake_case (lowercased).
//   2. Device type names used as struct names use PascalCase.
//   3. Ferrum names that clash with Rust keywords are suffixed with `_`.
//   4. Function names become snake_case.
//   5. Constant names become SCREAMING_SNAKE_CASE.

use crate::lexer::token::Ident;

/// Convert a Ferrum identifier to a Rust variable / function name (snake_case).
/// Uses ident.key (already lowercase).
pub fn to_snake(ident: &Ident) -> String {
    let s = escape_rust_keyword(&ident.key);
    s
}

/// Convert a Ferrum identifier to a Rust struct / type name (PascalCase).
/// Splits on underscores and capitalises each segment.
pub fn to_pascal(ident: &Ident) -> String {
    let s: String = ident.key
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            let mut c = seg.chars();
            match c.next() {
                None    => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect();
    escape_rust_keyword_type(&s)
}

/// Convert a Ferrum identifier to a Rust constant name (SCREAMING_SNAKE_CASE).
pub fn to_screaming_snake(ident: &Ident) -> String {
    ident.key.to_uppercase()
}

/// Escape a lowercase name that is a Rust keyword by appending `_`.
fn escape_rust_keyword(s: &str) -> String {
    if RUST_KEYWORDS.contains(&s) {
        format!("{}_", s)
    } else {
        s.to_string()
    }
}

fn escape_rust_keyword_type(s: &str) -> String {
    if RUST_TYPE_KEYWORDS.contains(&s.to_lowercase().as_str()) {
        format!("{}Type", s)
    } else {
        s.to_string()
    }
}

/// Rust reserved keywords that cannot be used as identifiers.
static RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
    "super", "trait", "true", "type", "unsafe", "use", "where", "while",
    "async", "await", "dyn", "abstract", "become", "box", "do", "final",
    "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
    "try",
];

static RUST_TYPE_KEYWORDS: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128",
    "isize", "str", "u8", "u16", "u32", "u64", "u128", "usize",
    "string", "option", "result", "vec",
];

#[cfg(test)]
mod mangler_tests {
    use super::*;
    use std::sync::Arc;
    use crate::lexer::token::Span;

    fn ident(s: &str) -> Ident {
        Ident::new(s, Span::new(Arc::new("test.fe".into()), 1, 1, 1))
    }

    #[test]
    fn snake_case_passthrough() {
        assert_eq!(to_snake(&ident("moisture")), "moisture");
    }

    #[test]
    fn pascal_from_snake_case() {
        assert_eq!(to_pascal(&ident("water_pump")), "WaterPump");
    }

    #[test]
    fn pascal_single_word() {
        assert_eq!(to_pascal(&ident("button")), "Button");
    }

    #[test]
    fn screaming_snake() {
        assert_eq!(to_screaming_snake(&ident("max_speed")), "MAX_SPEED");
    }

    #[test]
    fn rust_keyword_escaped() {
        assert_eq!(to_snake(&ident("loop")), "loop_");
        assert_eq!(to_snake(&ident("type")), "type_");
    }

    #[test]
    fn type_keyword_escaped() {
        assert_eq!(to_pascal(&ident("string")), "StringType");
    }
}