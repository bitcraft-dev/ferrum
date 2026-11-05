// `ferrum lsp --stdio` — a real language server, replacing the
// problem-matcher-only diagnostics the VS Code extension falls back
// to. The client side (ferrum-vscode/src/ferrumLsp.ts) has been
// wired and waiting for this since before this file existed.
//
// PROTOCOL
//   Standard LSP over stdio (Content-Length framed JSON-RPC), via the
//   same `lsp-server`/`lsp-types` crates rust-analyzer uses — see
//   compiler/Cargo.toml's comment for why this is the compiler's only
//   dependency.
//
// SUPPORTED
//   textDocument/didOpen, didChange, didClose -> publishDiagnostics
//   textDocument/hover       -> the resolved type at the cursor
//   textDocument/completion  -> keywords + names declared in this file
//
// NOT SCOPE-AWARE
//   Completion and hover walk the whole file's declarations rather
//   than tracking function-local scope precisely — the type checker
//   is still the source of truth for what's actually valid; this is
//   an editor convenience, not a second semantic pass.

mod analysis;
mod docs;
mod position;

use std::collections::HashMap;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CompletionItem, CompletionItemKind, CompletionOptions,
    CompletionParams, Hover, HoverContents, HoverParams, HoverProviderCapability, MarkupContent,
    MarkupKind, Position, PublishDiagnosticsParams, Range, ServerCapabilities, TextEdit,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkspaceEdit,
};

use crate::ast::{ExprKind, Program};
use crate::lexer::token::{SpannedToken, Token};
use position::CandidateKind;

/// Keywords a student would plausibly be typing — see
/// CLAUDE.md's "Language Quick Reference" for the canonical list.
const KEYWORDS: &[&str] = &[
    "CONFIG", "DEFINE", "CREATE", "DECLARE", "CONSTANT", "FUNCTION", "RUN",
    "EVERY", "LOOP", "IF", "ELSE", "FOR", "IN", "RANGE", "BREAK", "CONTINUE",
    "RETURN", "CALL", "PRINT", "DELAY", "TURN", "SET", "TOGGLE",
    "READ", "READ_PERCENT", "HIGH", "LOW", "TRUE", "FALSE",
    "GIVE", "LEND", "BORROW", "IS", "NOT", "AND", "OR",
    "INPUT", "OUTPUT", "ANALOG_INPUT", "PWM", "DISPLAY", "PULSE",
    "PIN", "PULL", "UP", "DOWN", "INIT", "AS",
];

pub fn run() {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions::default()),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
    };
    let capabilities = serde_json::to_value(capabilities).unwrap();

    if connection.initialize(capabilities).is_err() {
        // Client disconnected before finishing the handshake — nothing
        // more to do.
        return;
    }

    main_loop(&connection);

    // Dropping `connection` closes the channels the IO threads read
    // from/write to; join them so stdout is fully flushed before exit.
    drop(connection);
    let _ = io_threads.join();
}

fn main_loop(connection: &Connection) {
    // Document store: URI -> current full text, updated on every
    // didOpen/didChange (we asked for TextDocumentSyncKind::FULL, so
    // didChange always carries the whole new text, not a diff).
    let mut docs: HashMap<Url, String> = HashMap::new();

    // Last successfully-parsed AST per URI. While a student is mid-edit
    // the current text often doesn't parse; completion falls back to
    // this so it can still offer the names declared earlier in the file
    // instead of collapsing to keywords-only.
    let mut last_good: HashMap<Url, Program> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                // `handle_shutdown` sends the shutdown response itself
                // and then blocks for the client's follow-up `exit`
                // notification, consuming it from this same channel —
                // when it returns `Ok(true)` the server should stop.
                match connection.handle_shutdown(&req) {
                    Ok(true)  => return,
                    Ok(false) => handle_request(connection, &docs, &last_good, req),
                    Err(_)    => return, // client disconnected mid-shutdown
                }
            }
            Message::Notification(not) => {
                if not.method == "exit" {
                    return;
                }
                handle_notification(connection, &mut docs, &mut last_good, not);
            }
            Message::Response(_) => {
                // We never send requests to the client, so nothing to
                // correlate a response against.
            }
        }
    }
}

fn handle_request(
    connection: &Connection,
    docs: &HashMap<Url, String>,
    last_good: &HashMap<Url, Program>,
    req: Request,
) {
    let id = req.id.clone();

    let result = match req.method.as_str() {
        "textDocument/hover"      => handle_hover(docs, req),
        "textDocument/completion" => handle_completion(docs, last_good, req),
        "textDocument/codeAction" => handle_code_action(docs, req),
        other => {
            send_response(connection, Response::new_err(
                id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                format!("unhandled method: {}", other),
            ));
            return;
        }
    };

    send_response(connection, Response::new_ok(id, result));
}

fn handle_hover(docs: &HashMap<Url, String>, req: Request) -> serde_json::Value {
    let (_, params) = match req.extract::<HoverParams>("textDocument/hover") {
        Ok(v)  => v,
        Err(_) => return serde_json::Value::Null,
    };
    let uri = params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;

    let Some(text) = docs.get(&uri) else { return serde_json::Value::Null; };
    let analysis = analysis::analyse_text(text, uri.as_str());

    let Some((value, range)) = hover_at(&analysis, text, pos) else {
        return serde_json::Value::Null;
    };

    let hover = Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind:  MarkupKind::Markdown,
            value,
        }),
        range: Some(range),
    };
    serde_json::to_value(hover).unwrap_or(serde_json::Value::Null)
}

/// Resolve the hover shown at `pos`, returning the markdown body and the
/// range to underline. Tries three sources in order of specificity:
///   1. A type-checked expression (`expr.ty`) — the precise, computed
///      type of an expression, e.g. the result of `counter + 1`.
///   2. A bare identifier token — a device operand of `TURN`/`SET`, an
///      assignment target, etc. These aren't expressions, so step 1
///      misses them; resolve the name against the file's declarations.
///   3. A language keyword / type / builtin — show its teaching blurb.
fn hover_at(analysis: &analysis::Analysis, text: &str, pos: Position) -> Option<(String, Range)> {
    // 1. Type-checked expression at the cursor.
    if let Some(program) = &analysis.program {
        if let Some(expr) = position::expr_at(program, pos) {
            if let Some(ty) = &expr.ty {
                let body = match &expr.kind {
                    ExprKind::Ident(ident) => format!("`{}`: **{:?}**", ident.original, ty),
                    _                      => format!("**{:?}**", ty),
                };
                return Some((body, analysis::span_to_range(&expr.span)));
            }
        }
    }

    // Everything below needs the token under the cursor.
    let tok = token_at(&analysis.tokens, pos)?;
    let range = analysis::span_to_range(&tok.span);

    // 2. A user-defined name used somewhere the AST doesn't expose as an
    //    expression (device operands, assignment targets, definitions).
    if let Token::Ident(ident) = &tok.node {
        if let Some(program) = &analysis.program {
            if let Some(body) = position::describe_name(program, &ident.key) {
                return Some((body, range));
            }
        }
    }

    // 3. A language keyword / type / builtin — look up the source word.
    let word = token_text(text, &tok.span)?;
    let body = docs::keyword_doc(&word.to_uppercase())?;
    Some((format!("**{}** — {}", word, body), range))
}

/// The token whose span contains `pos`, if any.
fn token_at(tokens: &[SpannedToken], pos: Position) -> Option<&SpannedToken> {
    tokens.iter().find(|t| analysis::span_contains(&t.span, pos))
}

/// The exact source text a span covers. Spans are 1-based line/column
/// with a byte length; Ferrum keywords and identifiers are ASCII, so
/// byte offsets and character offsets coincide here.
fn token_text<'a>(text: &'a str, span: &crate::lexer::token::Span) -> Option<&'a str> {
    let line = text.lines().nth(span.line.saturating_sub(1) as usize)?;
    let start = span.column.saturating_sub(1) as usize;
    let end = start + span.length as usize;
    line.get(start..end)
}

fn handle_completion(
    docs: &HashMap<Url, String>,
    last_good: &HashMap<Url, Program>,
    req: Request,
) -> serde_json::Value {
    let (_, params) = match req.extract::<CompletionParams>("textDocument/completion") {
        Ok(v)  => v,
        Err(_) => return serde_json::Value::Null,
    };
    let uri = params.text_document_position.text_document.uri;

    let mut items: Vec<CompletionItem> = KEYWORDS.iter().map(|kw| CompletionItem {
        label: kw.to_string(),
        kind:  Some(CompletionItemKind::KEYWORD),
        ..Default::default()
    }).collect();

    // Prefer the current text's AST; fall back to the last one that
    // parsed. Completion fires while a line is half-typed, which usually
    // breaks the current parse — without the fallback the user's own
    // names would vanish exactly when they're reaching for them.
    let current = docs
        .get(&uri)
        .map(|text| analysis::analyse_text(text, uri.as_str()).program)
        .unwrap_or(None);
    let program = current.as_ref().or_else(|| last_good.get(&uri));

    if let Some(program) = program {
        for candidate in position::collect_identifiers(program) {
            let kind = match candidate.kind {
                CandidateKind::DeviceType     => CompletionItemKind::CLASS,
                CandidateKind::DeviceInstance => CompletionItemKind::VARIABLE,
                CandidateKind::Variable       => CompletionItemKind::VARIABLE,
                CandidateKind::Constant       => CompletionItemKind::CONSTANT,
                CandidateKind::Function       => CompletionItemKind::FUNCTION,
            };
            items.push(CompletionItem {
                label: candidate.name,
                kind: Some(kind),
                ..Default::default()
            });
        }
    }

    serde_json::to_value(items).unwrap_or(serde_json::Value::Null)
}

fn handle_code_action(docs: &HashMap<Url, String>, req: Request) -> serde_json::Value {
    let (_, params) = match req.extract::<CodeActionParams>("textDocument/codeAction") {
        Ok(v)  => v,
        Err(_) => return serde_json::Value::Null,
    };
    let uri = params.text_document.uri;
    let requested = params.range;

    let Some(text) = docs.get(&uri) else { return serde_json::Value::Null; };
    let analysis = analysis::analyse_text(text, uri.as_str());

    // Offer only fixes whose originating diagnostic overlaps the range
    // the editor asked about (the line/selection the cursor is on).
    let actions: Vec<CodeActionOrCommand> = analysis
        .fixes
        .iter()
        .filter(|fix| ranges_overlap(&fix.diagnostic_range, &requested))
        .map(|fix| {
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![TextEdit {
                range:    fix.edit_range,
                new_text: fix.new_text.clone(),
            }]);
            CodeActionOrCommand::CodeAction(CodeAction {
                title: fix.title.clone(),
                kind:  Some(CodeActionKind::QUICKFIX),
                edit:  Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            })
        })
        .collect();

    serde_json::to_value(actions).unwrap_or(serde_json::Value::Null)
}

/// Do two ranges share any position? Used to decide whether a fix
/// belongs to the range the editor requested code actions for.
fn ranges_overlap(a: &Range, b: &Range) -> bool {
    !(before(&a.end, &b.start) || before(&b.end, &a.start))
}

fn before(p: &Position, q: &Position) -> bool {
    (p.line, p.character) < (q.line, q.character)
}

fn handle_notification(
    connection: &Connection,
    docs: &mut HashMap<Url, String>,
    last_good: &mut HashMap<Url, Program>,
    not: Notification,
) {
    match not.method.as_str() {
        "textDocument/didOpen" => {
            if let Ok(params) = not.extract::<lsp_types::DidOpenTextDocumentParams>("textDocument/didOpen") {
                let uri  = params.text_document.uri.clone();
                let text = params.text_document.text;
                publish_diagnostics(connection, &uri, &text, last_good);
                docs.insert(uri, text);
            }
        }
        "textDocument/didChange" => {
            if let Ok(params) = not.extract::<lsp_types::DidChangeTextDocumentParams>("textDocument/didChange") {
                // Full sync: the last content change *is* the whole
                // new document (see ServerCapabilities::text_document_sync above).
                if let Some(change) = params.content_changes.into_iter().last() {
                    let uri = params.text_document.uri;
                    publish_diagnostics(connection, &uri, &change.text, last_good);
                    docs.insert(uri, change.text);
                }
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) = not.extract::<lsp_types::DidCloseTextDocumentParams>("textDocument/didClose") {
                let uri = params.text_document.uri;
                docs.remove(&uri);
                last_good.remove(&uri);
            }
        }
        _ => {}
    }
}

fn publish_diagnostics(
    connection: &Connection,
    uri: &Url,
    text: &str,
    last_good: &mut HashMap<Url, Program>,
) {
    let analysis = analysis::analyse_text(text, uri.as_str());
    // Remember the AST whenever the text parses, so completion can fall
    // back to it during the next broken-parse keystroke.
    if let Some(program) = &analysis.program {
        last_good.insert(uri.clone(), program.clone());
    }
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: analysis.diagnostics,
        version: None,
    };
    let not = Notification::new("textDocument/publishDiagnostics".to_string(), params);
    let _ = connection.sender.send(Message::Notification(not));
}

fn send_response(connection: &Connection, response: Response) {
    let _ = connection.sender.send(Message::Response(response));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
CONFIG {
    TARGET = \"microbit_v2\"
}

DEFINE Led AS OUTPUT

CREATE Led status ON PIN 13

RUN {
    LOOP {
        TURN status HIGH
        DELAY 500ms
    }
}
";

    fn hover_body(text: &str, line: u32, character: u32) -> Option<String> {
        let analysis = analysis::analyse_text(text, "t.fe");
        hover_at(&analysis, text, Position { line, character }).map(|(body, _)| body)
    }

    #[test]
    fn hover_on_device_operand_of_turn_resolves_instance_type() {
        // `status` in `TURN status HIGH` is a bare Ident, not an
        // expression — this is exactly the case plain expr_at missed.
        let body = hover_body(SRC, 10, 15).expect("expected hover on the TURN operand");
        assert!(body.contains("status"), "got: {body}");
        assert!(body.contains("Led"), "got: {body}");
    }

    #[test]
    fn hover_on_keyword_shows_teaching_blurb() {
        let body = hover_body(SRC, 10, 9).expect("expected hover on TURN keyword");
        assert!(body.contains("TURN"), "got: {body}");
        // The docs.rs blurb for TURN mentions HIGH or LOW.
        assert!(body.contains("HIGH") || body.contains("LOW"), "got: {body}");
    }

    #[test]
    fn hover_on_whitespace_is_none() {
        // Column 0 of the LOOP line — no token there.
        assert!(hover_body(SRC, 9, 0).is_none());
    }

    #[test]
    fn code_action_offers_fix_for_stray_bang() {
        // A lone `!` is the lexer's "did you mean '!='?" case, which is
        // a mechanical quick fix.
        const BANG: &str = "\
CONFIG {
    TARGET = \"microbit_v2\"
}

RUN {
    LOOP {
        DELAY 500ms
        !
    }
}
";
        let analysis = analysis::analyse_text(BANG, "t.fe");
        assert!(
            analysis.fixes.iter().any(|f| f.new_text == "!="),
            "expected a '!' -> '!=' fix",
        );
    }

    #[test]
    fn ranges_overlap_matches_only_intersecting_ranges() {
        let line = |l| Range {
            start: Position { line: l, character: 0 },
            end:   Position { line: l, character: 10 },
        };
        assert!(ranges_overlap(&line(3), &line(3)));
        assert!(!ranges_overlap(&line(3), &line(5)));
    }
}
