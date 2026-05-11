// ferrum/compiler/src/semantic/type_checker.rs
//
// Type checker for the Ferrum semantic pass.
//
// RESPONSIBILITIES
//   This checker walks the AST and enforces:
//
//   Constraint  6 — type compatibility (assignment, parameters, operators)
//   Constraint  7 — INIT value types and PULL restrictions
//   Constraint  8 — Percentage literal range 0.0–100.0
//                   Byte literal range 0–255
//   Constraint 18 — RETURN type consistency within a function
//   Constraint 20 — qualifier validity against interface type
//   Constraint 21 — ambiguous SET target
//   Constraint 22 — IS / IS NOT only on device/Boolean
//   Constraint 25 — inline IF both branches same type
//   Constraint 26 — simple device INIT arity
//
//   Also fills every Expr.ty and FunctionDef.return_type slot.
//
// PASS ORDER
//   This checker runs after the symbol table is populated by the
//   declaration collector (which registers all DEFINE, CREATE,
//   DECLARE, and FUNCTION symbols before any body is checked).
//   It reads the symbol table but does not modify ownership state
//   (that is the ownership checker's job).

use crate::ast::*;
use crate::lexer::token::{Ident, Span};
use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind};
use crate::semantic::symbol_table::{
    ParamInfo, ParamKind, SymbolKind, SymbolTable, UsageTracker,
};

// ----------------------------------------------------------------
// TypeChecker
// ----------------------------------------------------------------

pub struct TypeChecker<'a> {
    symbols:     &'a mut SymbolTable,
    diagnostics: Vec<Diagnostic>,
    tracker:     UsageTracker,
    /// Name of the function currently being checked — for return type tracking.
    current_fn:  Option<String>,
    /// Return types seen so far in the current function — for consistency check.
    return_types: Vec<(Type, Span)>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(symbols: &'a mut SymbolTable) -> Self {
        TypeChecker {
            symbols,
            diagnostics: Vec::new(),
            tracker:     UsageTracker::new(),
            current_fn:  None,
            return_types: Vec::new(),
        }
    }

    pub fn check(mut self, program: &mut Program) -> Vec<Diagnostic> {
        // CONFIG — validate key types
        if let Some(cfg) = &program.config {
            self.check_config(cfg);
        }

        // DEFINE — validate qualifier/interface combinations
        for item in &program.defines {
            self.check_define(item);
        }

        // CREATE — validate pin assignments and INIT values
        for item in &mut program.creates {
            self.check_create(item);
        }

        // DECLARE — validate initial value types and ranges
        for item in &mut program.declares {
            self.check_declare_item(item);
        }

        // FUNCTION — check bodies, fill return types
        for func in &mut program.functions {
            self.check_function(func);
        }

        // RUN — check all run items
        self.check_run_section(&mut program.run);

        // Collect diagnostics from symbol table and merge with our own
        let mut all = self.diagnostics;
        all.extend(self.symbols.take_diagnostics());
        all
    }

    // ── CONFIG ───────────────────────────────────────────────────

    fn check_config(&mut self, cfg: &ConfigSection) {
        for item in &cfg.items {
            match &item.key {
                ConfigKey::Unknown(k) => {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::UnknownConfigKey { key: k.clone() },
                        item.span.clone(),
                        format!(
                            "Unknown CONFIG key '{}'. Valid keys: \
                             TARGET, CLOCK_SPEED, SERIAL, DEFAULT_PULL_UP, \
                             DEBOUNCE_MS, OPTIMIZE, DEBUG",
                            k
                        ),
                        None,
                    ));
                }
                ConfigKey::Target => {
                    if !matches!(item.value, ConfigValue::Str(_)) {
                        self.config_type_error("TARGET", "String", &item.value, item.span.clone());
                    }
                }
                ConfigKey::ClockSpeed => {
                    if !matches!(item.value, ConfigValue::Clock(_)) {
                        self.config_type_error("CLOCK_SPEED", "clock speed (e.g. 64MHZ)", &item.value, item.span.clone());
                    }
                }
                ConfigKey::Serial | ConfigKey::DebounceMs => {
                    if !matches!(item.value, ConfigValue::Int(_)) {
                        self.config_type_error(item.key.name(), "Integer", &item.value, item.span.clone());
                    }
                }
                ConfigKey::DefaultPullUp | ConfigKey::Debug => {
                    if !matches!(item.value, ConfigValue::Bool(_)) {
                        self.config_type_error(item.key.name(), "Boolean", &item.value, item.span.clone());
                    }
                }
                ConfigKey::Optimize => {
                    match &item.value {
                        ConfigValue::Str(s) if matches!(s.as_str(), "speed" | "size" | "none") => {}
                        ConfigValue::Str(s) => {
                            self.emit(Diagnostic::error(
                                DiagnosticKind::ConfigTypeMismatch {
                                    key:      "OPTIMIZE".into(),
                                    expected: r#""speed", "size", or "none""#.into(),
                                    found:    format!("\"{}\"", s),
                                },
                                item.span.clone(),
                                format!(
                                    "OPTIMIZE value '{}' is not valid. \
                                     Use \"speed\", \"size\", or \"none\".", s
                                ),
                                Some("OPTIMIZE = \"speed\"".into()),
                            ));
                        }
                        _ => {
                            self.config_type_error("OPTIMIZE", r#""speed", "size", or "none""#, &item.value, item.span.clone());
                        }
                    }
                }
            }
        }
    }

    fn config_type_error(&mut self, key: &str, expected: &str, found: &ConfigValue, span: Span) {
        let found_str = match found {
            ConfigValue::Str(s)  => format!("\"{}\"", s),
            ConfigValue::Int(n)  => n.to_string(),
            ConfigValue::Clock(n)=> format!("{}MHZ", n),
            ConfigValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        };
        self.emit(Diagnostic::error(
            DiagnosticKind::ConfigTypeMismatch {
                key:      key.into(),
                expected: expected.into(),
                found:    found_str.clone(),
            },
            span,
            format!("CONFIG key '{}' expects {}, found {}", key, expected, found_str),
            None,
        ));
    }

    // ── DEFINE ───────────────────────────────────────────────────

    fn check_define(&mut self, item: &DefineItem) {
        let specs = match &item.spec {
            DeviceSpec::Simple(s)       => vec![s],
            DeviceSpec::Composite(list) => list.iter().collect(),
        };

        for spec in specs {
            if let Some(q) = &spec.qualifier {
                if !q.valid_interfaces().contains(&spec.interface) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::InvalidQualifier {
                            qualifier:        q.name().into(),
                            interface:        spec.interface.name().into(),
                            valid_interfaces: q.valid_interfaces().iter()
                                              .map(|i| i.name().into())
                                              .collect(),
                        },
                        spec.span.clone(),
                        format!(
                            "Invalid qualifier '{}' for interface '{}'. \
                             '{}' is valid on {} only.",
                            q.name(),
                            spec.interface.name(),
                            q.name(),
                            q.valid_interfaces().iter()
                             .map(|i| i.name())
                             .collect::<Vec<_>>()
                             .join(", ")
                        ),
                        None,
                    ));
                }
            }
        }
    }

    // ── CREATE ───────────────────────────────────────────────────

    fn check_create(&mut self, item: &mut CreateItem) {
        // Resolve the device type
        let def = match self.symbols.lookup(&item.device_type.key) {
            Some(sym) => sym.clone(),
            None => {
                // Already reported as undefined by symbol table population
                return;
            }
        };

        let specs: Vec<InterfaceSpec> = match &def.kind {
            SymbolKind::DeviceType { spec } => match spec {
                DeviceSpec::Simple(s)    => vec![s.clone()],
                DeviceSpec::Composite(v) => v.clone(),
            },
            _ => return,
        };

        // PULL validation — only valid on INPUT
        if let Some(_pull) = &item.pull {
            let has_input = specs.iter().any(|s| s.interface == InterfaceType::Input);
            if !has_input {
                self.emit(Diagnostic::error(
                    DiagnosticKind::PullOnNonInput {
                        interface: specs.first()
                            .map(|s| s.interface.name().to_string())
                            .unwrap_or_default(),
                    },
                    item.span.clone(),
                    format!(
                        "PULL is only valid on INPUT interfaces. \
                         Device '{}' does not have an INPUT interface.",
                        item.device_type.original
                    ),
                    None,
                ));
            }
        }

        // INIT validation
        if let Some(init) = &item.init {
            let writable: Vec<&InterfaceSpec> = specs.iter()
                .filter(|s| !s.interface.is_read_only())
                .collect();

            // Read-only interface INIT
            let has_readonly_init = specs.iter().any(|s| s.interface.is_read_only());
            if has_readonly_init && specs.len() == 1 {
                // Simple device with read-only interface
                self.emit(Diagnostic::error(
                    DiagnosticKind::InitOnReadOnlyInterface {
                        interface: specs[0].interface.name().into(),
                    },
                    init.span.clone(),
                    format!(
                        "INIT is not permitted on {} interfaces. \
                         '{}' is read-only — its state is driven by the hardware.",
                        specs[0].interface.name(),
                        item.instance_name.original
                    ),
                    Some(format!(
                        "To configure pull-up resistance, use PULL UP or PULL DOWN:\n  \
                         CREATE {} {} ON PIN N PULL UP",
                        item.device_type.original, item.instance_name.original
                    )),
                ));
                return;
            }

            // Arity check — INIT count must match writable interface count
            if init.values.len() != writable.len() {
                self.emit(Diagnostic::error(
                    DiagnosticKind::PartialInit {
                        device:   item.instance_name.original.clone(),
                        expected: writable.len(),
                        found:    init.values.len(),
                    },
                    init.span.clone(),
                    format!(
                        "Partial INIT not permitted for device '{}'. \
                         Expected {} values (one per writable interface), found {}.",
                        item.instance_name.original,
                        writable.len(),
                        init.values.len()
                    ),
                    None,
                ));
                return;
            }

            // Type-check each INIT value against its interface
            for (entry, iface) in init.values.iter().zip(writable.iter()) {
                self.check_init_value_for_interface(&entry.value, iface, &entry.span);
            }
        }

        // PIN uniqueness is checked separately in the device_checker.
    }

    fn check_init_value_for_interface(
        &mut self,
        value: &InitValue,
        iface: &InterfaceSpec,
        span: &Span,
    ) {
        match iface.interface {
            InterfaceType::Output => {
                if !matches!(value, InitValue::High | InitValue::Low) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "HIGH or LOW".into(),
                            found:    init_value_type_name(value).into(),
                            context:  format!("INIT for OUTPUT interface"),
                        },
                        span.clone(),
                        format!(
                            "OUTPUT interfaces must be initialised with HIGH or LOW, \
                             found {}.",
                            init_value_type_name(value)
                        ),
                        None,
                    ));
                }
            }
            InterfaceType::Pwm => {
                match value {
                    InitValue::Decimal(f) => {
                        if *f < 0.0 || *f > 1.0 {
                            self.emit(Diagnostic::error(
                                DiagnosticKind::ValueOutOfRange {
                                    value:     f.to_string(),
                                    ty:        "PWM INIT".into(),
                                    valid_min: "0.0".into(),
                                    valid_max: "1.0".into(),
                                },
                                span.clone(),
                                format!(
                                    "PWM INIT value {} is out of range. \
                                     Valid range is 0.0 to 1.0.",
                                    f
                                ),
                                None,
                            ));
                        }
                    }
                    _ => {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::TypeMismatch {
                                expected: "Decimal (0.0 to 1.0)".into(),
                                found:    init_value_type_name(value).into(),
                                context:  "INIT for PWM interface".into(),
                            },
                            span.clone(),
                            format!(
                                "PWM interfaces must be initialised with a decimal \
                                 value (0.0 to 1.0), found {}.",
                                init_value_type_name(value)
                            ),
                            None,
                        ));
                    }
                }
            }
            InterfaceType::Display => {
                if !matches!(value, InitValue::Str(_) | InitValue::Int(_)) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "String or Integer".into(),
                            found:    init_value_type_name(value).into(),
                            context:  "INIT for DISPLAY interface".into(),
                        },
                        span.clone(),
                        format!(
                            "DISPLAY interfaces must be initialised with a String \
                             or Integer, found {}.",
                            init_value_type_name(value)
                        ),
                        None,
                    ));
                }
            }
            // Read-only interfaces filtered out before reaching here
            _ => {}
        }
    }

    // ── DECLARE ──────────────────────────────────────────────────

    fn check_declare_item(&mut self, item: &mut DeclareItem) {
        match &item.kind {
            DeclareKind::Variable(v) => self.check_variable_decl(v, &item.span),
            DeclareKind::Constant(c) => self.check_constant_decl(c, &item.span),
        }
    }

    fn check_variable_decl(&mut self, decl: &VariableDecl, span: &Span) {
        if let Some(init) = &decl.init {
            let expected = decl.ty.to_type();
            match init {
                InitExpr::Single(lit) => {
                    self.check_literal_against_type(lit, &expected, span);
                }
                InitExpr::Array(lits) => {
                    if decl.array_size.is_none() {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::TypeMismatch {
                                expected: format!("{}[N]", decl.ty.name()),
                                found:    "array literal".into(),
                                context:  format!("DECLARE {}", decl.name.original),
                            },
                            span.clone(),
                            format!(
                                "Array literal requires an array type declaration. \
                                 Declare '{}' as {}[{}] to use an array initialiser.",
                                decl.name.original,
                                decl.ty.name(),
                                lits.len()
                            ),
                            None,
                        ));
                    } else {
                        for lit in lits {
                            self.check_literal_against_type(lit, &expected, span);
                        }
                    }
                }
            }
        }
    }

    fn check_constant_decl(&mut self, decl: &ConstantDecl, span: &Span) {
        let expected = decl.ty.to_type();
        self.check_literal_against_type(&decl.value, &expected, span);
    }

    // ── FUNCTION ─────────────────────────────────────────────────

    fn check_function(&mut self, func: &mut FunctionDef) {
        self.current_fn = Some(func.name.original.clone());
        self.return_types.clear();
        self.symbols.push_scope(format!("function '{}'", func.name.original));

        // Register parameters in function scope
        for param in &func.params {
            match param {
                Param::Data { ty, name, span } => {
                    use crate::semantic::symbol_table::Symbol;
                    self.symbols.insert(Symbol {
                        ident:      name.clone(),
                        kind:       SymbolKind::Variable { mutable: false, array_size: None },
                        ty:         ty.to_type(),
                        defined_at: span.clone(),
                    });
                }
                Param::Device { ownership: _, device_ty, name, span } => {
                    use crate::semantic::symbol_table::{DeviceState, Symbol};
                    self.symbols.insert(Symbol {
                        ident:      name.clone(),
                        kind:       SymbolKind::Device {
                            device_type: device_ty.clone(),
                            state:       DeviceState::Available,
                        },
                        ty:         Type::Device(device_ty.clone()),
                        defined_at: span.clone(),
                    });
                }
            }
        }

        // Check body statements
        for stmt in &mut func.body {
            self.check_statement(stmt);
        }

        // Check return statement
        if let Some(ret) = &mut func.ret {
            if let Some(expr) = &mut ret.value {
                let ty = self.check_expr(expr);
                self.return_types.push((ty, ret.span.clone()));
            }
            // else: void return — no type to record
        }

        // Enforce return type consistency (constraint 18)
        let fn_return_type = self.resolve_return_type(func);
        func.return_type = Some(fn_return_type);

        self.symbols.pop_scope();
        self.current_fn = None;
    }

    /// Determine the final return type of a function from all RETURN
    /// statements seen, enforcing consistency (constraint 18).
    fn resolve_return_type(&mut self, func: &FunctionDef) -> Type {
        if self.return_types.is_empty() {
            return Type::Void;
        }

        let (first_ty, first_span) = self.return_types[0].clone();
        let func_name = func.name.original.clone();
        
        // Collect all remaining returns so the immutable borrow of self.return_types ends
        let remaining: Vec<_> = self.return_types[1..].iter().cloned().collect();

        for (ty, span) in remaining {
            if ty != first_ty {
                self.emit(Diagnostic::error(
                    DiagnosticKind::ConflictingReturnTypes {
                        function: func_name.clone(),
                        type_a:   type_name(&first_ty).into(),
                        type_b:   type_name(&ty).into(),
                        site_a:   first_span.clone(),
                        site_b:   span.clone(),
                    },
                    span.clone(),
                    format!(
                        "Function '{}' has conflicting return types: \
                         first RETURN is {}, this RETURN is {}.",
                        func_name,
                        type_name(&first_ty),
                        type_name(&ty)
                    ),
                    Some("All RETURN statements in a function must return the same type.".into()),
                ));
            }
        }

        first_ty
    }
    // ── RUN section ──────────────────────────────────────────────

    fn check_run_section(&mut self, run: &mut RunSection) {
        self.symbols.push_scope("RUN");
        for item in &mut run.items {
            self.check_run_item(item);
        }
        self.symbols.pop_scope();
    }

    fn check_run_item(&mut self, item: &mut RunItem) {
        match item {
            RunItem::Every(e) => self.check_every_block(e),
            RunItem::TopIf(t) => self.check_top_if(t),
            RunItem::Loop(l)  => self.check_loop_block(l),
            RunItem::Stmt(s)  => self.check_statement(s),
        }
    }

    fn check_every_block(&mut self, block: &mut EveryBlock) {
        self.symbols.push_scope("EVERY block");
        for stmt in &mut block.body {
            // Delay-exceeds-period warning (constraint 16)
            if let StmtKind::Delay(d) = &stmt.kind {
                if d.duration.as_millis() >= block.period.as_millis() {
                    self.emit(Diagnostic::warning(
                        DiagnosticKind::DelayExceedsEveryPeriod {
                            delay_ms:  d.duration.as_millis(),
                            period_ms: block.period.as_millis(),
                        },
                        d.span.clone(),
                        format!(
                            "DELAY {}ms inside EVERY {}ms may cause timing violations. \
                             The delay equals or exceeds the scheduled period.",
                            d.duration.as_millis(),
                            block.period.as_millis()
                        ),
                        Some("Reduce the delay or increase the EVERY period.".into()),
                    ));
                }
            }
            self.check_statement(stmt);
        }
        self.symbols.pop_scope();
    }

    fn check_top_if(&mut self, block: &mut TopIfBlock) {
        let cond_ty = self.check_expr(&mut block.condition);
        self.expect_boolean_condition(&cond_ty, &block.condition.span);

        self.symbols.push_scope("IF block (RUN top level)");
        for item in &mut block.then_items { self.check_run_item(item); }
        self.symbols.pop_scope();

        if let Some(else_items) = &mut block.else_items {
            self.symbols.push_scope("ELSE block (RUN top level)");
            for item in else_items { self.check_run_item(item); }
            self.symbols.pop_scope();
        }
    }

    fn check_loop_block(&mut self, block: &mut LoopBlock) {
        self.symbols.push_scope("LOOP block");
        for stmt in &mut block.body { self.check_statement(stmt); }
        self.symbols.pop_scope();
    }

    // ── Statements ───────────────────────────────────────────────

    fn check_statement(&mut self, stmt: &mut Statement) {
        match &mut stmt.kind {
            StmtKind::Assignment(s)    => self.check_assignment(s),
            StmtKind::Set(s)           => self.check_set(s),
            StmtKind::Turn(s)          => self.check_turn(s),
            StmtKind::Toggle(s)        => self.check_toggle(s),
            StmtKind::Print(s)         => { self.check_expr(&mut s.value); }
            StmtKind::VoidCall(s)      => self.check_call_stmt(s),
            StmtKind::Delay(_)         => { /* duration is validated syntactically */ }
            StmtKind::If(s)            => self.check_if_stmt(s),
            StmtKind::For(s)           => self.check_for_stmt(s),
            StmtKind::InlineDeclare(s) => self.check_inline_declare(s),
            StmtKind::Break            |
            StmtKind::Continue         => { /* validated by parser */ }
        }
    }

    fn check_assignment(&mut self, stmt: &mut AssignStmt) {
        self.tracker.mark_used(&stmt.target.key);
        let val_ty = self.check_expr(&mut stmt.value);

        match self.symbols.lookup(&stmt.target.key).map(|s| s.kind.clone()) {
            None => {
                // Error already reported by symbol table
            }
            Some(SymbolKind::Constant) => {
                self.emit(Diagnostic::error(
                    DiagnosticKind::AssignToConstant { name: stmt.target.original.clone() },
                    stmt.span.clone(),
                    format!(
                        "Cannot assign to constant '{}'. \
                         Constants cannot be changed after declaration.",
                        stmt.target.original
                    ),
                    None,
                ));
            }
            Some(SymbolKind::Variable { .. }) => {
                let expected_ty = self.symbols.lookup(&stmt.target.key)
                    .map(|s| s.ty.clone())
                    .unwrap_or(Type::Void);
                self.expect_type_match(&expected_ty, &val_ty, &stmt.span, &stmt.target.original);
            }
            Some(SymbolKind::Device { .. }) => {
                self.emit(Diagnostic::error(
                    DiagnosticKind::TypeMismatch {
                        expected: "variable".into(),
                        found:    "device".into(),
                        context:  format!("assignment to '{}'", stmt.target.original),
                    },
                    stmt.span.clone(),
                    format!(
                        "'{}' is a device instance and cannot be assigned to directly. \
                         Use TURN, SET, or TOGGLE to control it.",
                        stmt.target.original
                    ),
                    None,
                ));
            }
            _ => {}
        }
    }

    fn check_set(&mut self, stmt: &mut SetStmt) {
        self.tracker.mark_used(&stmt.device.key);
        self.check_expr(&mut stmt.value);
        // Qualifier/ambiguity checking is done in device_checker.
        // Here we just ensure the device exists.
        self.symbols.resolve_device_available(&stmt.device);
    }

    fn check_turn(&mut self, stmt: &mut TurnStmt) {
        self.tracker.mark_used(&stmt.device.key);
        self.symbols.resolve_device_available(&stmt.device);
        // OUTPUT-interface check is in device_checker.
    }

    fn check_toggle(&mut self, stmt: &mut ToggleStmt) {
        self.tracker.mark_used(&stmt.device.key);
        self.symbols.resolve_device_available(&stmt.device);
    }

    fn check_call_stmt(&mut self, stmt: &mut CallStmt) {
        self.tracker.mark_used(&stmt.function.key);
        match self.symbols.lookup(&stmt.function.key).cloned() {
            None => {
                self.emit(Diagnostic::error(
                    DiagnosticKind::UndefinedFunction { name: stmt.function.original.clone() },
                    stmt.span.clone(),
                    format!("'{}' is not defined", stmt.function.original),
                    None,
                ));
            }
            Some(sym) => {
                match &sym.kind {
                    SymbolKind::Function { params, .. } => {
                        self.check_call_args(&mut stmt.args, params, &stmt.function, &stmt.span);
                    }
                    _ => {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::UndefinedFunction { name: stmt.function.original.clone() },
                            stmt.span.clone(),
                            format!(
                                "'{}' is not a function",
                                stmt.function.original
                            ),
                            Some(format!("CALL requires a function name. \
                                '{}' is a {}.",
                                stmt.function.original,
                                symbol_kind_name(&sym.kind)
                            )),
                        ));
                    }
                }
            }
        }
    }

    fn check_if_stmt(&mut self, stmt: &mut IfStmt) {
        let cond_ty = self.check_expr(&mut stmt.condition);
        self.expect_boolean_condition(&cond_ty, &stmt.condition.span);

        self.symbols.push_scope("IF block");
        for s in &mut stmt.then_body { self.check_statement(s); }
        self.symbols.pop_scope();

        if let Some(ElseClause::Block(stmts)) = &mut stmt.else_body {
            self.symbols.push_scope("ELSE block");
            for s in stmts { self.check_statement(s); }
            self.symbols.pop_scope();
        }
    }

    fn check_for_stmt(&mut self, stmt: &mut ForStmt) {
        self.symbols.push_scope(format!("FOR block"));

        // Check iterator and determine element type
        let elem_ty = match &mut stmt.iterator {
            ForIterator::Array(expr) => {
                let arr_ty = self.check_expr(expr);
                match arr_ty {
                    Type::Array(elem, _) => *elem,
                    _ => {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::TypeMismatch {
                                expected: "array".into(),
                                found:    type_name(&arr_ty).into(),
                                context:  "FOR iterator".into(),
                            },
                            expr.span.clone(),
                            format!(
                                "FOR iterator must be an array, found {}.",
                                type_name(&arr_ty)
                            ),
                            None,
                        ));
                        Type::Integer // recovery
                    }
                }
            }
            ForIterator::Range { from, to } => {
                let from_ty = self.check_expr(from);
                let to_ty   = self.check_expr(to);
                if !matches!(from_ty, Type::Integer) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::RangeBoundsNotInteger { found: type_name(&from_ty).into() },
                        from.span.clone(),
                        format!("RANGE start must be Integer, found {}.", type_name(&from_ty)),
                        None,
                    ));
                }
                if !matches!(to_ty, Type::Integer) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::RangeBoundsNotInteger { found: type_name(&to_ty).into() },
                        to.span.clone(),
                        format!("RANGE end must be Integer, found {}.", type_name(&to_ty)),
                        None,
                    ));
                }
                // Literal range direction check (constraint 9)
                if let (ExprKind::Literal(Literal::Int(f)),
                        ExprKind::Literal(Literal::Int(t))) = (&from.kind, &to.kind) {
                    if f > t {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::RangeDirectionError { from: *f, to: *t },
                            from.span.clone().to(&to.span),
                            format!(
                                "RANGE start {} must be <= end {}. \
                                 RANGE only supports ascending sequences.",
                                f, t
                            ),
                            None,
                        ));
                    }
                }
                Type::Integer
            }
        };

        // Register the loop variable in the FOR scope
        use crate::semantic::symbol_table::Symbol;
        self.symbols.insert(Symbol {
            ident:      stmt.variable.clone(),
            kind:       SymbolKind::Variable { mutable: false, array_size: None },
            ty:         elem_ty,
            defined_at: stmt.variable.span.clone(),
        });

        for s in &mut stmt.body { self.check_statement(s); }
        self.symbols.pop_scope();
    }

    fn check_inline_declare(&mut self, stmt: &mut InlineDeclareStmt) {
        let expected_ty = stmt.ty.to_type();

        let actual_ty = match &mut stmt.init {
            InlineDeclareInit::Value(init_expr) => {
                match init_expr {
                    InitExpr::Single(lit) => {
                        self.check_literal_against_type(lit, &expected_ty, &stmt.span);
                        expected_ty.clone()
                    }
                    InitExpr::Array(lits) => {
                        for lit in lits.iter() {
                            self.check_literal_against_type(lit, &expected_ty, &stmt.span);
                        }
                        Type::Array(Box::new(expected_ty.clone()), lits.len())
                    }
                }
            }
            InlineDeclareInit::Call(call) => {
                self.check_call_expr(call)
            }
        };

        // Only check type match for non-call inits — call return type
        // is checked inside check_call_expr.
        if !matches!(stmt.init, InlineDeclareInit::Call(_)) {
            self.expect_type_match(&expected_ty, &actual_ty, &stmt.span, &stmt.name.original);
        }

        // Register in current scope
        use crate::semantic::symbol_table::Symbol;
        self.symbols.insert(Symbol {
            ident:      stmt.name.clone(),
            kind:       SymbolKind::Variable {
                mutable:    true,
                array_size: stmt.array_size,
            },
            ty:         expected_ty,
            defined_at: stmt.name.span.clone(),
        });
    }

    // ── Call argument checking ────────────────────────────────────

    fn check_call_args(
        &mut self,
        args: &mut Vec<CallArg>,
        params: &[ParamInfo],
        fn_ident: &Ident,
        span: &Span,
    ) {
        // Arity check
        if args.len() != params.len() {
            self.emit(Diagnostic::error(
                DiagnosticKind::TypeMismatch {
                    expected: format!("{} argument(s)", params.len()),
                    found:    format!("{} argument(s)", args.len()),
                    context:  format!("call to '{}'", fn_ident.original),
                },
                span.clone(),
                format!(
                    "Function '{}' expects {} argument(s), found {}.",
                    fn_ident.original, params.len(), args.len()
                ),
                None,
            ));
            return;
        }

        // Device uniqueness — no device passed twice in one call (constraint 12)
        let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();
        for arg in args.iter() {
            if let CallArgKind::Device { name, .. } = &arg.kind {
                if !seen_devices.insert(name.key.clone()) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::DevicePassedTwice { device: name.original.clone() },
                        arg.span.clone(),
                        format!(
                            "'{}' cannot be passed twice in the same call. \
                             A device can only be passed once per function call.",
                            name.original
                        ),
                        None,
                    ));
                }
            }
        }

        // Per-parameter type / ownership checks
        for (arg, param) in args.iter_mut().zip(params.iter()) {
            match (&mut arg.kind, &param.kind) {
                // Data arg, data param
                (CallArgKind::Data(expr), ParamKind::Data(expected_ty)) => {
                    let found_ty = self.check_expr(expr);
                    self.expect_type_match(expected_ty, &found_ty, &arg.span, &param.name.original);
                }
                // Device arg, device param — check ownership keyword match
                (CallArgKind::Device { ownership: given, name },
                 ParamKind::Device { ownership: expected, device_type: _ }) => {
                    if given != expected {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::OwnershipMismatch {
                                function: fn_ident.original.clone(),
                                param:    param.name.original.clone(),
                                expected: ownership_name(expected).into(),
                                found:    ownership_name(given).into(),
                            },
                            arg.span.clone(),
                            format!(
                                "Ownership mismatch for device '{}'. \
                                 Function '{}' declares parameter '{}' as {}, \
                                 but {} was used here.",
                                name.original,
                                fn_ident.original,
                                param.name.original,
                                ownership_name(expected),
                                ownership_name(given),
                            ),
                            Some(format!(
                                "Use GIVE to transfer full ownership (caller loses the device).\n  \
                                 Use BORROW to grant temporary write access (caller keeps the device).\n  \
                                 Use LEND to grant temporary read-only access (caller keeps the device)."
                            )),
                        ));
                    }
                    // Mark device as used
                    self.tracker.mark_used(&name.key);
                }
                // Mismatch: data given for device param or vice versa
                (CallArgKind::Device { name, .. }, ParamKind::Data(_)) => {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "data value".into(),
                            found:    format!("device '{}'", name.original),
                            context:  format!("argument {} of call to '{}'", param.name.original, fn_ident.original),
                        },
                        arg.span.clone(),
                        format!(
                            "Parameter '{}' of '{}' expects a data value, \
                             but device '{}' was passed.",
                            param.name.original, fn_ident.original, name.original
                        ),
                        None,
                    ));
                }
                (CallArgKind::Data(_), ParamKind::Device { .. }) => {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: format!("device (with GIVE, LEND, or BORROW)"),
                            found:    "data value".into(),
                            context:  format!("argument {} of call to '{}'", param.name.original, fn_ident.original),
                        },
                        arg.span.clone(),
                        format!(
                            "Parameter '{}' of '{}' expects a device argument \
                             with GIVE, LEND, or BORROW.",
                            param.name.original, fn_ident.original
                        ),
                        None,
                    ));
                }
            }
        }
    }

    fn check_call_expr(&mut self, call: &mut CallExpr) -> Type {
        match self.symbols.lookup(&call.function.key).cloned() {
            None => {
                self.emit(Diagnostic::error(
                    DiagnosticKind::UndefinedFunction { name: call.function.original.clone() },
                    call.span.clone(),
                    format!("'{}' is not defined", call.function.original),
                    None,
                ));
                Type::Void
            }
            Some(sym) => {
                match &sym.kind {
                    SymbolKind::Function { params, return_type } => {
                        let rt = return_type.clone();
                        let params = params.clone();
                        self.check_call_args(&mut call.args, &params, &call.function, &call.span);
                        rt
                    }
                    _ => {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::UndefinedFunction { name: call.function.original.clone() },
                            call.span.clone(),
                            format!("'{}' is not a function", call.function.original),
                            None,
                        ));
                        Type::Void
                    }
                }
            }
        }
    }

    // ── Expressions ──────────────────────────────────────────────

    /// Type-check an expression, filling its `.ty` field.
    /// Returns the resolved type.
    pub fn check_expr(&mut self, expr: &mut Expr) -> Type {
        let ty = self.resolve_expr_type(expr);
        expr.ty = Some(ty.clone());
        ty
    }

    fn resolve_expr_type(&mut self, expr: &mut Expr) -> Type {
        match &mut expr.kind {

            ExprKind::Literal(lit) => literal_type(lit),

            ExprKind::Ident(ident) => {
                self.tracker.mark_used(&ident.key);
                match self.symbols.resolve(ident) {
                    Some(sym) => sym.ty.clone(),
                    None      => Type::Void,
                }
            }

            ExprKind::Read(device) => {
                self.tracker.mark_used(&device.key);
                match self.symbols.lookup(&device.key).cloned() {
                    Some(sym) => {
                        match &sym.kind {
                            SymbolKind::Device { device_type, .. } => {
                                // Look up the device type to find its interface
                                let dt_key = device_type.key.clone();
                                match self.symbols.lookup(&dt_key).cloned() {
                                    Some(def_sym) => {
                                        if let SymbolKind::DeviceType { spec } = &def_sym.kind {
                                            let interface = first_interface(spec);
                                            match interface {
                                                Some(InterfaceType::Input)       => Type::PinState,
                                                Some(InterfaceType::AnalogInput) => Type::AnalogRaw,
                                                _ => {
                                                    self.emit(Diagnostic::error(
                                                        DiagnosticKind::TypeMismatch {
                                                            expected: "INPUT or ANALOG_INPUT".into(),
                                                            found:    interface.map(|i| i.name().to_string())
                                                                               .unwrap_or_default(),
                                                            context:  format!("READ {}", device.original),
                                                        },
                                                        expr.span.clone(),
                                                        format!(
                                                            "READ is only valid on INPUT or ANALOG_INPUT devices. \
                                                             '{}' has a different interface.",
                                                            device.original
                                                        ),
                                                        None,
                                                    ));
                                                    Type::Void
                                                }
                                            }
                                        } else { Type::Void }
                                    }
                                    None => Type::Void,
                                }
                            }
                            _ => {
                                self.emit(Diagnostic::error(
                                    DiagnosticKind::TypeMismatch {
                                        expected: "device".into(),
                                        found:    "variable".into(),
                                        context:  format!("READ {}", device.original),
                                    },
                                    expr.span.clone(),
                                    format!("'{}' is not a device — READ requires a device name.", device.original),
                                    None,
                                ));
                                Type::Void
                            }
                        }
                    }
                    None => Type::Void,
                }
            }

            ExprKind::ReadPercent(device) => {
                self.tracker.mark_used(&device.key);
                match self.symbols.lookup(&device.key).cloned() {
                    Some(sym) => {
                        if let SymbolKind::Device { device_type, .. } = &sym.kind {
                            let dt_key = device_type.key.clone();
                            if let Some(def_sym) = self.symbols.lookup(&dt_key).cloned() {
                                if let SymbolKind::DeviceType { spec } = &def_sym.kind {
                                    let interface = first_interface(spec);
                                    if !matches!(interface, Some(InterfaceType::AnalogInput)) {
                                        self.emit(Diagnostic::error(
                                            DiagnosticKind::ReadPercentOnDigitalInput {
                                                device: device.original.clone(),
                                            },
                                            expr.span.clone(),
                                            format!(
                                                "READ_PERCENT is not valid for device '{}'. \
                                                 READ_PERCENT requires an ANALOG_INPUT interface.",
                                                device.original
                                            ),
                                            Some(format!("Use READ {} for digital devices.", device.original)),
                                        ));
                                    }
                                }
                            }
                        }
                        Type::Percentage
                    }
                    None => Type::Void,
                }
            }

            ExprKind::BinOp { op, left, right } => {
                let lt = self.check_expr(left);
                let rt = self.check_expr(right);
                self.resolve_binop_type(op, &lt, &rt, &expr.span)
            }

            ExprKind::UnaryOp { op, operand } => {
                let ot = self.check_expr(operand);
                match op {
                    UnaryOp::Not => {
                        if !matches!(ot, Type::Boolean) {
                            self.emit(Diagnostic::error(
                                DiagnosticKind::TypeMismatch {
                                    expected: "Boolean".into(),
                                    found:    type_name(&ot).into(),
                                    context:  "NOT operator".into(),
                                },
                                expr.span.clone(),
                                format!("NOT requires a Boolean operand, found {}.", type_name(&ot)),
                                None,
                            ));
                        }
                        Type::Boolean
                    }
                    UnaryOp::Neg => {
                        if !matches!(ot, Type::Integer | Type::Decimal | Type::Percentage) {
                            self.emit(Diagnostic::error(
                                DiagnosticKind::TypeMismatch {
                                    expected: "numeric type".into(),
                                    found:    type_name(&ot).into(),
                                    context:  "unary negation".into(),
                                },
                                expr.span.clone(),
                                format!("Unary minus requires a numeric operand, found {}.", type_name(&ot)),
                                None,
                            ));
                        }
                        ot
                    }
                }
            }

            ExprKind::Is { target, negated: _, state } => {
                let target_ty = self.check_expr(target);
                let _state_ty = self.check_expr(state);

                // Constraint 22: IS / IS NOT only on device or Boolean
                match &target_ty {
                    Type::Boolean    => {}
                    Type::PinState   => {}
                    Type::Device(_)  => {}
                    _ => {
                        self.emit(Diagnostic::error(
                            DiagnosticKind::IsOnNonDeviceOrBoolean {
                                expression_type: type_name(&target_ty).into(),
                            },
                            target.span.clone(),
                            format!(
                                "'IS' can only be used with device interfaces or Boolean variables. \
                                 '{}' is of type {}. Use '==' for value comparison.",
                                // best-effort: grab ident name if available
                                if let ExprKind::Ident(i) = &target.kind { i.original.as_str() } else { "expression" },
                                type_name(&target_ty)
                            ),
                            Some(format!("IF {} == ... {{ ... }}", if let ExprKind::Ident(i) = &target.kind { i.original.as_str() } else { "value" })),
                        ));
                    }
                }
                Type::Boolean
            }

            ExprKind::Call(call) => {
                self.check_call_expr(call)
            }

            ExprKind::IfExpr { condition, then_value, else_value } => {
                let cond_ty  = self.check_expr(condition);
                let then_ty  = self.check_expr(then_value);
                let else_ty  = self.check_expr(else_value);

                self.expect_boolean_condition(&cond_ty, &condition.span);

                // Constraint 25: both branches must return the same type
                if then_ty != else_ty {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::InlineIfTypeMismatch {
                            then_type: type_name(&then_ty).into(),
                            else_type: type_name(&else_ty).into(),
                        },
                        then_value.span.clone().to(&else_value.span),
                        format!(
                            "Both branches of an inline IF must return the same type. \
                             THEN returns {}, ELSE returns {}.",
                            type_name(&then_ty),
                            type_name(&else_ty)
                        ),
                        None,
                    ));
                }
                then_ty
            }
        }
    }

    fn resolve_binop_type(&mut self, op: &BinOp, lt: &Type, rt: &Type, span: &Span) -> Type {
        match op {
            // Arithmetic — both sides must be numeric, returns wider type
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                if !is_numeric(lt) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "numeric type".into(),
                            found:    type_name(lt).into(),
                            context:  format!("left side of '{}'", op.symbol()),
                        },
                        span.clone(),
                        format!("'{}' requires numeric operands; left side is {}.", op.symbol(), type_name(lt)),
                        None,
                    ));
                }
                if !is_numeric(rt) && !matches!((op, rt), (BinOp::Add, Type::String)) {
                    // String + anything is string concatenation — allowed
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "numeric type".into(),
                            found:    type_name(rt).into(),
                            context:  format!("right side of '{}'", op.symbol()),
                        },
                        span.clone(),
                        format!("'{}' requires numeric operands; right side is {}.", op.symbol(), type_name(rt)),
                        None,
                    ));
                }
                // String concatenation returns String
                if matches!(op, BinOp::Add) && (matches!(lt, Type::String) || matches!(rt, Type::String)) {
                    return Type::String;
                }
                // Promote to wider numeric type
                numeric_promote(lt, rt)
            }

            // Comparison — numeric both sides, returns Boolean
            BinOp::Gt | BinOp::Lt | BinOp::Gte | BinOp::Lte => {
                if !is_numeric(lt) || !is_numeric(rt) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "numeric types".into(),
                            found:    format!("{} and {}", type_name(lt), type_name(rt)),
                            context:  format!("comparison '{}'", op.symbol()),
                        },
                        span.clone(),
                        format!("Comparison '{}' requires numeric operands.", op.symbol()),
                        None,
                    ));
                }
                Type::Boolean
            }

            // Equality — same type on both sides, returns Boolean
            BinOp::Eq | BinOp::NotEq => {
                if !types_compatible(lt, rt) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: type_name(lt).into(),
                            found:    type_name(rt).into(),
                            context:  format!("'{}' comparison", op.symbol()),
                        },
                        span.clone(),
                        format!(
                            "Cannot compare {} and {} with '{}'. \
                             Both sides must be the same type.",
                            type_name(lt), type_name(rt), op.symbol()
                        ),
                        None,
                    ));
                }
                Type::Boolean
            }

            // Logical — Boolean both sides
            BinOp::And | BinOp::Or => {
                if !matches!(lt, Type::Boolean) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "Boolean".into(),
                            found:    type_name(lt).into(),
                            context:  format!("{} operator — left side", op.symbol()),
                        },
                        span.clone(),
                        format!("{} requires Boolean operands; left side is {}.", op.symbol(), type_name(lt)),
                        None,
                    ));
                }
                if !matches!(rt, Type::Boolean) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: "Boolean".into(),
                            found:    type_name(rt).into(),
                            context:  format!("{} operator — right side", op.symbol()),
                        },
                        span.clone(),
                        format!("{} requires Boolean operands; right side is {}.", op.symbol(), type_name(rt)),
                        None,
                    ));
                }
                Type::Boolean
            }
        }
    }

    // ── Literal range validation (constraints 6, 8) ───────────────

    fn check_literal_against_type(&mut self, lit: &Literal, expected: &Type, span: &Span) {
        match (lit, expected) {
            // Percentage literal range check: 0.0–100.0
            (Literal::Decimal(f), Type::Percentage) => {
                if *f < 0.0 || *f > 100.0 {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::ValueOutOfRange {
                            value:     f.to_string(),
                            ty:        "Percentage".into(),
                            valid_min: "0.0".into(),
                            valid_max: "100.0".into(),
                        },
                        span.clone(),
                        format!(
                            "Value {} is out of range for type Percentage (valid: 0.0–100.0).",
                            f
                        ),
                        Some(format!(
                            "Use Decimal if unconstrained range is intended, \
                             or clamp the value: CALL clamp {}, 0.0, 100.0", f
                        )),
                    ));
                }
            }
            // Byte literal range check: 0–255
            (Literal::Int(n), Type::Byte) => {
                if *n < 0 || *n > 255 {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::ValueOutOfRange {
                            value:     n.to_string(),
                            ty:        "Byte".into(),
                            valid_min: "0".into(),
                            valid_max: "255".into(),
                        },
                        span.clone(),
                        format!(
                            "Value {} is out of range for type Byte (valid: 0–255).",
                            n
                        ),
                        Some("Use Integer if a wider range is needed.".into()),
                    ));
                }
            }
            (Literal::Hex(n), Type::Byte) => {
                if *n > 255 {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::ValueOutOfRange {
                            value:     format!("0x{:X}", n),
                            ty:        "Byte".into(),
                            valid_min: "0x00".into(),
                            valid_max: "0xFF".into(),
                        },
                        span.clone(),
                        format!(
                            "Value 0x{:X} is out of range for type Byte (valid: 0x00–0xFF).",
                            n
                        ),
                        None,
                    ));
                }
            }
            // Type mismatch between literal and expected type
            (lit, expected) => {
                let lit_ty = Type::from_literal(lit);
                if !types_compatible(&lit_ty, expected) {
                    self.emit(Diagnostic::error(
                        DiagnosticKind::TypeMismatch {
                            expected: type_name(expected).into(),
                            found:    lit.type_name().into(),
                            context:  "literal value".into(),
                        },
                        span.clone(),
                        format!(
                            "Type mismatch: expected {}, found {}.",
                            type_name(expected),
                            lit.type_name()
                        ),
                        None,
                    ));
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn expect_boolean_condition(&mut self, ty: &Type, span: &Span) {
        if !matches!(ty, Type::Boolean | Type::PinState) {
            self.emit(Diagnostic::error(
                DiagnosticKind::TypeMismatch {
                    expected: "Boolean".into(),
                    found:    type_name(ty).into(),
                    context:  "IF condition".into(),
                },
                span.clone(),
                format!(
                    "IF condition must be Boolean, found {}. \
                     Use a comparison expression or a Boolean variable.",
                    type_name(ty)
                ),
                None,
            ));
        }
    }

    fn expect_type_match(&mut self, expected: &Type, found: &Type, span: &Span, context: &str) {
        if !types_compatible(expected, found) {
            self.emit(Diagnostic::error(
                DiagnosticKind::TypeMismatch {
                    expected: type_name(expected).into(),
                    found:    type_name(found).into(),
                    context:  context.into(),
                },
                span.clone(),
                format!(
                    "Type mismatch: '{}' expects {}, found {}.",
                    context, type_name(expected), type_name(found)
                ),
                None,
            ));
        }
    }

    fn emit(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }
}

// ----------------------------------------------------------------
// Type utility functions
// ----------------------------------------------------------------

impl Type {
    /// Infer the natural type of a literal.
    pub fn from_literal(lit: &Literal) -> Type {
        match lit {
            Literal::Int(_)     => Type::Integer,
            Literal::Hex(_)     => Type::Byte,
            Literal::Decimal(_) => Type::Decimal,
            Literal::Str(_)     => Type::String,
            Literal::Bool(_)    => Type::Boolean,
            Literal::High |
            Literal::Low        => Type::PinState,
        }
    }
}

pub fn type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Integer    => "Integer",
        Type::Decimal    => "Decimal",
        Type::Percentage => "Percentage",
        Type::Boolean    => "Boolean",
        Type::String     => "String",
        Type::Byte       => "Byte",
        Type::Array(..)  => "Array",
        Type::PinState   => "PinState",
        Type::AnalogRaw  => "Integer (raw analog)",
        Type::Device(_)  => "Device",
        Type::Void       => "Void",
    }
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Integer | Type::Decimal | Type::Percentage | Type::Byte | Type::AnalogRaw)
}

fn numeric_promote(a: &Type, b: &Type) -> Type {
    match (a, b) {
        (Type::Decimal, _) | (_, Type::Decimal)         => Type::Decimal,
        (Type::Percentage, _) | (_, Type::Percentage)   => Type::Percentage,
        _                                                => Type::Integer,
    }
}

fn types_compatible(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Integer, Type::Byte)  | (Type::Byte, Type::Integer)   => true,
        (Type::Integer, Type::AnalogRaw) | (Type::AnalogRaw, Type::Integer) => true,
        (Type::Decimal, Type::Percentage) | (Type::Percentage, Type::Decimal) => true,
        (x, y) => x == y,
    }
}

fn literal_type(lit: &Literal) -> Type {
    Type::from_literal(lit)
}

fn init_value_type_name(v: &InitValue) -> &'static str {
    match v {
        InitValue::High       => "HIGH",
        InitValue::Low        => "LOW",
        InitValue::Decimal(_) => "Decimal",
        InitValue::Str(_)     => "String",
        InitValue::Int(_)     => "Integer",
    }
}

fn first_interface(spec: &DeviceSpec) -> Option<InterfaceType> {
    match spec {
        DeviceSpec::Simple(s)    => Some(s.interface.clone()),
        DeviceSpec::Composite(v) => v.first().map(|s| s.interface.clone()),
    }
}

fn ownership_name(o: &Ownership) -> &'static str {
    match o {
        Ownership::Give   => "GIVE",
        Ownership::Lend   => "LEND",
        Ownership::Borrow => "BORROW",
    }
}

fn symbol_kind_name(k: &SymbolKind) -> &'static str {
    match k {
        SymbolKind::Variable { .. } => "variable",
        SymbolKind::Constant        => "constant",
        SymbolKind::Device { .. }   => "device",
        SymbolKind::Function { .. } => "function",
        SymbolKind::DeviceType { .. }=> "device type",
    }
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_promotion_decimal_wins() {
        assert_eq!(numeric_promote(&Type::Integer, &Type::Decimal), Type::Decimal);
        assert_eq!(numeric_promote(&Type::Decimal, &Type::Integer), Type::Decimal);
    }

    #[test]
    fn numeric_promotion_integers() {
        assert_eq!(numeric_promote(&Type::Integer, &Type::Integer), Type::Integer);
    }

    #[test]
    fn types_compatible_byte_and_integer() {
        assert!(types_compatible(&Type::Byte, &Type::Integer));
        assert!(types_compatible(&Type::Integer, &Type::Byte));
    }

    #[test]
    fn types_compatible_decimal_and_percentage() {
        assert!(types_compatible(&Type::Decimal, &Type::Percentage));
    }

    #[test]
    fn types_incompatible_string_and_integer() {
        assert!(!types_compatible(&Type::String, &Type::Integer));
    }

    #[test]
    fn literal_type_inference() {
        assert_eq!(Type::from_literal(&Literal::Int(42)),         Type::Integer);
        assert_eq!(Type::from_literal(&Literal::Decimal(3.14)),   Type::Decimal);
        assert_eq!(Type::from_literal(&Literal::Bool(true)),      Type::Boolean);
        assert_eq!(Type::from_literal(&Literal::High),            Type::PinState);
        assert_eq!(Type::from_literal(&Literal::Hex(0xFF)),       Type::Byte);
    }

    #[test]
    fn is_numeric_returns_correct_results() {
        assert!(is_numeric(&Type::Integer));
        assert!(is_numeric(&Type::Decimal));
        assert!(is_numeric(&Type::Percentage));
        assert!(is_numeric(&Type::Byte));
        assert!(!is_numeric(&Type::Boolean));
        assert!(!is_numeric(&Type::String));
    }
}