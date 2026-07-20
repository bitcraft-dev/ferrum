// Shared "run the pipeline on this text buffer" helper — used by
// diagnostics publishing, hover, and completion so all three always
// agree on what the document currently looks like.

use crate::ast::Program;
use crate::lexer;
use crate::parser;
use crate::semantic::{self, diagnostic::Diagnostic as FeDiagnostic};
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// Result of analysing one document's text: every diagnostic
/// (lex + parse + semantic) converted to LSP form, plus the annotated
/// AST if parsing succeeded — semantic errors don't prevent this from
/// being `Some` (see `SemanticResult::program`'s doc comment), only a
/// parse error does, since there's no tree to hand back at all then.
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub program:     Option<Program>,
}

pub fn analyse_text(text: &str, uri_path: &str) -> Analysis {
    let mut diagnostics = Vec::new();

    let lex_result = lexer::lex(text, uri_path);
    for e in &lex_result.errors {
        diagnostics.push(to_lsp_diagnostic(&e.span, &e.message, DiagnosticSeverity::ERROR));
    }

    let parse_result = parser::parser::parse(lex_result.tokens, uri_path);
    for e in &parse_result.errors {
        let msg = match &e.suggestion {
            Some(s) => format!("{}\nSuggestion: {}", e.message, s),
            None    => e.message.clone(),
        };
        diagnostics.push(to_lsp_diagnostic(&e.span, &msg, DiagnosticSeverity::ERROR));
    }

    let program = match parse_result.program {
        Some(program) => {
            let result = semantic::analyse(program);
            for d in &result.diagnostics {
                diagnostics.push(fe_diagnostic_to_lsp(d));
            }
            result.program
        }
        // Parse failed outright — no tree for semantic analysis, hover,
        // or completion to work from until the syntax is valid again.
        None => None,
    };

    Analysis { diagnostics, program }
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
