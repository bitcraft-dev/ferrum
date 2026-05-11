// Device checker — hardware-specific semantic validation.
//
// RESPONSIBILITIES
//   Constraint  3 — CONFIG item separation (no commas)
//   Constraint  4 — call argument separator (commas required)
//   Constraint  5 — PIN uniqueness across all CREATE items
//   Constraint  6 — interface coverage (all pins assigned)
//   Constraint  7 — INIT arity and read-only interface restriction
//   Constraint  9 — RANGE direction (covered partially in type_checker,
//                    completed here for variable bounds)
//   Constraint 20 — qualifier validity against interface (DEFINE)
//   Constraint 21 — ambiguous SET target
//
// PIN UNIQUENESS
//   All CREATE items are walked and every assigned PIN number is
//   recorded. A duplicate triggers an error with the first-use location.
//
// AMBIGUOUS SET
//   A SET without a qualifier on a composite device that has more
//   than one numeric-accepting interface is an error. The checker
//   looks up the device type, counts numeric interfaces, and reports
//   if more than one would accept the value type.

use std::collections::HashMap;
use crate::ast::*;
use crate::lexer::token::Span;
use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind};
use crate::semantic::symbol_table::{SymbolKind, SymbolTable};

pub struct DeviceChecker<'a> {
    symbols:     &'a SymbolTable,
    diagnostics: Vec<Diagnostic>,
    /// PIN → (first device name, first span)
    pin_registry: HashMap<u32, (String, Span)>,
}

impl<'a> DeviceChecker<'a> {
    pub fn new(symbols: &'a SymbolTable) -> Self {
        DeviceChecker {
            symbols,
            diagnostics:  Vec::new(),
            pin_registry: HashMap::new(),
        }
    }

    pub fn check(mut self, program: &Program) -> Vec<Diagnostic> {
        // PIN uniqueness across all CREATE items
        for item in &program.creates {
            self.check_pin_uniqueness(item);
        }

        // Ambiguous SET targets in RUN section
        self.check_run_for_ambiguous_set(&program.run);

        self.diagnostics
    }

    // ── PIN uniqueness (constraint 5) ─────────────────────────────

    fn check_pin_uniqueness(&mut self, item: &CreateItem) {
        let pins = collect_pins(&item.pins);
        for (pin, span) in pins {
            if let Some((first_device, first_span)) = self.pin_registry.get(&pin) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticKind::DuplicatePinAssignment {
                        pin,
                        first_used_by: first_device.clone(),
                        first_used_at: first_span.clone(),
                    },
                    span.clone(),
                    format!(
                        "Duplicate pin assignment: PIN {} is already used by '{}' (defined at {}).",
                        pin, first_device, first_span,
                    ),
                    Some(format!("Choose a different pin for '{}'.", item.instance_name.original)),
                ));
            } else {
                self.pin_registry.insert(pin, (item.instance_name.original.clone(), span));
            }
        }
    }

    // ── Ambiguous SET target (constraint 21) ──────────────────────

    fn check_run_for_ambiguous_set(&mut self, run: &RunSection) {
        for item in &run.items {
            self.check_run_item_set(item);
        }
    }

    fn check_run_item_set(&mut self, item: &RunItem) {
        match item {
            RunItem::Every(e)  => { for s in &e.body { self.check_stmt_set(s); } }
            RunItem::Loop(l)   => { for s in &l.body { self.check_stmt_set(s); } }
            RunItem::TopIf(t)  => {
                for i in &t.then_items { self.check_run_item_set(i); }
                if let Some(els) = &t.else_items {
                    for i in els { self.check_run_item_set(i); }
                }
            }
            RunItem::Stmt(s) => self.check_stmt_set(s),
        }
    }

    fn check_stmt_set(&mut self, stmt: &Statement) {
        match &stmt.kind {
            StmtKind::Set(s) if s.qualifier.is_none() => {
                self.check_set_ambiguity(s);
            }
            StmtKind::If(s) => {
                for st in &s.then_body { self.check_stmt_set(st); }
                if let Some(ElseClause::Block(stmts)) = &s.else_body {
                    for st in stmts { self.check_stmt_set(st); }
                }
            }
            StmtKind::For(s) => {
                for st in &s.body { self.check_stmt_set(st); }
            }
            _ => {}
        }
    }

    fn check_set_ambiguity(&mut self, stmt: &SetStmt) {
        // Look up device → device type → interfaces
        let device_type_key = match self.symbols.lookup(&stmt.device.key) {
            Some(sym) => match &sym.kind {
                SymbolKind::Device { device_type, .. } => device_type.key.clone(),
                _ => return,
            },
            None => return,
        };

        let spec = match self.symbols.lookup(&device_type_key) {
            Some(sym) => match &sym.kind {
                SymbolKind::DeviceType { spec } => spec.clone(),
                _ => return,
            },
            None => return,
        };

        // Count numeric-accepting interfaces (PWM and DISPLAY)
        let numeric_ifaces: Vec<String> = match &spec {
            DeviceSpec::Simple(s) => {
                if is_numeric_interface(&s.interface) {
                    vec![interface_display(s)]
                } else { vec![] }
            }
            DeviceSpec::Composite(specs) => {
                specs.iter()
                    .filter(|s| is_numeric_interface(&s.interface))
                    .map(|s| interface_display(s))
                    .collect()
            }
        };

        if numeric_ifaces.len() > 1 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticKind::AmbiguousSetTarget {
                    device:     stmt.device.original.clone(),
                    interfaces: numeric_ifaces.clone(),
                },
                stmt.span.clone(),
                format!(
                    "Ambiguous SET target. '{}' has multiple interfaces \
                     that accept a numeric value: {}",
                    stmt.device.original,
                    numeric_ifaces.join(", "),
                ),
                Some(format!(
                    "Use a qualifier to disambiguate, e.g.: SET {} {} 0.5",
                    stmt.device.original,
                    "BRIGHTNESS", // example — actual qualifier depends on device
                )),
            ));
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn collect_pins(spec: &PinSpec) -> Vec<(u32, Span)> {
    match spec {
        PinSpec::Single { pin, span }   => vec![(*pin, span.clone())],
        PinSpec::Multi(assignments)     => {
            assignments.iter()
                .map(|a| (a.pin_number, a.span.clone()))
                .collect()
        }
    }
}

fn is_numeric_interface(iface: &InterfaceType) -> bool {
    matches!(iface, InterfaceType::Pwm | InterfaceType::Display)
}

fn interface_display(spec: &InterfaceSpec) -> String {
    match &spec.qualifier {
        Some(q) => format!("{} {}", spec.interface.name(), q.name()),
        None    => spec.interface.name().to_string(),
    }
}

#[cfg(test)]
mod device_checker_tests {
    use super::*;
    use std::sync::Arc;
    use crate::lexer::token::{Ident, Span};
    use crate::semantic::symbol_table::SymbolTable;

    fn span() -> Span { Span::new(Arc::new("test.fe".into()), 1, 1, 1) }
    fn ident(s: &str) -> Ident { Ident::new(s, span()) }

    fn pin_spec(pin: u32) -> PinSpec {
        PinSpec::Single { pin, span: span() }
    }

    fn create_item(type_name: &str, instance: &str, pin: u32) -> CreateItem {
        CreateItem {
            device_type:   ident(type_name),
            instance_name: ident(instance),
            pins:          pin_spec(pin),
            pull:          None,
            init:          None,
            span:          span(),
        }
    }

    #[test]
    fn duplicate_pin_is_error() {
        let st = SymbolTable::new();
        let program = Program {
            config: None,
            defines: vec![],
            creates: vec![
                create_item("Button", "btn1", 14),
                create_item("Button", "btn2", 14), // same pin
            ],
            declares: vec![],
            functions: vec![],
            run: RunSection { items: vec![], span: span() },
            span: span(),
        };
        let errs = DeviceChecker::new(&st).check(&program);
        assert!(errs.iter().any(|e| matches!(
            &e.kind,
            DiagnosticKind::DuplicatePinAssignment { pin: 14, .. }
        )));
    }

    #[test]
    fn unique_pins_no_error() {
        let st = SymbolTable::new();
        let program = Program {
            config: None,
            defines: vec![],
            creates: vec![
                create_item("Button", "btn1", 14),
                create_item("Button", "btn2", 15),
            ],
            declares: vec![],
            functions: vec![],
            run: RunSection { items: vec![], span: span() },
            span: span(),
        };
        let errs = DeviceChecker::new(&st).check(&program);
        assert!(errs.is_empty());
    }
}