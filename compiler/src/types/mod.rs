/// Source location of any node or token.
/// Used in every error message.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub file:   String,   // filename — useful when multi-file projects arrive
    pub line:   u32,
    pub column: u32,
    pub length: u32,
}

impl Span {
    /// Merge two spans into one covering both (for compound nodes).
    pub fn to(&self, other: &Span) -> Span {
        Span {
            file:   self.file.clone(),
            line:   self.line,
            column: self.column,
            length: (other.column + other.length).saturating_sub(self.column),
        }
    }
}

/// A user-defined name — case-preserving but case-insensitive.
/// `original` is shown in error messages.
/// `key` (lowercase) is used for all symbol table lookups.
#[derive(Debug, Clone, PartialEq)]
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

/// Equality is always on the normalised key.
impl std::hash::Hash for Ident {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

/// Every resolved type in the language.
/// The semantic pass fills Option<Type> slots on expression nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    // Data types
    Integer,
    Decimal,
    Percentage,
    Boolean,
    String,
    Byte,

    // Array types — element type + fixed size
    Array(Box<Type>, usize),

    // Hardware value types (returned by READ / READ_PERCENT)
    PinState,          // HIGH | LOW  — returned by READ on INPUT
    AnalogRaw,         // 0–1023      — returned by READ on ANALOG_INPUT

    // Device type — named, references a DEFINE entry
    Device(Ident),

    // Void — functions with no RETURN
    Void,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ownership {
    Give,
    Lend,
    Borrow,
}