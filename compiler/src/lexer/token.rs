// ferrum/compiler/src/lexer/token.rs
//
// Every terminal in the Ferrum EBNF grammar maps to exactly one
// variant in this enum. The lexer produces a Vec<Spanned<Token>>;
// the parser consumes it.
//
// CASE POLICY
//   Keywords  — lexer normalises to uppercase before matching;
//               variants here represent the canonical form.
//   Identifiers — carried as original_text + normalised_key;
//               see the `Ident` struct below.
//   Strings   — content between quotes is preserved verbatim.

// ----------------------------------------------------------------
// Span
// ----------------------------------------------------------------

/// Source location of a single token or AST node.
/// Every token carries one; every AST node carries one built from
/// the span of its constituent tokens.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    /// Source file path or "<stdin>" for REPL use.
    pub file:   std::sync::Arc<String>,
    /// 1-based line number.
    pub line:   u32,
    /// 1-based column of the first character.
    pub column: u32,
    /// Byte length of the token in the source.
    pub length: u32,
}

impl Span {
    pub fn new(file: std::sync::Arc<String>, line: u32, column: u32, length: u32) -> Self {
        Span { file, line, column, length }
    }

    /// Merge two spans into one covering both ends.
    /// Assumes both spans are from the same file.
    pub fn to(&self, other: &Span) -> Span {
        let end = other.column + other.length;
        Span {
            file:   self.file.clone(),
            line:   self.line,
            column: self.column,
            length: end.saturating_sub(self.column),
        }
    }

    /// A dummy span for generated or synthetic nodes.
    /// Should never appear in user-facing diagnostics.
    pub fn synthetic(file: std::sync::Arc<String>) -> Self {
        Span { file, line: 0, column: 0, length: 0 }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

// ----------------------------------------------------------------
// Spanned<T>
// ----------------------------------------------------------------

/// A value paired with its source location.
/// The lexer produces `Vec<Spanned<Token>>`.
/// AST nodes are built from spanned tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Spanned { node, span }
    }
}

/// Convenience alias used throughout the compiler.
pub type SpannedToken = Spanned<Token>;

// ----------------------------------------------------------------
// Ident — case-preserving, case-insensitive identifier
// ----------------------------------------------------------------

/// A user-defined name.
///
/// `original`  — exactly as the author wrote it; used in diagnostics.
/// `key`       — `original.to_lowercase()`; used for all comparisons.
///
/// Two `Ident` values are equal iff their `key` fields are equal,
/// regardless of how the author capitalised them.
#[derive(Debug, Clone, Eq)]
pub struct Ident {
    pub original: String,
    pub key:      String,
    pub span:     Span,
}

impl Ident {
    pub fn new(original: &str, span: Span) -> Self {
        Ident {
            key:      original.to_lowercase(),
            original: original.to_string(),
            span,
        }
    }
}

impl PartialEq for Ident {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl std::hash::Hash for Ident {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.original)
    }
}

// ----------------------------------------------------------------
// Token
// ----------------------------------------------------------------

/// Every terminal in the Ferrum grammar.
///
/// Groupings mirror the EBNF reserved-keyword categories so that
/// the file is easy to cross-reference with the grammar document.
/// Each group is separated by a blank line and a comment header.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {

    // ── Section keywords ────────────────────────────────────────
    Config,
    Define,
    Create,
    Declare,
    Function,
    Run,

    // ── Control flow ────────────────────────────────────────────
    Loop,
    Every,
    For,
    In,
    Range,
    If,
    Else,
    Break,
    Continue,

    // ── Function / expression ────────────────────────────────────
    Return,
    Call,

    // ── I/O commands ────────────────────────────────────────────
    Delay,
    Print,
    Turn,
    Set,
    Toggle,
    Read,
    ReadPercent,    // READ_PERCENT — scanned as a single keyword token

    // ── Device / pin syntax ──────────────────────────────────────
    On,
    Pin,
    Pull,
    Up,
    Down,
    Init,
    As,
    Constant,

    // ── Ownership keywords ───────────────────────────────────────
    Give,
    Lend,
    Borrow,

    // ── Literals (keyword-form) ──────────────────────────────────
    High,
    Low,
    True,
    False,

    // ── Operators (keyword-form) ─────────────────────────────────
    Is,
    Not,
    And,
    Or,

    // ── Interface types ──────────────────────────────────────────
    Input,
    Output,
    AnalogInput,    // ANALOG_INPUT — underscore included, scanned as one keyword
    Pwm,
    Display,
    Pulse,

    // ── Qualifiers ───────────────────────────────────────────────
    Brightness,
    Speed,
    Angle,
    Red,
    Green,
    Blue,
    Lcd,
    Oled,
    Trigger,
    Echo,
    Enable,

    // ── Built-in type names ──────────────────────────────────────
    TyInteger,      // INTEGER  — prefixed Ty to avoid clash with Rust's Integer
    TyDecimal,      // DECIMAL
    TyPercentage,   // PERCENTAGE
    TyBoolean,      // BOOLEAN
    TyString,       // STRING
    TyByte,         // BYTE

    // ── Literal value tokens ─────────────────────────────────────

    /// Integer literal: decimal digits, e.g. `0`, `42`, `1023`
    IntLit(i64),

    /// Hexadecimal literal: `0xFF`, `0x27`, `0XFF`
    /// Stored as the parsed integer value.
    HexLit(u64),

    /// Decimal (floating point) literal: `3.14`, `0.5`, `100.0`
    DecimalLit(f64),

    /// Clock speed literal: `64MHZ` → stored as MHz value
    ClockLit(u32),

    /// Duration suffix tokens — only emitted immediately after an
    /// integer literal inside a duration context.
    /// The parser consumes IntLit + Ms/S together as a Duration.
    Ms,             // "ms"
    S,              // "s"  — bare 's' suffix after integer

    /// String literal: content between double quotes, verbatim.
    /// Escape sequences `\"`, `\\`, `\n` are processed by the lexer.
    StringLit(String),

    /// User-defined identifier.
    /// Carries original spelling and lowercase key.
    Ident(Ident),

    // ── Punctuation ──────────────────────────────────────────────
    LBrace,         // {
    RBrace,         // }
    LBracket,       // [
    RBracket,       // ]
    LParen,         // (
    RParen,         // )
    Comma,          // ,
    Colon,          // :
    Eq,             // =   (assignment and CONFIG =)
    EqEq,           // ==  (equality comparison)
    NotEq,          // !=
    Gt,             // >
    Lt,             // <
    GtEq,           // >=
    LtEq,           // <=
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /

    // ── Meta tokens ──────────────────────────────────────────────

    /// End of file.
    Eof,

    /// Produced when the lexer encounters a character it cannot
    /// classify. Carries the offending character for diagnostics.
    /// The lexer always continues after emitting this — it never
    /// panics — so the parser can collect multiple errors.
    Unknown(char),
}

impl Token {
    /// Returns true if this token is a keyword (not an identifier,
    /// literal, punctuation, or meta token).
    /// Used by the lexer to prevent keywords being used as names.
    pub fn is_keyword(&self) -> bool {
        matches!(self,
            Token::Config    | Token::Define    | Token::Create    |
            Token::Declare   | Token::Function  | Token::Run       |
            Token::Loop      | Token::Every     | Token::For       |
            Token::In        | Token::Range     | Token::If        |
            Token::Else      | Token::Break     | Token::Continue  |
            Token::Return    | Token::Call      | Token::Delay     |
            Token::Print     | Token::Turn      | Token::Set       |
            Token::Toggle    | Token::Read      | Token::ReadPercent |
            Token::On        | Token::Pin       | Token::Pull      |
            Token::Up        | Token::Down      | Token::Init      |
            Token::As        | Token::Constant  | Token::Give      |
            Token::Lend      | Token::Borrow    | Token::High      |
            Token::Low       | Token::True      | Token::False     |
            Token::Is        | Token::Not       | Token::And       |
            Token::Or        | Token::Input     | Token::Output    |
            Token::AnalogInput | Token::Pwm     | Token::Display   |
            Token::Pulse     | Token::Brightness | Token::Speed    |
            Token::Angle     | Token::Red       | Token::Green     |
            Token::Blue      | Token::Lcd       | Token::Oled      |
            Token::Trigger   | Token::Echo      | Token::Enable    |
            Token::TyInteger | Token::TyDecimal | Token::TyPercentage |
            Token::TyBoolean | Token::TyString  | Token::TyByte
        )
    }

    /// Returns a human-readable description of this token.
    /// Used in parser error messages: "expected '{', found <end of file>"
    pub fn describe(&self) -> String {
        match self {
            // Section keywords
            Token::Config       => "'CONFIG'".into(),
            Token::Define       => "'DEFINE'".into(),
            Token::Create       => "'CREATE'".into(),
            Token::Declare      => "'DECLARE'".into(),
            Token::Function     => "'FUNCTION'".into(),
            Token::Run          => "'RUN'".into(),
            // Control flow
            Token::Loop         => "'LOOP'".into(),
            Token::Every        => "'EVERY'".into(),
            Token::For          => "'FOR'".into(),
            Token::In           => "'IN'".into(),
            Token::Range        => "'RANGE'".into(),
            Token::If           => "'IF'".into(),
            Token::Else         => "'ELSE'".into(),
            Token::Break        => "'BREAK'".into(),
            Token::Continue     => "'CONTINUE'".into(),
            // Function / expression
            Token::Return       => "'RETURN'".into(),
            Token::Call         => "'CALL'".into(),
            // I/O
            Token::Delay        => "'DELAY'".into(),
            Token::Print        => "'PRINT'".into(),
            Token::Turn         => "'TURN'".into(),
            Token::Set          => "'SET'".into(),
            Token::Toggle       => "'TOGGLE'".into(),
            Token::Read         => "'READ'".into(),
            Token::ReadPercent  => "'READ_PERCENT'".into(),
            // Device / pin
            Token::On           => "'ON'".into(),
            Token::Pin          => "'PIN'".into(),
            Token::Pull         => "'PULL'".into(),
            Token::Up           => "'UP'".into(),
            Token::Down         => "'DOWN'".into(),
            Token::Init         => "'INIT'".into(),
            Token::As           => "'AS'".into(),
            Token::Constant     => "'CONSTANT'".into(),
            // Ownership
            Token::Give         => "'GIVE'".into(),
            Token::Lend         => "'LEND'".into(),
            Token::Borrow       => "'BORROW'".into(),
            // Literals (keyword-form)
            Token::High         => "'HIGH'".into(),
            Token::Low          => "'LOW'".into(),
            Token::True         => "'TRUE'".into(),
            Token::False        => "'FALSE'".into(),
            // Operators (keyword-form)
            Token::Is           => "'IS'".into(),
            Token::Not          => "'NOT'".into(),
            Token::And          => "'AND'".into(),
            Token::Or           => "'OR'".into(),
            // Interface types
            Token::Input        => "'INPUT'".into(),
            Token::Output       => "'OUTPUT'".into(),
            Token::AnalogInput  => "'ANALOG_INPUT'".into(),
            Token::Pwm          => "'PWM'".into(),
            Token::Display      => "'DISPLAY'".into(),
            Token::Pulse        => "'PULSE'".into(),
            // Qualifiers
            Token::Brightness   => "'BRIGHTNESS'".into(),
            Token::Speed        => "'SPEED'".into(),
            Token::Angle        => "'ANGLE'".into(),
            Token::Red          => "'RED'".into(),
            Token::Green        => "'GREEN'".into(),
            Token::Blue         => "'BLUE'".into(),
            Token::Lcd          => "'LCD'".into(),
            Token::Oled         => "'OLED'".into(),
            Token::Trigger      => "'TRIGGER'".into(),
            Token::Echo         => "'ECHO'".into(),
            Token::Enable       => "'ENABLE'".into(),
            // Type names
            Token::TyInteger    => "'INTEGER'".into(),
            Token::TyDecimal    => "'DECIMAL'".into(),
            Token::TyPercentage => "'PERCENTAGE'".into(),
            Token::TyBoolean    => "'BOOLEAN'".into(),
            Token::TyString     => "'STRING'".into(),
            Token::TyByte       => "'BYTE'".into(),
            // Literal values
            Token::IntLit(n)    => format!("integer '{}'", n),
            Token::HexLit(n)    => format!("hex literal '0x{:X}'", n),
            Token::DecimalLit(f)=> format!("decimal '{}'", f),
            Token::ClockLit(n)  => format!("clock speed '{}MHZ'", n),
            Token::Ms           => "'ms'".into(),
            Token::S            => "'s'".into(),
            Token::StringLit(s) => format!("string \"{}\"", s),
            Token::Ident(i)     => format!("identifier '{}'", i.original),
            // Punctuation
            Token::LBrace       => "'{'".into(),
            Token::RBrace       => "'}'".into(),
            Token::LBracket     => "'['".into(),
            Token::RBracket     => "']'".into(),
            Token::LParen       => "'('".into(),
            Token::RParen       => "')'".into(),
            Token::Comma        => "','".into(),
            Token::Colon        => "':'".into(),
            Token::Eq           => "'='".into(),
            Token::EqEq         => "'=='".into(),
            Token::NotEq        => "'!='".into(),
            Token::Gt           => "'>'".into(),
            Token::Lt           => "'<'".into(),
            Token::GtEq         => "'>='".into(),
            Token::LtEq         => "'<='".into(),
            Token::Plus         => "'+'".into(),
            Token::Minus        => "'-'".into(),
            Token::Star         => "'*'".into(),
            Token::Slash        => "'/'".into(),
            // Meta
            Token::Eof          => "end of file".into(),
            Token::Unknown(c)   => format!("unexpected character '{}'", c),
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.describe())
    }
}

// ----------------------------------------------------------------
// Keyword table
// ----------------------------------------------------------------

/// Maps a normalised (lowercase) string to its keyword Token.
///
/// The lexer calls `keyword_from_str` after lowercasing a scanned
/// word. If it returns `Some`, a keyword token is emitted.
/// If it returns `None`, an `Ident` token is emitted instead.
///
/// `READ_PERCENT` and `ANALOG_INPUT` contain underscores.
/// The lexer scans them as a single word by including `_` in the
/// identifier character set, then looks them up here.
pub fn keyword_from_str(s: &str) -> Option<Token> {
    match s {
        // Section keywords
        "config"        => Some(Token::Config),
        "define"        => Some(Token::Define),
        "create"        => Some(Token::Create),
        "declare"       => Some(Token::Declare),
        "function"      => Some(Token::Function),
        "run"           => Some(Token::Run),
        // Control flow
        "loop"          => Some(Token::Loop),
        "every"         => Some(Token::Every),
        "for"           => Some(Token::For),
        "in"            => Some(Token::In),
        "range"         => Some(Token::Range),
        "if"            => Some(Token::If),
        "else"          => Some(Token::Else),
        "break"         => Some(Token::Break),
        "continue"      => Some(Token::Continue),
        // Function / expression
        "return"        => Some(Token::Return),
        "call"          => Some(Token::Call),
        // I/O commands
        "delay"         => Some(Token::Delay),
        "print"         => Some(Token::Print),
        "turn"          => Some(Token::Turn),
        "set"           => Some(Token::Set),
        "toggle"        => Some(Token::Toggle),
        "read"          => Some(Token::Read),
        "read_percent"  => Some(Token::ReadPercent),
        // Device / pin syntax
        "on"            => Some(Token::On),
        "pin"           => Some(Token::Pin),
        "pull"          => Some(Token::Pull),
        "up"            => Some(Token::Up),
        "down"          => Some(Token::Down),
        "init"          => Some(Token::Init),
        "as"            => Some(Token::As),
        "constant"      => Some(Token::Constant),
        // Ownership keywords
        "give"          => Some(Token::Give),
        "lend"          => Some(Token::Lend),
        "borrow"        => Some(Token::Borrow),
        // Literals (keyword-form)
        "high"          => Some(Token::High),
        "low"           => Some(Token::Low),
        "true"          => Some(Token::True),
        "false"         => Some(Token::False),
        // Operators (keyword-form)
        "is"            => Some(Token::Is),
        "not"           => Some(Token::Not),
        "and"           => Some(Token::And),
        "or"            => Some(Token::Or),
        // Interface types
        "input"         => Some(Token::Input),
        "output"        => Some(Token::Output),
        "analog_input"  => Some(Token::AnalogInput),
        "pwm"           => Some(Token::Pwm),
        "display"       => Some(Token::Display),
        "pulse"         => Some(Token::Pulse),
        // Qualifiers
        "brightness"    => Some(Token::Brightness),
        "speed"         => Some(Token::Speed),
        "angle"         => Some(Token::Angle),
        "red"           => Some(Token::Red),
        "green"         => Some(Token::Green),
        "blue"          => Some(Token::Blue),
        "lcd"           => Some(Token::Lcd),
        "oled"          => Some(Token::Oled),
        "trigger"       => Some(Token::Trigger),
        "echo"          => Some(Token::Echo),
        "enable"        => Some(Token::Enable),
        // Built-in type names
        "integer"       => Some(Token::TyInteger),
        "decimal"       => Some(Token::TyDecimal),
        "percentage"    => Some(Token::TyPercentage),
        "boolean"       => Some(Token::TyBoolean),
        "string"        => Some(Token::TyString),
        "byte"          => Some(Token::TyByte),
        // Not a keyword
        _               => None,
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_span() -> Span {
        Span::new(std::sync::Arc::new("<test>".into()), 1, 1, 1)
    }

    // ── keyword_from_str ────────────────────────────────────────

    #[test]
    fn keywords_resolve_from_lowercase() {
        assert_eq!(keyword_from_str("config"),   Some(Token::Config));
        assert_eq!(keyword_from_str("run"),      Some(Token::Run));
        assert_eq!(keyword_from_str("give"),     Some(Token::Give));
        assert_eq!(keyword_from_str("borrow"),   Some(Token::Borrow));
        assert_eq!(keyword_from_str("analog_input"), Some(Token::AnalogInput));
        assert_eq!(keyword_from_str("read_percent"), Some(Token::ReadPercent));
    }

    #[test]
    fn unknown_word_returns_none() {
        assert_eq!(keyword_from_str("moisture"),  None);
        assert_eq!(keyword_from_str("status_led"), None);
        assert_eq!(keyword_from_str(""),           None);
    }

    #[test]
    fn keyword_table_is_exhaustive_for_type_names() {
        // Every data type name must resolve to a Ty* token.
        assert_eq!(keyword_from_str("integer"),    Some(Token::TyInteger));
        assert_eq!(keyword_from_str("decimal"),    Some(Token::TyDecimal));
        assert_eq!(keyword_from_str("percentage"), Some(Token::TyPercentage));
        assert_eq!(keyword_from_str("boolean"),    Some(Token::TyBoolean));
        assert_eq!(keyword_from_str("string"),     Some(Token::TyString));
        assert_eq!(keyword_from_str("byte"),       Some(Token::TyByte));
    }

    // ── Ident case policy ───────────────────────────────────────

    #[test]
    fn ident_equality_is_case_insensitive() {
        let a = Ident::new("Button",  dummy_span());
        let b = Ident::new("button",  dummy_span());
        let c = Ident::new("BUTTON",  dummy_span());
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a, c);
    }

    #[test]
    fn ident_preserves_original_spelling() {
        let a = Ident::new("StatusLed", dummy_span());
        assert_eq!(a.original, "StatusLed");
        assert_eq!(a.key,      "statusled");
    }

    #[test]
    fn ident_hash_consistent_with_equality() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Ident::new("Led",  dummy_span()));
        // Inserting the same name with different casing should not
        // add a second entry.
        set.insert(Ident::new("led",  dummy_span()));
        set.insert(Ident::new("LED",  dummy_span()));
        assert_eq!(set.len(), 1);
    }

    // ── Span ────────────────────────────────────────────────────

    #[test]
    fn span_merge_covers_both_ends() {
        let file: std::sync::Arc<String> = std::sync::Arc::new("test.fe".into());
        let a = Span::new(file.clone(), 1, 1, 5);   // cols 1-5
        let b = Span::new(file.clone(), 1, 8, 4);   // cols 8-11
        let merged = a.to(&b);
        assert_eq!(merged.column, 1);
        assert_eq!(merged.length, 11); // 8 + 4 - 1 = 11
    }

    // ── Token::describe ─────────────────────────────────────────

    #[test]
    fn describe_produces_readable_strings() {
        assert_eq!(Token::LBrace.describe(),         "'{'");
        assert_eq!(Token::Eof.describe(),            "end of file");
        assert_eq!(Token::IntLit(42).describe(),     "integer '42'");
        assert_eq!(Token::Unknown('£').describe(),   "unexpected character '£'");
    }

    #[test]
    fn is_keyword_returns_true_for_all_reserved_words() {
        // Spot-check a representative sample from every category.
        assert!(Token::Config.is_keyword());
        assert!(Token::Give.is_keyword());
        assert!(Token::AnalogInput.is_keyword());
        assert!(Token::TyPercentage.is_keyword());
        assert!(Token::Brightness.is_keyword());
        assert!(Token::ReadPercent.is_keyword());
    }

    #[test]
    fn is_keyword_returns_false_for_non_keywords() {
        assert!(!Token::Ident(Ident::new("moisture", dummy_span())).is_keyword());
        assert!(!Token::IntLit(0).is_keyword());
        assert!(!Token::StringLit("hello".into()).is_keyword());
        assert!(!Token::LBrace.is_keyword());
        assert!(!Token::Eof.is_keyword());
    }
}
