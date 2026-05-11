// Diagnostic reporter — formats and renders compiler diagnostics.
//
// RESPONSIBILITIES
//   Takes a Vec<Diagnostic> and the original source text, and
//   renders them as human-readable terminal output.
//
// OUTPUT FORMAT
//   error[E042]: 'status_led' is not available here
//    --> src/main.fe:12:5
//    12 │     TURN status_led HIGH
//              ^^^^^^^^^^^ 'status_led' was given to 'run_blink' at line 8
//   Suggestion: Use BORROW instead if the caller needs to keep the device.
//
// COLOUR
//   Uses ANSI escape codes when the output is a TTY.
//   Suppressed when NO_COLOR env var is set or output is redirected.

use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind, Severity};

// ----------------------------------------------------------------
// Colour support
// ----------------------------------------------------------------

struct Colors {
    red:    &'static str,
    yellow: &'static str,
    cyan:   &'static str,
    bold:   &'static str,
    reset:  &'static str,
    dim:    &'static str,
}

impl Colors {
    fn enabled() -> Self {
        Colors {
            red:    "\x1b[31m",
            yellow: "\x1b[33m",
            cyan:   "\x1b[36m",
            bold:   "\x1b[1m",
            reset:  "\x1b[0m",
            dim:    "\x1b[2m",
        }
    }

    fn disabled() -> Self {
        Colors {
            red: "", yellow: "", cyan: "", bold: "", reset: "", dim: "",
        }
    }

    fn is_tty_and_color_enabled() -> bool {
        std::env::var("NO_COLOR").is_err() && atty_check()
    }

    fn for_output() -> Self {
        if Self::is_tty_and_color_enabled() {
            Self::enabled()
        } else {
            Self::disabled()
        }
    }
}

fn atty_check() -> bool {
    // Simple check: if TERM is "dumb" or unset, disable colour.
    match std::env::var("TERM") {
        Ok(t) if t == "dumb" => false,
        Err(_)               => false,
        _                    => true,
    }
}

// ----------------------------------------------------------------
// Reporter
// ----------------------------------------------------------------

pub struct Reporter {
    source_lines: Vec<String>,
    filename:     String,
    colors:       Colors,
}

impl Reporter {
    pub fn new(source: &str, filename: impl Into<String>) -> Self {
        Reporter {
            source_lines: source.lines().map(|l| l.to_string()).collect(),
            filename:     filename.into(),
            colors:       Colors::for_output(),
        }
    }

    /// Render all diagnostics to a String.
    pub fn render(&self, diagnostics: &[Diagnostic]) -> String {
        let mut out = String::new();
        for d in diagnostics {
            out.push_str(&self.render_one(d));
            out.push('\n');
        }
        // Summary line
        let errors   = diagnostics.iter().filter(|d| d.is_error()).count();
        let warnings = diagnostics.iter().filter(|d| d.is_warning()).count();
        if errors > 0 || warnings > 0 {
            out.push_str(&self.render_summary(errors, warnings));
        }
        out
    }

    fn render_one(&self, d: &Diagnostic) -> String {
        let c = &self.colors;
        let severity_color = match d.severity {
            Severity::Error   => c.red,
            Severity::Warning => c.yellow,
        };
        let severity_label = match d.severity {
            Severity::Error   => "error",
            Severity::Warning => "warning",
        };
        let code = diagnostic_code(&d.kind);

        let mut out = format!(
            "{}{}{}[{}]{}: {}{}\n",
            c.bold, severity_color, severity_label, code, c.reset,
            c.bold, d.message,
        );
        out.push_str(&format!("{}{}", c.reset, ""));

        // Location line: --> filename:line:col
        out.push_str(&format!(
            " {}-->{} {}{}:{}{}\n",
            c.cyan, c.reset,
            c.dim, self.filename, d.span, c.reset,
        ));

        // Source line with caret
        let line_num = d.span.line as usize;
        if line_num > 0 && line_num <= self.source_lines.len() {
            let line_text = &self.source_lines[line_num - 1];
            let col = d.span.column.saturating_sub(1) as usize;
            let len = (d.span.length as usize).max(1);

            out.push_str(&format!(
                " {:>4} {} {}\n",
                line_num,
                format!("{}│{}", c.cyan, c.reset),
                line_text
            ));
            let caret: String = " ".repeat(col) + &"^".repeat(len);
            out.push_str(&format!(
                "      {} {}{}{}\n",
                format!("{}│{}", c.cyan, c.reset),
                c.red, caret, c.reset,
            ));
        }

        // Suggestion
        if let Some(suggestion) = &d.suggestion {
            for line in suggestion.lines() {
                out.push_str(&format!(
                    "  {}Suggestion:{} {}\n",
                    c.cyan, c.reset, line
                ));
            }
        }

        out
    }

    fn render_summary(&self, errors: usize, warnings: usize) -> String {
        let c = &self.colors;
        let mut parts = Vec::new();
        if errors > 0 {
            parts.push(format!(
                "{}{}{} error{}{}",
                c.bold, c.red, errors,
                if errors == 1 { "" } else { "s" },
                c.reset
            ));
        }
        if warnings > 0 {
            parts.push(format!(
                "{}{}{} warning{}{}",
                c.bold, c.yellow, warnings,
                if warnings == 1 { "" } else { "s" },
                c.reset
            ));
        }
        format!("aborting due to {}\n", parts.join(" and "))
    }
}

// ----------------------------------------------------------------
// Error code table
// ----------------------------------------------------------------

/// Returns a short alphanumeric code for each diagnostic kind.
/// Shown as error[E001] in the output — easy to look up in docs.
fn diagnostic_code(kind: &DiagnosticKind) -> &'static str {
    match kind {
        DiagnosticKind::InvalidQualifier { .. }          => "E001",
        DiagnosticKind::MissingPinAssignment { .. }      => "E002",
        DiagnosticKind::DuplicatePinAssignment { .. }    => "E003",
        DiagnosticKind::InitOnReadOnlyInterface { .. }   => "E004",
        DiagnosticKind::PullOnNonInput { .. }            => "E005",
        DiagnosticKind::PartialInit { .. }               => "E006",
        DiagnosticKind::TypeMismatch { .. }              => "E007",
        DiagnosticKind::ValueOutOfRange { .. }           => "E008",
        DiagnosticKind::UndefinedVariable { .. }         => "E009",
        DiagnosticKind::UndefinedFunction { .. }         => "E010",
        DiagnosticKind::BareCallWithoutKeyword { .. }    => "E011",
        DiagnosticKind::AssignFromVoidFunction { .. }    => "E012",
        DiagnosticKind::AmbiguousSetTarget { .. }        => "E013",
        DiagnosticKind::QualifierNotOnDevice { .. }      => "E014",
        DiagnosticKind::ConflictingReturnTypes { .. }    => "E015",
        DiagnosticKind::EveryInsideLoop                  => "E016",
        DiagnosticKind::OutOfScopeVariable { .. }        => "E017",
        DiagnosticKind::ReadPercentOnDigitalInput { .. } => "E018",
        DiagnosticKind::IsOnNonDeviceOrBoolean { .. }    => "E019",
        DiagnosticKind::EqEqOnDevice { .. }              => "E020",
        DiagnosticKind::AssignToConstant { .. }          => "E021",
        DiagnosticKind::InlineIfMissingElse              => "E022",
        DiagnosticKind::InlineIfTypeMismatch { .. }      => "E023",
        DiagnosticKind::MissingOwnershipKeyword { .. }   => "E024",
        DiagnosticKind::OwnershipViolation { .. }        => "E025",
        DiagnosticKind::WriteToLendParam { .. }          => "E026",
        DiagnosticKind::OwnershipMismatch { .. }         => "E027",
        DiagnosticKind::ScheduledDeviceConflict { .. }   => "E028",
        DiagnosticKind::ScheduledBorrowConflict { .. }   => "E029",
        DiagnosticKind::DevicePassedTwice { .. }         => "E030",
        DiagnosticKind::SectionOrderViolation { .. }     => "E031",
        DiagnosticKind::InlineDefineMultiple             => "E032",
        DiagnosticKind::BreakOutsideLoop                 => "E033",
        DiagnosticKind::ContinueOutsideLoop              => "E034",
        DiagnosticKind::RangeBoundsNotInteger { .. }     => "E035",
        DiagnosticKind::RangeDirectionError { .. }       => "E036",
        DiagnosticKind::DuplicateDefinition { .. }       => "E037",
        DiagnosticKind::UnknownConfigKey { .. }          => "E038",
        DiagnosticKind::ConfigTypeMismatch { .. }        => "E039",
        // Warnings
        DiagnosticKind::UnusedVariable { .. }            => "W001",
        DiagnosticKind::DelayExceedsEveryPeriod { .. }   => "W002",
    }
}

#[cfg(test)]
mod reporter_tests {
    use super::*;
    use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind};
    use crate::lexer::token::Span;
    use std::sync::Arc;

    fn span(line: u32, col: u32, len: u32) -> Span {
        Span::new(Arc::new("test.fe".into()), line, col, len)
    }

    #[test]
    fn render_contains_error_message() {
        let source = "DECLARE Percentage level INIT 105.0";
        let reporter = Reporter::new(source, "test.fe");
        let d = Diagnostic::error(
            DiagnosticKind::ValueOutOfRange {
                value:     "105.0".into(),
                ty:        "Percentage".into(),
                valid_min: "0.0".into(),
                valid_max: "100.0".into(),
            },
            span(1, 29, 5),
            "Value 105.0 is out of range for type Percentage (valid: 0.0–100.0).",
            Some("Use Decimal if unconstrained range is intended.".into()),
        );
        let output = reporter.render(&[d]);
        assert!(output.contains("105.0"));
        assert!(output.contains("E008"));
        assert!(output.contains("Suggestion"));
    }

    #[test]
    fn render_warning_uses_correct_label() {
        let source = "DECLARE Integer debug_count INIT 0";
        let reporter = Reporter::new(source, "test.fe");
        let d = Diagnostic::warning(
            DiagnosticKind::UnusedVariable { name: "debug_count".into() },
            span(1, 9, 11),
            "'debug_count' is declared but never used.",
            None,
        );
        let output = reporter.render(&[d]);
        assert!(output.contains("warning"));
        assert!(output.contains("W001"));
    }

    #[test]
    fn summary_shows_counts() {
        let source = "x";
        let reporter = Reporter::new(source, "test.fe");
        let diagnostics = vec![
            Diagnostic::error(
                DiagnosticKind::UndefinedVariable { name: "x".into() },
                span(1, 1, 1),
                "'x' is not defined",
                None,
            ),
            Diagnostic::warning(
                DiagnosticKind::UnusedVariable { name: "y".into() },
                span(1, 1, 1),
                "'y' is declared but never used.",
                None,
            ),
        ];
        let output = reporter.render(&diagnostics);
        assert!(output.contains("1 error"));
        assert!(output.contains("1 warning"));
    }
}