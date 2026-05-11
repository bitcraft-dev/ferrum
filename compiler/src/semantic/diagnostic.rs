// ferrum/compiler/src/semantic/diagnostic.rs
//
// Unified diagnostic type for the semantic pass.
//
// Every compile-time error and warning in the spec (§18) maps to
// exactly one DiagnosticKind variant. The variant carries the
// structured data needed to format the full error message —
// name, expected type, found type, location, etc.
//
// The `message` field on Diagnostic is a pre-formatted human-readable
// string for the cases where a simple format is sufficient.
// The `kind` field carries the structured data for cases where
// the reporter wants to render a richer multi-line message.

use crate::lexer::token::Span;

// ----------------------------------------------------------------
// Severity
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error   => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
        }
    }
}

// ----------------------------------------------------------------
// Diagnostic
// ----------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity:   Severity,
    pub kind:       DiagnosticKind,
    pub span:       Span,
    /// Pre-formatted message — always present.
    pub message:    String,
    /// Optional one-line fix suggestion — shown after the message.
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn is_warning(&self) -> bool {
        self.severity == Severity::Warning
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.span, self.severity, self.message)?;
        if let Some(s) = &self.suggestion {
            write!(f, "\n  Suggestion: {}", s)?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------------
// DiagnosticKind — structured data behind every diagnostic
// ----------------------------------------------------------------

/// Every compile-time error and warning from §18 of the spec.
/// Variants are named after the condition they report, not the
/// message text, so the reporter can render them in any format.
#[derive(Debug, Clone)]
pub enum DiagnosticKind {

    // ── §18.1 Compile-time errors ─────────────────────────────────

    /// Invalid qualifier 'Q' for interface 'I'
    InvalidQualifier {
        qualifier:        String,
        interface:        String,
        valid_interfaces: Vec<String>,
    },

    /// Missing pin assignment for interface 'I' on device 'D'
    MissingPinAssignment {
        interface: String,
        device:    String,
    },

    /// Duplicate pin assignment: PIN N already used by 'D'
    DuplicatePinAssignment {
        pin:              u32,
        first_used_by:    String,
        first_used_at:    Span,
    },

    /// INIT is not permitted on INPUT / ANALOG_INPUT / PULSE interfaces
    InitOnReadOnlyInterface {
        interface: String,
    },

    /// PULL is only valid on INPUT interfaces
    PullOnNonInput {
        interface: String,
    },

    /// Partial INIT not permitted for device 'D'
    PartialInit {
        device:    String,
        expected:  usize,
        found:     usize,
    },

    /// Type mismatch: expected T, found V
    TypeMismatch {
        expected: String,
        found:    String,
        context:  String,
    },

    /// Value V is out of range for type T (valid: range)
    ValueOutOfRange {
        value:     String,
        ty:        String,
        valid_min: String,
        valid_max: String,
    },

    /// Undefined variable 'N'
    UndefinedVariable {
        name: String,
    },

    /// Undefined function 'N'
    UndefinedFunction {
        name: String,
    },

    /// 'N' is not a variable — did you mean: CALL N?
    BareCallWithoutKeyword {
        name: String,
    },

    /// Cannot assign void function 'N' to variable
    AssignFromVoidFunction {
        name: String,
    },

    /// Ambiguous SET target for 'D' — multiple numeric interfaces
    AmbiguousSetTarget {
        device:     String,
        interfaces: Vec<String>,
    },

    /// Invalid qualifier 'Q' — device 'D' does not have this interface
    QualifierNotOnDevice {
        qualifier: String,
        device:    String,
    },

    /// Function 'N' has conflicting return types (T1 and T2)
    ConflictingReturnTypes {
        function: String,
        type_a:   String,
        type_b:   String,
        site_a:   Span,
        site_b:   Span,
    },

    /// 'EVERY' is not valid inside a LOOP or FOR block
    /// (This is caught structurally by the parser; kept here for
    /// completeness in case the semantic pass needs to re-check.)
    EveryInsideLoop,

    /// 'N' is not accessible here — declared inside block at line L
    OutOfScopeVariable {
        name:         String,
        declared_at:  Span,
        scope_label:  String,
    },

    /// READ_PERCENT is not valid for INPUT device 'D'
    ReadPercentOnDigitalInput {
        device: String,
    },

    /// 'IS' used on non-device, non-Boolean expression
    IsOnNonDeviceOrBoolean {
        expression_type: String,
    },

    /// '==' used on a device interface — use IS
    EqEqOnDevice {
        device: String,
    },

    /// Cannot assign to constant 'N'
    AssignToConstant {
        name: String,
    },

    /// Inline IF missing ELSE branch
    InlineIfMissingElse,

    /// Both branches of inline IF must return the same type
    InlineIfTypeMismatch {
        then_type: String,
        else_type: String,
    },

    /// Device parameter 'N' is missing an ownership keyword
    MissingOwnershipKeyword {
        param: String,
    },

    /// 'D' was given to 'F' and ownership did not return
    OwnershipViolation {
        device: String,
    },

    /// Cannot write to 'N' — declared LEND (read-only borrow)
    WriteToLendParam {
        param: String,
    },

    /// Ownership mismatch: 'F' expects KEYWORD for 'N', found KEYWORD
    OwnershipMismatch {
        function:  String,
        param:     String,
        expected:  String,
        found:     String,
    },

    /// Conflicting access to 'D' in EVERY and LOOP blocks
    ScheduledDeviceConflict {
        device:    String,
        every_at:  Span,
        loop_at:   Span,
    },

    /// Conflicting BORROW of 'D' in EVERY and LOOP blocks
    ScheduledBorrowConflict {
        device:   String,
        every_at: Span,
        loop_at:  Span,
    },

    /// 'D' cannot be passed twice in the same call
    DevicePassedTwice {
        device: String,
    },

    /// Section order violation: 'S' must appear after 'S2'
    SectionOrderViolation {
        found:    String,
        expected: String,
    },

    /// Inline DEFINE accepts only one definition
    InlineDefineMultiple,

    /// 'BREAK' is not valid here — no enclosing LOOP or FOR block
    /// (Also caught by the parser; kept for semantic re-check.)
    BreakOutsideLoop,

    /// 'CONTINUE' is not valid here — no enclosing LOOP or FOR block
    ContinueOutsideLoop,

    /// RANGE bounds must be Integer
    RangeBoundsNotInteger {
        found: String,
    },

    /// RANGE start must be <= end
    RangeDirectionError {
        from: i64,
        to:   i64,
    },

    /// Duplicate definition of 'N' in the same scope
    DuplicateDefinition {
        name:          String,
        first_defined: Span,
    },

    // ── §18.2 Warnings ────────────────────────────────────────────

    /// Unused variable 'N'
    UnusedVariable {
        name: String,
    },

    /// DELAY Nms inside EVERY Nms may cause timing violations
    DelayExceedsEveryPeriod {
        delay_ms:  u64,
        period_ms: u64,
    },

    // ── CONFIG-specific errors ────────────────────────────────────

    /// Unknown CONFIG key
    UnknownConfigKey {
        key: String,
    },

    /// Wrong value type for a CONFIG key
    ConfigTypeMismatch {
        key:      String,
        expected: String,
        found:    String,
    },
}

// ----------------------------------------------------------------
// Diagnostic builder helpers
// ----------------------------------------------------------------

/// Convenience constructors so the semantic checkers don't have to
/// repeat long field initialisation everywhere.
impl Diagnostic {
    pub fn error(
        kind:       DiagnosticKind,
        span:       Span,
        message:    impl Into<String>,
        suggestion: Option<String>,
    ) -> Self {
        Diagnostic {
            severity:   Severity::Error,
            kind,
            span,
            message:    message.into(),
            suggestion,
        }
    }

    pub fn warning(
        kind:       DiagnosticKind,
        span:       Span,
        message:    impl Into<String>,
        suggestion: Option<String>,
    ) -> Self {
        Diagnostic {
            severity:   Severity::Warning,
            kind,
            span,
            message:    message.into(),
            suggestion,
        }
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn span() -> Span {
        Span::new(Arc::new("test.fe".into()), 1, 1, 1)
    }

    #[test]
    fn diagnostic_display_includes_message() {
        let d = Diagnostic::error(
            DiagnosticKind::UndefinedVariable { name: "moisture".into() },
            span(),
            "'moisture' is not defined",
            Some("Did you mean 'moisturee'?".into()),
        );
        let s = d.to_string();
        assert!(s.contains("moisture"));
        assert!(s.contains("error"));
        assert!(s.contains("Suggestion"));
    }

    #[test]
    fn warning_has_correct_severity() {
        let d = Diagnostic::warning(
            DiagnosticKind::UnusedVariable { name: "count".into() },
            span(),
            "'count' is declared but never used",
            None,
        );
        assert!(d.is_warning());
        assert!(!d.is_error());
    }

    #[test]
    fn error_has_correct_severity() {
        let d = Diagnostic::error(
            DiagnosticKind::AssignToConstant { name: "MAX".into() },
            span(),
            "cannot assign to constant 'MAX'",
            None,
        );
        assert!(d.is_error());
        assert!(!d.is_warning());
    }
}