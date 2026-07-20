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
mod position;

use std::collections::HashMap;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, Hover, HoverContents,
    HoverParams, HoverProviderCapability, MarkupContent, MarkupKind,
    PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    Url,
};

use crate::ast::ExprKind;
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

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                // `handle_shutdown` sends the shutdown response itself
                // and then blocks for the client's follow-up `exit`
                // notification, consuming it from this same channel —
                // when it returns `Ok(true)` the server should stop.
                match connection.handle_shutdown(&req) {
                    Ok(true)  => return,
                    Ok(false) => handle_request(connection, &docs, req),
                    Err(_)    => return, // client disconnected mid-shutdown
                }
            }
            Message::Notification(not) => {
                if not.method == "exit" {
                    return;
                }
                handle_notification(connection, &mut docs, not);
            }
            Message::Response(_) => {
                // We never send requests to the client, so nothing to
                // correlate a response against.
            }
        }
    }
}

fn handle_request(connection: &Connection, docs: &HashMap<Url, String>, req: Request) {
    let id = req.id.clone();

    let result = match req.method.as_str() {
        "textDocument/hover"      => handle_hover(docs, req),
        "textDocument/completion" => handle_completion(docs, req),
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
    let Some(program) = &analysis.program else { return serde_json::Value::Null; };

    let Some(expr) = position::expr_at(program, pos) else { return serde_json::Value::Null; };
    let Some(ty) = &expr.ty else { return serde_json::Value::Null; };

    let label = match &expr.kind {
        ExprKind::Ident(ident) => format!("`{}`: **{:?}**", ident.original, ty),
        _                      => format!("**{:?}**", ty),
    };

    let hover = Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: label,
        }),
        range: Some(analysis::span_to_range(&expr.span)),
    };
    serde_json::to_value(hover).unwrap_or(serde_json::Value::Null)
}

fn handle_completion(docs: &HashMap<Url, String>, req: Request) -> serde_json::Value {
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

    if let Some(text) = docs.get(&uri) {
        let analysis = analysis::analyse_text(text, uri.as_str());
        if let Some(program) = &analysis.program {
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
    }

    serde_json::to_value(items).unwrap_or(serde_json::Value::Null)
}

fn handle_notification(connection: &Connection, docs: &mut HashMap<Url, String>, not: Notification) {
    match not.method.as_str() {
        "textDocument/didOpen" => {
            if let Ok(params) = not.extract::<lsp_types::DidOpenTextDocumentParams>("textDocument/didOpen") {
                let uri  = params.text_document.uri.clone();
                let text = params.text_document.text;
                publish_diagnostics(connection, &uri, &text);
                docs.insert(uri, text);
            }
        }
        "textDocument/didChange" => {
            if let Ok(params) = not.extract::<lsp_types::DidChangeTextDocumentParams>("textDocument/didChange") {
                // Full sync: the last content change *is* the whole
                // new document (see ServerCapabilities::text_document_sync above).
                if let Some(change) = params.content_changes.into_iter().last() {
                    let uri = params.text_document.uri;
                    publish_diagnostics(connection, &uri, &change.text);
                    docs.insert(uri, change.text);
                }
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) = not.extract::<lsp_types::DidCloseTextDocumentParams>("textDocument/didClose") {
                docs.remove(&params.text_document.uri);
            }
        }
        _ => {}
    }
}

fn publish_diagnostics(connection: &Connection, uri: &Url, text: &str) {
    let analysis = analysis::analyse_text(text, uri.as_str());
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
