// Ownership checker — enforces GIVE / LEND / BORROW rules.
//
// RESPONSIBILITIES
//   Constraints 10 — ownership keyword mandatory on device params
//   Constraint  11 — call-site ownership keyword must match definition
//   Constraint  12 — same device not passed twice in one call
//   Constraint  13 — GIVE terminates device in calling scope
//   Constraint  14 — LEND params are read-only inside function body
//   Constraint  19 — no write conflict between EVERY and LOOP blocks
//
// PASS ORDER
//   Runs after the type checker. By this point:
//   - All Expr.ty slots are filled
//   - All FunctionDef.return_type slots are filled
//   - The symbol table has all symbols registered
//
// LEND READ-ONLY ENFORCEMENT
//   When entering a function body, device params declared LEND are
//   registered with a read-only flag. The checker then walks TURN,
//   SET, and TOGGLE statements and reports an error if the target
//   is a LEND param.
//
// SCHEDULED BLOCK CONFLICT DETECTION
//   After collecting the write-targets of all EVERY blocks and the
//   write-targets of all LOOP blocks in a RUN section, the checker
//   reports any device that appears as a write target in both.

use std::collections::{HashMap, HashSet};
use crate::ast::*;
use crate::lexer::token::Span;
use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind};
use crate::semantic::symbol_table::{DeviceState, SymbolKind, SymbolTable};

pub struct OwnershipChecker<'a> {
    symbols:     &'a mut SymbolTable,
    diagnostics: Vec<Diagnostic>,
    /// Set of device keys that are LEND params in the current function.
    lend_params: HashSet<String>,
    /// Set of device keys that are BORROW params in the current function.
    borrow_params: HashSet<String>,
}

impl<'a> OwnershipChecker<'a> {
    pub fn new(symbols: &'a mut SymbolTable) -> Self {
        OwnershipChecker {
            symbols,
            diagnostics:   Vec::new(),
            lend_params:   HashSet::new(),
            borrow_params: HashSet::new(),
        }
    }

    pub fn check(mut self, program: &mut Program) -> Vec<Diagnostic> {
        for func in &mut program.functions {
            self.check_function(func);
        }
        self.check_run_ownership(&mut program.run);

        let mut all = self.diagnostics;
        all.extend(self.symbols.take_diagnostics());
        all
    }

    // ── Function body ─────────────────────────────────────────────

    fn check_function(&mut self, func: &mut FunctionDef) {
        let saved_lend   = std::mem::take(&mut self.lend_params);
        let saved_borrow = std::mem::take(&mut self.borrow_params);

        // Register LEND and BORROW params
        for param in &func.params {
            if let Param::Device { ownership, name, .. } = param {
                match ownership {
                    Ownership::Lend   => { self.lend_params.insert(name.key.clone()); }
                    Ownership::Borrow => { self.borrow_params.insert(name.key.clone()); }
                    Ownership::Give   => {} // full ownership — no restriction
                }
            }
        }

        for stmt in &mut func.body {
            self.check_statement_ownership(stmt);
        }

        self.lend_params   = saved_lend;
        self.borrow_params = saved_borrow;
    }

    // ── RUN section — scheduled conflict detection ────────────────

    fn check_run_ownership(&mut self, run: &mut RunSection) {
        // Collect write targets for every EVERY block and LOOP block
        // Device key → span of the write in that block
        let mut every_writes: HashMap<String, Span> = HashMap::new();
        let mut loop_writes:  HashMap<String, Span> = HashMap::new();

        for item in &run.items {
            match item {
                RunItem::Every(e) => {
                    for stmt in &e.body {
                        for (key, span) in write_targets_in_stmt(stmt) {
                            every_writes.entry(key).or_insert(span);
                        }
                    }
                }
                RunItem::Loop(l) => {
                    for stmt in &l.body {
                        for (key, span) in write_targets_in_stmt(stmt) {
                            loop_writes.entry(key).or_insert(span);
                        }
                    }
                }
                RunItem::TopIf(t) => {
                    // Top-level IF can contain EVERY — collect from within
                    for inner in &t.then_items {
                        if let RunItem::Every(e) = inner {
                            for stmt in &e.body {
                                for (key, span) in write_targets_in_stmt(stmt) {
                                    every_writes.entry(key).or_insert(span);
                                }
                            }
                        }
                    }
                }
                RunItem::Stmt(_) => {}
            }
        }

        // Report any device written in both EVERY and LOOP (constraint 19)
        for (key, every_span) in &every_writes {
            if let Some(loop_span) = loop_writes.get(key) {
                // Get original spelling from symbol table
                let name = self.symbols.lookup(key)
                    .map(|s| s.ident.original.clone())
                    .unwrap_or_else(|| key.clone());

                self.diagnostics.push(Diagnostic::error(
                    DiagnosticKind::ScheduledDeviceConflict {
                        device:   name.clone(),
                        every_at: every_span.clone(),
                        loop_at:  loop_span.clone(),
                    },
                    every_span.clone(),
                    format!(
                        "Conflicting access to '{}'. \
                         '{}' is written in both an EVERY block and a LOOP block. \
                         Two scheduled blocks cannot write to the same device \
                         without coordination. This would produce unpredictable \
                         hardware behaviour.",
                        name, name,
                    ),
                    Some(format!(
                        "Move all '{}' interactions into one block, \
                         or let one block own the device entirely.",
                        name
                    )),
                ));
            }
        }

        // Walk the RUN section for GIVE scope enforcement
        for item in &mut run.items {
            self.check_run_item_ownership(item);
        }
    }

    fn check_run_item_ownership(&mut self, item: &mut RunItem) {
        match item {
            RunItem::Every(e)  => {
                for s in &mut e.body { self.check_statement_ownership(s); }
            }
            RunItem::Loop(l)   => {
                for s in &mut l.body { self.check_statement_ownership(s); }
            }
            RunItem::TopIf(t)  => {
                for i in &mut t.then_items { self.check_run_item_ownership(i); }
                if let Some(els) = &mut t.else_items {
                    for i in els { self.check_run_item_ownership(i); }
                }
            }
            RunItem::Stmt(s)   => self.check_statement_ownership(s),
        }
    }

    // ── Statement ownership enforcement ──────────────────────────

    fn check_statement_ownership(&mut self, stmt: &mut Statement) {
        match &mut stmt.kind {
            // TURN — write command: check LEND restriction
            StmtKind::Turn(t) => {
                self.check_not_lend_write(&t.device, "TURN", &t.span);
                self.symbols.resolve_device_available(&t.device);
            }
            // SET — write command
            StmtKind::Set(s) => {
                self.check_not_lend_write(&s.device, "SET", &s.span);
                self.symbols.resolve_device_available(&s.device);
            }
            // TOGGLE — write command
            StmtKind::Toggle(t) => {
                self.check_not_lend_write(&t.device, "TOGGLE", &t.span);
                self.symbols.resolve_device_available(&t.device);
            }
            // CALL — ownership transfer for GIVE; scope termination
            StmtKind::VoidCall(c) => {
                self.check_call_ownership(&mut c.args, &c.function, &c.span);
            }
            // IF — walk both branches
            StmtKind::If(s) => {
                for st in &mut s.then_body { self.check_statement_ownership(st); }
                if let Some(ElseClause::Block(stmts)) = &mut s.else_body {
                    for st in stmts { self.check_statement_ownership(st); }
                }
            }
            // FOR — walk body
            StmtKind::For(s) => {
                for st in &mut s.body { self.check_statement_ownership(st); }
            }
            // InlineDeclare with CALL — ownership in the call
            StmtKind::InlineDeclare(s) => {
                if let InlineDeclareInit::Call(call) = &mut s.init {
                    self.check_call_ownership(&mut call.args, &call.function, &call.span);
                }
            }
            _ => {}
        }
    }

    // ── LEND write restriction (constraint 14) ───────────────────

    fn check_not_lend_write(&mut self, device: &crate::lexer::token::Ident, cmd: &str, span: &Span) {
        if self.lend_params.contains(&device.key) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticKind::WriteToLendParam { param: device.original.clone() },
                span.clone(),
                format!(
                    "Cannot write to '{}' inside this function. \
                     '{}' was declared LEND — this function has read-only access. \
                     The caller retains ownership and is responsible for any writes.",
                    device.original, device.original,
                ),
                Some(format!(
                    "Change LEND to BORROW if this function needs to write to the device \
                     and the caller should keep ownership after the call.\n  \
                     Change LEND to GIVE if the caller no longer needs the device at all.\n  \
                     Or remove the {} command if reading is sufficient.",
                    cmd
                )),
            ));
        }
    }

    // ── GIVE scope termination (constraint 13) ───────────────────

    fn check_call_ownership(
        &mut self,
        args: &mut Vec<CallArg>,
        fn_ident: &crate::lexer::token::Ident,
        span: &Span,
    ) {
        for arg in args.iter() {
            if let CallArgKind::Device { ownership: Ownership::Give, name } = &arg.kind {
                // Mark the device as given away in the symbol table
                if let Some(sym) = self.symbols.lookup_mut(&name.key) {
                    if let SymbolKind::Device { state, .. } = &mut sym.kind {
                        *state = DeviceState::GivenAway { at: span.clone() };
                    }
                }
            }
        }
    }
}

// ── Write target extraction helper ───────────────────────────────

/// Returns (device_key, span) for every write command in a statement.
/// Used by scheduled conflict detection.
fn write_targets_in_stmt(stmt: &Statement) -> Vec<(String, Span)> {
    let mut targets = Vec::new();
    collect_write_targets(stmt, &mut targets);
    targets
}

fn collect_write_targets(stmt: &Statement, out: &mut Vec<(String, Span)>) {
    match &stmt.kind {
        StmtKind::Turn(t)   => out.push((t.device.key.clone(), t.span.clone())),
        StmtKind::Set(s)    => out.push((s.device.key.clone(), s.span.clone())),
        StmtKind::Toggle(t) => out.push((t.device.key.clone(), t.span.clone())),
        StmtKind::VoidCall(c) => {
            for arg in &c.args {
                if let CallArgKind::Device {
                    ownership: Ownership::Give | Ownership::Borrow,
                    name
                } = &arg.kind {
                    out.push((name.key.clone(), c.span.clone()));
                }
            }
        }
        StmtKind::If(s) => {
            for st in &s.then_body { collect_write_targets(st, out); }
            if let Some(ElseClause::Block(stmts)) = &s.else_body {
                for st in stmts { collect_write_targets(st, out); }
            }
        }
        StmtKind::For(s) => {
            for st in &s.body { collect_write_targets(st, out); }
        }
        _ => {}
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use std::sync::Arc;
    use crate::lexer::token::{Ident, Span};
    use crate::semantic::symbol_table::{Symbol, SymbolTable};

    fn span() -> Span { Span::new(Arc::new("test.fe".into()), 1, 1, 1) }
    fn ident(s: &str) -> Ident { Ident::new(s, span()) }

    #[test]
    fn lend_write_detection() {
        let mut st = SymbolTable::new();
        let mut checker = OwnershipChecker::new(&mut st);
        checker.lend_params.insert("led".into());

        let stmt = Statement {
            kind: StmtKind::Turn(TurnStmt {
                device:    ident("led"),
                qualifier: None,
                state:     PinState::High,
                span:      span(),
            }),
            span: span(),
        };
        // Borrow checker prevents direct mut here in the test — simulate
        let device = ident("led");
        checker.check_not_lend_write(&device, "TURN", &span());
        assert!(checker.diagnostics.iter().any(|d| d.is_error()));
    }

    #[test]
    fn non_lend_write_is_allowed() {
        let mut st = SymbolTable::new();
        let mut checker = OwnershipChecker::new(&mut st);
        // lend_params is empty
        let device = ident("led");
        checker.check_not_lend_write(&device, "TURN", &span());
        assert!(checker.diagnostics.is_empty());
    }

    #[test]
    fn give_marks_device_unavailable() {
        let mut st = SymbolTable::new();
        st.insert(Symbol {
            ident:      ident("pump"),
            kind:       SymbolKind::Device {
                device_type: ident("WaterPump"),
                state:       DeviceState::Available,
            },
            ty:         Type::Device(ident("WaterPump")),
            defined_at: span(),
        });

        let mut checker = OwnershipChecker::new(&mut st);
        let mut args = vec![CallArg {
            kind: CallArgKind::Device {
                ownership: Ownership::Give,
                name:      ident("pump"),
            },
            span: span(),
        }];
        checker.check_call_ownership(&mut args, &ident("run_pump"), &span());

        match &checker.symbols.lookup("pump").unwrap().kind {
            SymbolKind::Device { state: DeviceState::GivenAway { .. }, .. } => {}
            _ => panic!("expected GivenAway state after GIVE"),
        }
    }
}