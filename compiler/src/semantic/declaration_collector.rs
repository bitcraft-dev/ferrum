// Declaration collector — first sub-pass of the semantic pass.
//
// RESPONSIBILITIES
//   Walks only the top-level declaration sections (DEFINE, CREATE,
//   DECLARE, FUNCTION) and registers every symbol into the symbol
//   table BEFORE any body is type-checked.
//
//   This two-phase approach means:
//   - Functions may call other functions defined later in the file
//   - CREATE can reference a DEFINE that appears before or after it
//   - The type checker always finds symbols already present
//
// WHAT IT REGISTERS
//   DEFINE  → SymbolKind::DeviceType
//   CREATE  → SymbolKind::Device
//   DECLARE → SymbolKind::Variable or SymbolKind::Constant
//   FUNCTION→ SymbolKind::Function (params + return type as Void
//              until the type checker fills it in)
//
// WHAT IT DOES NOT DO
//   Does not check bodies, types, or expressions.
//   Does not validate qualifier/interface combinations.
//   Does not touch RUN section.
//   Pin uniqueness is left for device_checker.

use crate::ast::*;
use crate::semantic::diagnostic::{Diagnostic, DiagnosticKind};
use crate::semantic::symbol_table::{
    ParamInfo, ParamKind, Symbol, SymbolKind, DeviceState, SymbolTable,
};

pub struct DeclarationCollector<'a> {
    symbols:     &'a mut SymbolTable,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> DeclarationCollector<'a> {
    pub fn new(symbols: &'a mut SymbolTable) -> Self {
        DeclarationCollector { symbols, diagnostics: Vec::new() }
    }

    pub fn collect(mut self, program: &Program) -> Vec<Diagnostic> {
        // Register builtins before collecting user declarations so
        // CALLs to builtins resolve in the symbol table.
        self.register_builtins();

        // Order matters: DEFINE before CREATE (CREATE references DEFINE),
        // DECLARE and FUNCTION can be in any order relative to each other.
        for item in &program.defines   { self.collect_define(item);   }
        for item in &program.creates   { self.collect_create(item);   }
        for item in &program.declares  { self.collect_declare(item);  }
        for func in &program.functions { self.collect_function(func); }

        let mut all = self.diagnostics;
        all.extend(self.symbols.take_diagnostics());
        all
    }

    fn register_builtins(&mut self) {
        use crate::semantic::symbol_table::{ParamInfo, ParamKind, Symbol};
        use crate::lexer::token::{Ident, Span};

        // Helper: build a data-only param
        let p = |name: &str, ty: Type| -> ParamInfo {
            ParamInfo {
                name: Ident::new(name, Span::synthetic(
                    std::sync::Arc::new("<builtin>".into())
                )),
                kind: ParamKind::Data(ty),
            }
        };

        // Helper: register one builtin function
        let mut reg = |key: &str, params: Vec<ParamInfo>, ret: Type| {
            let ident = Ident::new(key, Span::synthetic(
                std::sync::Arc::new("<builtin>".into())
            ));
            self.symbols.insert(Symbol {
                ident:      ident.clone(),
                kind:       SymbolKind::Function {
                    params,
                    return_type: ret,
                },
                ty:         Type::Void,
                defined_at: ident.span.clone(),
            });
        };

        // ── §13.1 Mathematical ────────────────────────────────────
        reg("abs",   vec![p("value", Type::Decimal)],  Type::Decimal);
        reg("min",   vec![p("a", Type::Decimal), p("b", Type::Decimal)], Type::Decimal);
        reg("max",   vec![p("a", Type::Decimal), p("b", Type::Decimal)], Type::Decimal);
        reg("clamp", vec![
            p("value",   Type::Decimal),
            p("min_val", Type::Decimal),
            p("max_val", Type::Decimal),
        ], Type::Decimal);
        reg("map", vec![
            p("value",     Type::Decimal),
            p("from_low",  Type::Decimal),
            p("from_high", Type::Decimal),
            p("to_low",    Type::Decimal),
            p("to_high",   Type::Decimal),
        ], Type::Decimal);

        // ── §13.2 Type conversion ─────────────────────────────────
        reg("to_integer",    vec![p("value", Type::Decimal)],     Type::Integer);
        reg("to_decimal",    vec![p("value", Type::Integer)],     Type::Decimal);
        reg("to_percentage", vec![p("value", Type::Decimal)],     Type::Percentage);
        reg("to_string",     vec![p("value", Type::Decimal)],     Type::String);

        // ── §13.3 String operations ───────────────────────────────
        reg("length",   vec![p("value", Type::String)],           Type::Integer);
        reg("includes", vec![p("haystack", Type::String), p("needle", Type::String)], Type::Boolean);

        // ── §13.4 Array operations ────────────────────────────────
        reg("add",    vec![p("array", Type::Void), p("value", Type::Void)], Type::Void);
        reg("remove", vec![p("array", Type::Void), p("index", Type::Integer)], Type::Void);
    }

    // ── DEFINE ───────────────────────────────────────────────────

    fn collect_define(&mut self, item: &DefineItem) {
        self.symbols.insert(Symbol {
            ident:      item.name.clone(),
            kind:       SymbolKind::DeviceType { spec: item.spec.clone() },
            ty:         Type::Device(item.name.clone()),
            defined_at: item.span.clone(),
        });
    }

    // ── CREATE ───────────────────────────────────────────────────

    fn collect_create(&mut self, item: &CreateItem) {
        // Verify the device type exists
        if self.symbols.lookup(&item.device_type.key).is_none() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticKind::UndefinedVariable { name: item.device_type.original.clone() },
                item.device_type.span.clone(),
                format!(
                    "Device type '{}' is not defined. \
                     Add 'DEFINE {} AS ...' before this CREATE.",
                    item.device_type.original,
                    item.device_type.original,
                ),
                None,
            ));
        }

        self.symbols.insert(Symbol {
            ident:      item.instance_name.clone(),
            kind:       SymbolKind::Device {
                device_type: item.device_type.clone(),
                state:       DeviceState::Available,
            },
            ty:         Type::Device(item.device_type.clone()),
            defined_at: item.span.clone(),
        });
    }

    // ── DECLARE ──────────────────────────────────────────────────

    fn collect_declare(&mut self, item: &DeclareItem) {
        match &item.kind {
            DeclareKind::Variable(v) => {
                let ty = if let Some(size) = v.array_size {
                    Type::Array(Box::new(v.ty.to_type()), size)
                } else {
                    v.ty.to_type()
                };
                self.symbols.insert(Symbol {
                    ident:      v.name.clone(),
                    kind:       SymbolKind::Variable { mutable: true, array_size: v.array_size },
                    ty,
                    defined_at: v.span.clone(),
                });
            }
            DeclareKind::Constant(c) => {
                self.symbols.insert(Symbol {
                    ident:      c.name.clone(),
                    kind:       SymbolKind::Constant,
                    ty:         c.ty.to_type(),
                    defined_at: c.span.clone(),
                });
            }
        }
    }

    // ── FUNCTION ─────────────────────────────────────────────────

    fn collect_function(&mut self, func: &FunctionDef) {
        // Build ParamInfo list from AST params
        let params: Vec<ParamInfo> = func.params.iter().map(|p| match p {
            Param::Data { ty, name, .. } => ParamInfo {
                name: name.clone(),
                kind: ParamKind::Data(ty.to_type()),
            },
            Param::Device { ownership, device_ty, name, .. } => ParamInfo {
                name: name.clone(),
                kind: ParamKind::Device {
                    ownership:   ownership.clone(),
                    device_type: device_ty.clone(),
                },
            },
        }).collect();

        // Return type is Void until the type checker fills it in
        self.symbols.insert(Symbol {
            ident:      func.name.clone(),
            kind:       SymbolKind::Function {
                params,
                return_type: Type::Void,
            },
            ty:         Type::Void,
            defined_at: func.span.clone(),
        });
    }
}

#[cfg(test)]
mod declaration_collector_tests {
    use super::*;
    use std::sync::Arc;
    use crate::lexer::token::{Ident, Span};

    fn span() -> Span { Span::new(Arc::new("test.fe".into()), 1, 1, 1) }
    fn ident(s: &str) -> Ident { Ident::new(s, span()) }

    #[test]
    fn collects_define_as_device_type() {
        let mut st = SymbolTable::new();
        let program = Program {
            config: None,
            defines: vec![DefineItem {
                name: ident("Button"),
                spec: DeviceSpec::Simple(InterfaceSpec {
                    interface: InterfaceType::Input,
                    qualifier: None,
                    span: span(),
                }),
                span: span(),
            }],
            creates: vec![],
            declares: vec![],
            functions: vec![],
            run: RunSection { items: vec![], span: span() },
            span: span(),
        };
        let errs = DeclarationCollector::new(&mut st).collect(&program);
        assert!(errs.is_empty());
        assert!(matches!(
            st.lookup("button").map(|s| &s.kind),
            Some(SymbolKind::DeviceType { .. })
        ));
    }

    #[test]
    fn collects_variable_declaration() {
        let mut st = SymbolTable::new();
        let program = Program {
            config: None,
            defines: vec![],
            creates: vec![],
            declares: vec![DeclareItem {
                kind: DeclareKind::Variable(VariableDecl {
                    ty:         DataType::Integer,
                    array_size: None,
                    name:       ident("counter"),
                    init:       Some(InitExpr::Single(Literal::Int(0))),
                    span:       span(),
                }),
                span: span(),
            }],
            functions: vec![],
            run: RunSection { items: vec![], span: span() },
            span: span(),
        };
        let errs = DeclarationCollector::new(&mut st).collect(&program);
        assert!(errs.is_empty());
        let sym = st.lookup("counter").unwrap();
        assert_eq!(sym.ty, Type::Integer);
    }

    #[test]
    fn create_without_define_is_error() {
        let mut st = SymbolTable::new();
        let program = Program {
            config: None,
            defines: vec![],
            creates: vec![CreateItem {
                device_type:   ident("Ghost"),
                instance_name: ident("g"),
                pins:          PinSpec::Single { pin: 1, span: span() },
                pull:          None,
                init:          None,
                span:          span(),
            }],
            declares: vec![],
            functions: vec![],
            run: RunSection { items: vec![], span: span() },
            span: span(),
        };
        let errs = DeclarationCollector::new(&mut st).collect(&program);
        assert!(errs.iter().any(|e| e.is_error()));
    }
}