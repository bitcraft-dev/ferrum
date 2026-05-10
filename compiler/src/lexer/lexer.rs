// ferrum/compiler/src/lexer/lexer.rs
//
// Ferrum lexer — converts a .fe source string into Vec<SpannedToken>.
//
// RESPONSIBILITIES
//   • Skip whitespace and -- comments
//   • Scan and normalise all keywords (case-insensitive)
//   • Scan identifiers (case-preserving, case-insensitive lookup)
//   • Scan integer, hex, decimal, and clock literals
//   • Scan string literals with escape processing
//   • Scan duration suffixes (ms, s) after integer literals
//   • Emit Unknown(char) for unrecognised characters and continue
//     — the lexer never panics; the parser collects all errors
//
// CASE POLICY (enforced here, not in the parser)
//   Keywords:     lowercased → looked up in keyword_from_str
//                 → canonical keyword token emitted
//   Identifiers:  original text preserved in Ident::original
//                 lowercase key stored in Ident::key for lookup
//   Strings:      content between quotes preserved byte-for-byte
//   Numeric:      hex prefix/digits case-insensitive
//                 MHZ suffix case-insensitive

use std::sync::Arc;

use super::token::{
    keyword_from_str, Ident, Span, SpannedToken, Spanned, Token,
};

// ----------------------------------------------------------------
// LexError
// ----------------------------------------------------------------

/// A non-fatal lexer diagnostic.
/// The lexer collects these and continues rather than aborting,
/// so the parser sees as many tokens as possible.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span:    Span,
}

impl LexError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        LexError { message: message.into(), span }
    }
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] LexError: {}", self.span, self.message)
    }
}

// ----------------------------------------------------------------
// LexResult
// ----------------------------------------------------------------

/// The result of a successful lex pass.
pub struct LexResult {
    /// Token stream. Always ends with a `Token::Eof`.
    pub tokens: Vec<SpannedToken>,
    /// Non-fatal errors encountered during scanning.
    /// If non-empty the compiler should report them, but the
    /// token stream is still valid enough for the parser to
    /// attempt a best-effort parse.
    pub errors: Vec<LexError>,
}

// ----------------------------------------------------------------
// Lexer
// ----------------------------------------------------------------

pub struct Lexer {
    /// The complete source text as a Vec of chars for O(1) indexing.
    source:  Vec<char>,
    /// Current position in `source`.
    pos:     usize,
    /// Current line number (1-based).
    line:    u32,
    /// Current column number (1-based).
    column:  u32,
    /// Shared file path — stored in every Span produced.
    file:    Arc<String>,
    /// Accumulated output tokens.
    tokens:  Vec<SpannedToken>,
    /// Accumulated non-fatal errors.
    errors:  Vec<LexError>,
}

impl Lexer {
    // ── Construction ────────────────────────────────────────────

    pub fn new(source: &str, file: impl Into<String>) -> Self {
        Lexer {
            source:  source.chars().collect(),
            pos:     0,
            line:    1,
            column:  1,
            file:    Arc::new(file.into()),
            tokens:  Vec::new(),
            errors:  Vec::new(),
        }
    }

    // ── Public entry point ───────────────────────────────────────

    pub fn tokenise(mut self) -> LexResult {
        loop {
            self.skip_whitespace_and_comments();

            if self.is_at_end() {
                let span = self.span_here(0);
                self.tokens.push(Spanned::new(Token::Eof, span));
                break;
            }

            self.scan_token();
        }

        LexResult {
            tokens: self.tokens,
            errors: self.errors,
        }
    }

    // ── Core scan dispatcher ────────────────────────────────────

    fn scan_token(&mut self) {
        let start_line   = self.line;
        let start_column = self.column;
        let ch           = self.advance();

        match ch {
            // Single-character punctuation
            '{' => self.emit_at(Token::LBrace,   start_line, start_column, 1),
            '}' => self.emit_at(Token::RBrace,   start_line, start_column, 1),
            '[' => self.emit_at(Token::LBracket, start_line, start_column, 1),
            ']' => self.emit_at(Token::RBracket, start_line, start_column, 1),
            '(' => self.emit_at(Token::LParen,   start_line, start_column, 1),
            ')' => self.emit_at(Token::RParen,   start_line, start_column, 1),
            ',' => self.emit_at(Token::Comma,    start_line, start_column, 1),
            ':' => self.emit_at(Token::Colon,    start_line, start_column, 1),
            '+' => self.emit_at(Token::Plus,     start_line, start_column, 1),
            '*' => self.emit_at(Token::Star,     start_line, start_column, 1),
            '/' => self.emit_at(Token::Slash,    start_line, start_column, 1),

            // One-or-two character operators
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.emit_at(Token::EqEq, start_line, start_column, 2);
                } else {
                    self.emit_at(Token::Eq, start_line, start_column, 1);
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.emit_at(Token::NotEq, start_line, start_column, 2);
                } else {
                    // '!' alone is not valid in Ferrum
                    let span = self.span_at(start_line, start_column, 1);
                    self.errors.push(LexError::new(
                        "unexpected '!' — did you mean '!='?",
                        span.clone(),
                    ));
                    self.tokens.push(Spanned::new(Token::Unknown('!'), span));
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.emit_at(Token::GtEq, start_line, start_column, 2);
                } else {
                    self.emit_at(Token::Gt, start_line, start_column, 1);
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.emit_at(Token::LtEq, start_line, start_column, 2);
                } else {
                    self.emit_at(Token::Lt, start_line, start_column, 1);
                }
            }
            '-' => {
                // Minus used as subtraction operator.
                // Comments (--) are consumed in skip_whitespace_and_comments
                // before scan_token is called, so a lone '-' here is always
                // the arithmetic operator.
                self.emit_at(Token::Minus, start_line, start_column, 1);
            }

            // String literal
            '"' => self.scan_string(start_line, start_column),

            // Numbers (integer, hex, decimal, clock)
            c if c.is_ascii_digit() => {
                self.scan_number(c, start_line, start_column);
            }

            // Identifiers and keywords (including READ_PERCENT, ANALOG_INPUT)
            c if c.is_alphabetic() || c == '_' => {
                self.scan_word(c, start_line, start_column);
            }

            // Unknown character — emit and continue
            c => {
                let span = self.span_at(start_line, start_column, 1);
                self.errors.push(LexError::new(
                    format!("unexpected character '{}'", c),
                    span.clone(),
                ));
                self.tokens.push(Spanned::new(Token::Unknown(c), span));
            }
        }
    }

    // ── Whitespace and comment skipping ─────────────────────────

    /// Skip any combination of whitespace and `--` line comments.
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(c) = self.current() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            // Skip `--` line comments
            if self.current() == Some('-') && self.peek() == Some('-') {
                // Consume both dashes
                self.advance();
                self.advance();
                // Consume until end of line (or EOF)
                while let Some(c) = self.current() {
                    self.advance();
                    if c == '\n' { break; }
                }
            } else {
                break;
            }
        }
    }

    // ── String literal scanner ──────────────────────────────────

    /// Called after the opening `"` has been consumed.
    fn scan_string(&mut self, start_line: u32, start_col: u32) {
        let mut value   = String::new();
        let mut closed  = false;

        while let Some(c) = self.current() {
            match c {
                '"' => {
                    self.advance();
                    closed = true;
                    break;
                }
                '\\' => {
                    self.advance(); // consume backslash
                    match self.current() {
                        Some('"')  => { self.advance(); value.push('"');  }
                        Some('\\') => { self.advance(); value.push('\\'); }
                        Some('n')  => { self.advance(); value.push('\n'); }
                        Some(c) => {
                            // Unrecognised escape — preserve literally and warn
                            let esc_span = self.span_here(2);
                            self.errors.push(LexError::new(
                                format!("unknown escape sequence '\\{}' in string", c),
                                esc_span,
                            ));
                            value.push('\\');
                            value.push(c);
                            self.advance();
                        }
                        None => break,
                    }
                }
                '\n' => {
                    // Unterminated string — newline inside string literal
                    let span = self.span_at(start_line, start_col,
                        (self.column - start_col) as u32);
                    self.errors.push(LexError::new(
                        "unterminated string literal — newline inside string",
                        span,
                    ));
                    break;
                }
                c => {
                    value.push(c);
                    self.advance();
                }
            }
        }

        if !closed && self.is_at_end() {
            let span = self.span_at(start_line, start_col,
                (self.pos as u32).saturating_sub(start_col));
            self.errors.push(LexError::new(
                "unterminated string literal — reached end of file",
                span,
            ));
        }

        let length = value.len() as u32 + 2; // +2 for the quotes
        let span = self.span_at(start_line, start_col, length);
        self.tokens.push(Spanned::new(Token::StringLit(value), span));
    }

    // ── Number scanner ──────────────────────────────────────────

    /// Called after the first digit `first_digit` has been consumed.
    ///
    /// Handles:
    ///   0x / 0X prefix  → HexLit
    ///   digits.digits   → DecimalLit
    ///   digitsMHZ       → ClockLit
    ///   digits          → IntLit  (may be followed by ms/s suffix below)
    fn scan_number(&mut self, first_digit: char, start_line: u32, start_col: u32) {
        // Check for hex prefix: 0x or 0X
        if first_digit == '0' && matches!(self.current(), Some('x') | Some('X')) {
            self.advance(); // consume 'x' or 'X'
            return self.scan_hex(start_line, start_col);
        }

        // Collect remaining digits
        let mut raw = String::from(first_digit);
        while let Some(c) = self.current() {
            if c.is_ascii_digit() {
                raw.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Check for decimal point
        if self.current() == Some('.') && self.peek().map_or(false, |c| c.is_ascii_digit()) {
            raw.push('.');
            self.advance(); // consume '.'
            while let Some(c) = self.current() {
                if c.is_ascii_digit() {
                    raw.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            let length = raw.len() as u32;
            let span   = self.span_at(start_line, start_col, length);
            match raw.parse::<f64>() {
                Ok(f) => self.tokens.push(Spanned::new(Token::DecimalLit(f), span)),
                Err(_) => {
                    self.errors.push(LexError::new(
                        format!("invalid decimal literal '{}'", raw), span.clone()));
                    self.tokens.push(Spanned::new(Token::Unknown('.'), span));
                }
            }
            return;
        }

        // Check for MHZ clock suffix (case-insensitive)
        let suffix_start = self.pos;
        if self.peek_word_ci() == "mhz" {
            // Consume the three characters
            self.advance(); self.advance(); self.advance();
            let length = (self.pos - (start_col as usize - 1)) as u32;
            let span   = self.span_at(start_line, start_col, raw.len() as u32 + 3);
            match raw.parse::<u32>() {
                Ok(n) => self.tokens.push(Spanned::new(Token::ClockLit(n), span)),
                Err(_) => {
                    self.errors.push(LexError::new(
                        format!("invalid clock literal '{}MHZ'", raw), span.clone()));
                    self.tokens.push(Spanned::new(Token::Unknown('M'), span));
                }
            }
            let _ = suffix_start; // suppress unused warning
            return;
        }

        // Plain integer
        let length = raw.len() as u32;
        let span   = self.span_at(start_line, start_col, length);
        match raw.parse::<i64>() {
            Ok(n) => {
                self.tokens.push(Spanned::new(Token::IntLit(n), span.clone()));
                // Check for duration suffix: ms or s immediately after digits
                // (no space allowed between the number and the suffix)
                self.maybe_emit_duration_suffix(start_line, start_col + length);
            }
            Err(_) => {
                self.errors.push(LexError::new(
                    format!("integer literal '{}' overflows i64", raw), span.clone()));
                self.tokens.push(Spanned::new(Token::Unknown('0'), span));
            }
        }
    }

    /// Scan hex digits after the `0x` prefix has been consumed.
    fn scan_hex(&mut self, start_line: u32, start_col: u32) {
        let mut raw = String::new();
        while let Some(c) = self.current() {
            if c.is_ascii_hexdigit() {
                raw.push(c);
                self.advance();
            } else {
                break;
            }
        }

        if raw.is_empty() {
            let span = self.span_at(start_line, start_col, 2);
            self.errors.push(LexError::new(
                "hex literal '0x' has no digits", span.clone()));
            self.tokens.push(Spanned::new(Token::Unknown('x'), span));
            return;
        }

        let length = raw.len() as u32 + 2; // +2 for "0x"
        let span   = self.span_at(start_line, start_col, length);
        match u64::from_str_radix(&raw, 16) {
            Ok(n) => self.tokens.push(Spanned::new(Token::HexLit(n), span)),
            Err(_) => {
                self.errors.push(LexError::new(
                    format!("invalid hex literal '0x{}'", raw), span.clone()));
                self.tokens.push(Spanned::new(Token::Unknown('x'), span));
            }
        }
    }

    /// After emitting an IntLit, check whether `ms` or `s` follows
    /// immediately (no whitespace). If so, emit the suffix token.
    ///
    /// Duration suffix tokens are separate tokens so the parser can
    /// assemble them into a `Duration` node: IntLit + Ms/S.
    fn maybe_emit_duration_suffix(&mut self, line: u32, col: u32) {
        // Peek at next two characters without consuming
        let a = self.current();
        let b = self.peek();

        match (a.map(|c| c.to_ascii_lowercase()),
               b.map(|c| c.to_ascii_lowercase())) {
            // "ms" — consume both
            (Some('m'), Some('s')) => {
                self.advance();
                self.advance();
                self.emit_at(Token::Ms, line, col, 2);
            }
            // bare "s" not followed by a letter (would be an identifier)
            (Some('s'), next) if !next.map_or(false, |c| c.is_alphabetic() || c == '_') => {
                self.advance();
                self.emit_at(Token::S, line, col, 1);
            }
            _ => { /* no suffix — plain integer */ }
        }
    }

    // ── Word scanner (identifiers + keywords) ───────────────────

    /// Called after the first character `first` has been consumed.
    /// Scans the remainder of the word, then classifies it.
    ///
    /// Words include underscores, which is why ANALOG_INPUT and
    /// READ_PERCENT are treated as single tokens.
    fn scan_word(&mut self, first: char, start_line: u32, start_col: u32) {
        let mut raw = String::from(first);

        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' {
                raw.push(c);
                self.advance();
            } else {
                break;
            }
        }

        let length   = raw.len() as u32;
        let span     = self.span_at(start_line, start_col, length);
        let lower    = raw.to_lowercase();

        match keyword_from_str(&lower) {
            Some(kw) => {
                // Keyword — emit canonical token.
                // The original capitalisation is discarded for keywords;
                // only identifier spellings are preserved.
                self.tokens.push(Spanned::new(kw, span));
            }
            None => {
                // User-defined identifier — preserve original spelling.
                let ident = Ident::new(&raw, span.clone());
                self.tokens.push(Spanned::new(Token::Ident(ident), span));
            }
        }
    }

    // ── Helpers ─────────────────────────────────────────────────

    /// Return the current character without consuming it.
    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    /// Return the character one position ahead without consuming.
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    /// Peek at the next three characters as a lowercase string.
    /// Used for MHZ suffix detection.
    fn peek_word_ci(&self) -> String {
        self.source[self.pos..].iter()
            .take(3)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    /// Consume the current character, updating line/column tracking,
    /// and return it. Panics if called at EOF — always guard with
    /// `is_at_end()` or `current().is_some()` first.
    fn advance(&mut self) -> char {
        let c = self.source[self.pos];
        self.pos += 1;
        if c == '\n' {
            self.line   += 1;
            self.column  = 1;
        } else {
            self.column += 1;
        }
        c
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    /// Build a Span at the given position with the given byte length.
    fn span_at(&self, line: u32, column: u32, length: u32) -> Span {
        Span::new(self.file.clone(), line, column, length)
    }

    /// Build a Span at the *current* cursor position with zero length.
    /// Used for EOF and synthetic tokens.
    fn span_here(&self, length: u32) -> Span {
        Span::new(self.file.clone(), self.line, self.column, length)
    }

    /// Emit a token with an explicitly provided position.
    fn emit_at(&mut self, token: Token, line: u32, column: u32, length: u32) {
        let span = self.span_at(line, column, length);
        self.tokens.push(Spanned::new(token, span));
    }
}

// ----------------------------------------------------------------
// Public convenience function
// ----------------------------------------------------------------

/// Lex a Ferrum source string, returning all tokens and any
/// non-fatal errors. Always returns at least one token (Eof).
pub fn lex(source: &str, filename: impl Into<String>) -> LexResult {
    Lexer::new(source, filename).tokenise()
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::token::Token;

    fn tokens(source: &str) -> Vec<Token> {
        lex(source, "<test>")
            .tokens
            .into_iter()
            .map(|st| st.node)
            .collect()
    }

    fn errors(source: &str) -> Vec<LexError> {
        lex(source, "<test>").errors
    }

    // ── Whitespace and comments ─────────────────────────────────

    #[test]
    fn empty_source_gives_only_eof() {
        assert_eq!(tokens(""), vec![Token::Eof]);
    }

    #[test]
    fn whitespace_only_gives_eof() {
        assert_eq!(tokens("   \t\n  "), vec![Token::Eof]);
    }

    #[test]
    fn line_comment_is_skipped() {
        let t = tokens("-- this is a comment\nRUN");
        assert_eq!(t, vec![Token::Run, Token::Eof]);
    }

    #[test]
    fn inline_comment_is_skipped() {
        let t = tokens("LOOP -- start loop\n{");
        assert_eq!(t, vec![Token::Loop, Token::LBrace, Token::Eof]);
    }

    #[test]
    fn multiple_comments_skipped() {
        let t = tokens("-- first\n-- second\nCONFIG");
        assert_eq!(t, vec![Token::Config, Token::Eof]);
    }

    // ── Case insensitivity ──────────────────────────────────────

    #[test]
    fn keywords_case_insensitive() {
        let t = tokens("config Config CONFIG cOnFiG");
        assert_eq!(t, vec![
            Token::Config, Token::Config, Token::Config, Token::Config,
            Token::Eof
        ]);
    }

    #[test]
    fn type_names_case_insensitive() {
        let t = tokens("integer Integer INTEGER");
        assert_eq!(t, vec![
            Token::TyInteger, Token::TyInteger, Token::TyInteger,
            Token::Eof
        ]);
    }

    #[test]
    fn ownership_keywords_case_insensitive() {
        let t = tokens("give Give GIVE lend LEND borrow BORROW");
        assert_eq!(t, vec![
            Token::Give, Token::Give, Token::Give,
            Token::Lend, Token::Lend,
            Token::Borrow, Token::Borrow,
            Token::Eof,
        ]);
    }

    // ── Identifiers ─────────────────────────────────────────────

    #[test]
    fn identifier_preserves_original_spelling() {
        let result = lex("StatusLed", "<test>");
        match &result.tokens[0].node {
            Token::Ident(i) => {
                assert_eq!(i.original, "StatusLed");
                assert_eq!(i.key,      "statusled");
            }
            other => panic!("expected Ident, got {:?}", other),
        }
    }

    #[test]
    fn identifiers_with_underscores() {
        let result = lex("mode_btn dry_threshold", "<test>");
        let t: Vec<_> = result.tokens.iter().map(|s| &s.node).collect();
        assert!(matches!(t[0], Token::Ident(i) if i.key == "mode_btn"));
        assert!(matches!(t[1], Token::Ident(i) if i.key == "dry_threshold"));
    }

    #[test]
    fn read_percent_is_single_token() {
        // READ_PERCENT must scan as one token, not READ + unknown + PERCENT
        let t = tokens("READ_PERCENT soil");
        assert_eq!(t[0], Token::ReadPercent);
        assert!(matches!(&t[1], Token::Ident(i) if i.key == "soil"));
    }

    #[test]
    fn analog_input_is_single_token() {
        let t = tokens("ANALOG_INPUT");
        assert_eq!(t[0], Token::AnalogInput);
    }

    // ── Integer literals ────────────────────────────────────────

    #[test]
    fn integer_literal() {
        let t = tokens("42");
        assert_eq!(t[0], Token::IntLit(42));
    }

    #[test]
    fn zero_integer() {
        let t = tokens("0");
        assert_eq!(t[0], Token::IntLit(0));
    }

    #[test]
    fn large_integer() {
        let t = tokens("1023");
        assert_eq!(t[0], Token::IntLit(1023));
    }

    // ── Hex literals ─────────────────────────────────────────────

    #[test]
    fn hex_literal_lowercase_prefix() {
        let t = tokens("0xff");
        assert_eq!(t[0], Token::HexLit(0xff));
    }

    #[test]
    fn hex_literal_uppercase_prefix() {
        let t = tokens("0XFF");
        assert_eq!(t[0], Token::HexLit(0xFF));
    }

    #[test]
    fn hex_literal_mixed_digits() {
        let t = tokens("0x27");
        assert_eq!(t[0], Token::HexLit(0x27));
    }

    #[test]
    fn hex_literal_no_digits_is_error() {
        let e = errors("0x");
        assert!(!e.is_empty());
        assert!(e[0].message.contains("no digits"));
    }

    // ── Decimal literals ─────────────────────────────────────────

    #[test]
    fn decimal_literal() {
        let t = tokens("3.14");
        assert_eq!(t[0], Token::DecimalLit(3.14));
    }

    #[test]
    fn decimal_zero_point_five() {
        let t = tokens("0.5");
        assert_eq!(t[0], Token::DecimalLit(0.5));
    }

    #[test]
    fn decimal_with_leading_dot_is_not_decimal() {
        // ".5" is not a valid decimal literal in Ferrum — must have
        // digits on both sides of the point.
        let t = tokens(".5");
        // '.' is not a valid start character, gets Unknown or similar
        assert!(matches!(t[0], Token::Unknown('.')));
    }

    // ── Clock literals ───────────────────────────────────────────

    #[test]
    fn clock_literal_uppercase() {
        let t = tokens("64MHZ");
        assert_eq!(t[0], Token::ClockLit(64));
    }

    #[test]
    fn clock_literal_lowercase() {
        let t = tokens("64mhz");
        assert_eq!(t[0], Token::ClockLit(64));
    }

    #[test]
    fn clock_literal_mixed_case() {
        let t = tokens("64Mhz");
        assert_eq!(t[0], Token::ClockLit(64));
    }

    // ── Duration suffixes ────────────────────────────────────────

    #[test]
    fn duration_ms_suffix() {
        let t = tokens("500ms");
        assert_eq!(t[0], Token::IntLit(500));
        assert_eq!(t[1], Token::Ms);
    }

    #[test]
    fn duration_s_suffix() {
        let t = tokens("2s");
        assert_eq!(t[0], Token::IntLit(2));
        assert_eq!(t[1], Token::S);
    }

    #[test]
    fn duration_ms_uppercase() {
        // ms is case-insensitive
        let t = tokens("100MS");
        assert_eq!(t[0], Token::IntLit(100));
        assert_eq!(t[1], Token::Ms);
    }

    #[test]
    fn integer_not_followed_by_suffix() {
        // Plain integer with no suffix
        let t = tokens("42 ");
        assert_eq!(t[0], Token::IntLit(42));
        assert_eq!(t[1], Token::Eof);
    }

    // ── String literals ──────────────────────────────────────────

    #[test]
    fn string_literal_basic() {
        let t = tokens(r#""hello""#);
        assert_eq!(t[0], Token::StringLit("hello".into()));
    }

    #[test]
    fn string_literal_escape_quote() {
        let t = tokens(r#""say \"hi\"""#);
        assert_eq!(t[0], Token::StringLit(r#"say "hi""#.into()));
    }

    #[test]
    fn string_literal_escape_newline() {
        let t = tokens(r#""line1\nline2""#);
        assert_eq!(t[0], Token::StringLit("line1\nline2".into()));
    }

    #[test]
    fn string_literal_escape_backslash() {
        let t = tokens(r#""path\\file""#);
        assert_eq!(t[0], Token::StringLit("path\\file".into()));
    }

    #[test]
    fn string_literal_is_case_sensitive() {
        let t = tokens(r#""Ready""#);
        assert_eq!(t[0], Token::StringLit("Ready".into()));
        // Distinct from "ready"
        let t2 = tokens(r#""ready""#);
        assert_ne!(t[0], t2[0]);
    }

    #[test]
    fn unterminated_string_emits_error() {
        let e = errors(r#""unterminated"#);
        assert!(!e.is_empty());
    }

    // ── Punctuation ──────────────────────────────────────────────

    #[test]
    fn punctuation_tokens() {
        let t = tokens("{ } [ ] ( ) , : = == != > < >= <= + - * /");
        let expected = vec![
            Token::LBrace, Token::RBrace,
            Token::LBracket, Token::RBracket,
            Token::LParen, Token::RParen,
            Token::Comma, Token::Colon,
            Token::Eq, Token::EqEq, Token::NotEq,
            Token::Gt, Token::Lt, Token::GtEq, Token::LtEq,
            Token::Plus, Token::Minus, Token::Star, Token::Slash,
            Token::Eof,
        ];
        assert_eq!(t, expected);
    }

    // ── Span accuracy ────────────────────────────────────────────

    #[test]
    fn span_line_column_are_correct() {
        let result = lex("RUN\n{", "<test>");
        let run_span = &result.tokens[0].span;
        assert_eq!(run_span.line,   1);
        assert_eq!(run_span.column, 1);
        assert_eq!(run_span.length, 3);

        let brace_span = &result.tokens[1].span;
        assert_eq!(brace_span.line,   2);
        assert_eq!(brace_span.column, 1);
        assert_eq!(brace_span.length, 1);
    }

    // ── Unknown character ────────────────────────────────────────

    #[test]
    fn unknown_character_emits_error_and_continues() {
        let result = lex("RUN @ {", "<test>");
        // Should get RUN, then Unknown('@'), then LBrace, then Eof
        let t: Vec<_> = result.tokens.iter().map(|s| &s.node).collect();
        assert_eq!(t[0], &Token::Run);
        assert_eq!(t[1], &Token::Unknown('@'));
        assert_eq!(t[2], &Token::LBrace);
        assert!(!result.errors.is_empty());
    }

    // ── Complete mini-program ────────────────────────────────────

    #[test]
    fn complete_mini_program_tokenises_correctly() {
        let src = r#"
-- Simple blink program
CONFIG {
    TARGET = "microbit_v2",
    DEBUG = TRUE
}

DEFINE Led AS OUTPUT

CREATE Led status ON PIN 13

RUN {
    LOOP {
        TURN status HIGH
        DELAY 500ms
        TURN status LOW
        DELAY 500ms
    }
}
"#;
        let result = lex(src, "blink.fe");
        assert!(result.errors.is_empty(), "unexpected errors: {:?}", result.errors);

        let t: Vec<_> = result.tokens.iter().map(|s| &s.node).collect();

        // Spot-check the sequence
        assert_eq!(t[0],  &Token::Config);
        assert_eq!(t[1],  &Token::LBrace);
        // TARGET = "microbit_v2",
        assert!(matches!(t[2], Token::Ident(i) if i.key == "target"));
        assert_eq!(t[3],  &Token::Eq);
        assert_eq!(t[4],  &Token::StringLit("microbit_v2".into()));
        assert_eq!(t[5],  &Token::Comma);
        // DEBUG = TRUE,
        assert!(matches!(t[6], Token::Ident(i) if i.key == "debug"));
        assert_eq!(t[7],  &Token::Eq);
        assert_eq!(t[8],  &Token::True);
        assert_eq!(t[9],  &Token::RBrace);
        // DEFINE Led AS OUTPUT
        assert_eq!(t[10], &Token::Define);
        assert!(matches!(t[11], Token::Ident(i) if i.key == "led"));
        assert_eq!(t[12], &Token::As);
        assert_eq!(t[13], &Token::Output);

        // Last token is always Eof
        assert_eq!(t.last().unwrap(), &&Token::Eof);
    }
}
