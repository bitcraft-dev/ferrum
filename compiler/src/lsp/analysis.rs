// Shared "run the pipeline on this text buffer" helper — used by
// diagnostics publishing, hover, and completion so all three always
// agree on what the document currently looks like.

use crate::ast::Program;
use crate::lexer::{self, token::SpannedToken};
use crate::parser;
use crate::semantic::{self, diagnostic::{Diagnostic as FeDiagnostic, DiagnosticKind}};
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// Result of analysing one document's text: every diagnostic
/// (lex + parse + semantic) converted to LSP form, plus the annotated
/// AST if parsing succeeded — semantic errors don't prevent this from
/// being `Some` (see `SemanticResult::program`'s doc comment), only a
/// parse error does, since there's no tree to hand back at all then.
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub program:     Option<Program>,
    /// The full token stream, kept so hover can identify the keyword or
    /// identifier under the cursor even in spots the AST doesn't reach
    /// (e.g. a `TURN <device>` operand, which is a bare Ident, not an
    /// expression node).
    pub tokens:      Vec<SpannedToken>,
    /// Mechanical quick fixes derivable from the diagnostics — only the
    /// unambiguous ones (spelling corrections, a missing CALL keyword,
    /// `!` → `!=`). Surfaced to the client as `textDocument/codeAction`.
    pub fixes:       Vec<CodeFix>,
}

/// A single applicable quick fix: replace `edit_range` with `new_text`.
/// `diagnostic_range` is the span of the diagnostic that motivated the
/// fix, used to decide whether the fix is relevant to the range the
/// editor asked about.
pub struct CodeFix {
    pub title:            String,
    pub diagnostic_range: Range,
    pub edit_range:       Range,
    pub new_text:         String,
}

pub fn analyse_text(text: &str, uri_path: &str) -> Analysis {
    let mut diagnostics = Vec::new();
    let mut fixes = Vec::new();

    let lex_result = lexer::lex(text, uri_path);
    // Clone before the parser consumes the stream — hover needs the
    // tokens after parsing, and re-lexing would just repeat the work.
    let tokens = lex_result.tokens.clone();
    for e in &lex_result.errors {
        diagnostics.push(to_lsp_diagnostic(&e.span, &e.message, DiagnosticSeverity::ERROR));
        // `unexpected '!' — did you mean '!='?` is a mechanical fix.
        if e.message.contains("!=") {
            let r = span_to_range(&e.span);
            fixes.push(CodeFix {
                title:            "Change '!' to '!='".into(),
                diagnostic_range: r,
                edit_range:       r,
                new_text:         "!=".into(),
            });
        }
    }

    let parse_result = parser::parser::parse(lex_result.tokens, uri_path);
    for e in &parse_result.errors {
        let msg = match &e.suggestion {
            Some(s) => format!("{}\nSuggestion: {}", e.message, s),
            None    => e.message.clone(),
        };
        diagnostics.push(to_lsp_diagnostic(&e.span, &msg, DiagnosticSeverity::ERROR));
        if let Some(fix) = fix_from_suggestion(&e.span, e.suggestion.as_deref()) {
            fixes.push(fix);
        }
    }

    let program = match parse_result.program {
        Some(program) => {
            let result = semantic::analyse(program);
            for d in &result.diagnostics {
                diagnostics.push(fe_diagnostic_to_lsp(d));
                if let Some(fix) = fix_from_semantic(d) {
                    fixes.push(fix);
                }
            }
            result.program
        }
        // Parse failed outright — no tree for semantic analysis, hover,
        // or completion to work from until the syntax is valid again.
        None => None,
    };

    Analysis { diagnostics, program, tokens, fixes }
}

/// Derive a quick fix from a diagnostic's suggestion text when it names
/// a concrete replacement (the internal `Did you mean 'X'?` convention).
fn fix_from_suggestion(span: &crate::lexer::token::Span, suggestion: Option<&str>) -> Option<CodeFix> {
    let replacement = did_you_mean(suggestion?)?;
    let r = span_to_range(span);
    Some(CodeFix {
        title:            format!("Change to '{}'", replacement),
        diagnostic_range: r,
        edit_range:       r,
        new_text:         replacement,
    })
}

/// Derive a quick fix from a structured semantic diagnostic. Only the
/// cases where the edit is unambiguous produce a fix — currently the
/// spelling corrections, whose suggestion names the intended symbol.
fn fix_from_semantic(d: &FeDiagnostic) -> Option<CodeFix> {
    let r = span_to_range(&d.span);
    match &d.kind {
        DiagnosticKind::UndefinedVariable { .. }
        | DiagnosticKind::UndefinedFunction { .. } => {
            let replacement = did_you_mean(d.suggestion.as_deref()?)?;
            Some(CodeFix {
                title:            format!("Change to '{}'", replacement),
                diagnostic_range: r,
                edit_range:       r,
                new_text:         replacement,
            })
        }
        _ => None,
    }
}

/// Pull the replacement name out of a `Did you mean 'X'?` suggestion.
/// Returns `None` for prose suggestions that don't name a token.
fn did_you_mean(suggestion: &str) -> Option<String> {
    if !suggestion.to_lowercase().contains("did you mean") {
        return None;
    }
    let start = suggestion.find('\'')? + 1;
    let rest = &suggestion[start..];
    let end = rest.find('\'')?;
    let name = &rest[..end];
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn fe_diagnostic_to_lsp(d: &FeDiagnostic) -> Diagnostic {
    use crate::semantic::diagnostic::Severity;
    let severity = match d.severity {
        Severity::Error   => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };
    let msg = match &d.suggestion {
        Some(s) => format!("{}\nSuggestion: {}", d.message, s),
        None    => d.message.clone(),
    };
    to_lsp_diagnostic(&d.span, &msg, severity)
}

fn to_lsp_diagnostic(span: &crate::lexer::token::Span, message: &str, severity: DiagnosticSeverity) -> Diagnostic {
    Diagnostic {
        range: span_to_range(span),
        severity: Some(severity),
        source: Some("ferrum".into()),
        message: message.to_string(),
        ..Default::default()
    }
}

/// Ferrum spans are 1-based line/column with a byte length; LSP
/// positions are 0-based line/UTF-16-code-unit. Spans never carry
/// their own end line (see `Span::to`'s doc comment), so a span that
/// truly crosses lines renders as just its start position — an
/// acceptable approximation for a first LSP pass.
pub fn span_to_range(span: &crate::lexer::token::Span) -> Range {
    let line = span.line.saturating_sub(1);
    let start_col = span.column.saturating_sub(1);
    let end_col = start_col + span.length;
    Range {
        start: Position { line, character: start_col },
        end:   Position { line, character: end_col },
    }
}

/// Does `pos` (0-based LSP position) fall inside `span`? Single-line
/// approximation, matching `span_to_range`'s limitation.
pub fn span_contains(span: &crate::lexer::token::Span, pos: Position) -> bool {
    if span.line.saturating_sub(1) != pos.line {
        return false;
    }
    let start_col = span.column.saturating_sub(1);
    let end_col = start_col + span.length;
    pos.character >= start_col && pos.character <= end_col
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_you_mean_extracts_quoted_name() {
        assert_eq!(did_you_mean("Did you mean 'moisture'?"), Some("moisture".to_string()));
        assert_eq!(did_you_mean("did you mean 'CALL'?"), Some("CALL".to_string()));
    }

    #[test]
    fn did_you_mean_ignores_prose_suggestions() {
        assert_eq!(did_you_mean("Add an ownership keyword before this parameter"), None);
        assert_eq!(did_you_mean("Did you mean to call a function?"), None);
    }

    #[test]
    fn undefined_variable_typo_produces_a_replace_fix() {
        // `moisturee` is a distance-1 typo of the declared `moisture`,
        // so the symbol table's "Did you mean 'moisture'?" suggestion
        // should surface as a mechanical quick fix.
        const SRC: &str = "\
CONFIG {
    TARGET = \"microbit_v2\"
}

DECLARE Integer moisture INIT 0

RUN {
    LOOP {
        moisture = moisturee + 1
    }
}
";
        let analysis = analyse_text(SRC, "t.fe");
        let fix = analysis.fixes.iter().find(|f| f.new_text == "moisture")
            .expect("expected a spelling-correction fix");
        assert!(fix.title.contains("moisture"));
    }
}
