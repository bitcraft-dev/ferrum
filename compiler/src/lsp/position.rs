// Position -> AST lookups used by hover and completion.

use crate::ast::*;
use crate::lsp::analysis::span_contains;
use lsp_types::Position;

/// Find the innermost `Expr` whose span contains `pos`, walking the
/// whole program. "Innermost" because `visit_expr` walks a node's
/// children before checking the node itself, so a more specific child
/// expression overwrites a broader parent match.
pub fn expr_at(program: &Program, pos: Position) -> Option<Expr> {
    struct Finder {
        pos:   Position,
        found: Option<Expr>,
    }

    impl AstVisitor for Finder {
        fn visit_expr(&mut self, node: &Expr) {
            self.walk_expr(node);
            // Children are visited above via walk_expr and run to
            // completion first, so if one already matched, it's more
            // specific than this (enclosing) node — keep it.
            if self.found.is_none() && span_contains(&node.span, self.pos) {
                self.found = Some(node.clone());
            }
        }
    }

    let mut finder = Finder { pos, found: None };
    finder.visit_program(program);
    finder.found
}

/// One completion candidate: the text to insert and a short label
/// for what kind of thing it is (shown in the completion list).
pub struct Candidate {
    pub name: String,
    pub kind: CandidateKind,
}

pub enum CandidateKind {
    DeviceType,
    DeviceInstance,
    Variable,
    Constant,
    Function,
}

/// User-defined names in scope: device types (DEFINE), device
/// instances (CREATE), variables/constants (DECLARE), and functions.
/// Not scope-aware (doesn't distinguish a function-local DECLARE from
/// a program-level one) — good enough for a first completion pass;
/// the type checker still catches anything actually out of scope.
pub fn collect_identifiers(program: &Program) -> Vec<Candidate> {
    let mut out = Vec::new();

    for d in &program.defines {
        out.push(Candidate { name: d.name.original.clone(), kind: CandidateKind::DeviceType });
    }
    for c in &program.creates {
        out.push(Candidate { name: c.instance_name.original.clone(), kind: CandidateKind::DeviceInstance });
    }
    for decl in &program.declares {
        match &decl.kind {
            DeclareKind::Variable(v) => out.push(Candidate {
                name: v.name.original.clone(),
                kind: CandidateKind::Variable,
            }),
            DeclareKind::Constant(c) => out.push(Candidate {
                name: c.name.original.clone(),
                kind: CandidateKind::Constant,
            }),
        }
    }
    for f in &program.functions {
        out.push(Candidate { name: f.name.original.clone(), kind: CandidateKind::Function });
        // Function parameters are in scope inside the function body but
        // were previously never offered — a student writing the body of
        // `all_off` got no completion for its own `red`/`amber`/`green`
        // parameters. Not scope-aware (they leak into completion outside
        // the function too), consistent with this pass's other names.
        for p in &f.params {
            match p {
                Param::Data { name, .. } => out.push(Candidate {
                    name: name.original.clone(),
                    kind: CandidateKind::Variable,
                }),
                Param::Device { name, .. } => out.push(Candidate {
                    name: name.original.clone(),
                    kind: CandidateKind::DeviceInstance,
                }),
            }
        }
    }

    out
}

/// A one-line hover description for a user-defined name, looked up by
/// its lowercase `key`. Covers device types, device instances,
/// variables, constants, functions, and function parameters — the
/// places a bare `Ident` (not an `Expr`) appears, which `expr_at`
/// can't reach. Not scope-aware: first declaration matching the key
/// wins, matching `collect_identifiers`.
pub fn describe_name(program: &Program, key: &str) -> Option<String> {
    for d in &program.defines {
        if d.name.key == key {
            return Some(format!("`{}` — **device type**", d.name.original));
        }
    }
    for c in &program.creates {
        if c.instance_name.key == key {
            return Some(format!(
                "`{}`: **{}** — device instance",
                c.instance_name.original, c.device_type.original
            ));
        }
    }
    for decl in &program.declares {
        match &decl.kind {
            DeclareKind::Variable(v) if v.name.key == key => {
                let arr = match v.array_size {
                    Some(n) => format!("[{}]", n),
                    None    => String::new(),
                };
                return Some(format!("`{}`: **{:?}{}** — variable", v.name.original, v.ty, arr));
            }
            DeclareKind::Constant(c) if c.name.key == key => {
                return Some(format!("`{}`: **{:?}** — constant", c.name.original, c.ty));
            }
            _ => {}
        }
    }
    for f in &program.functions {
        if f.name.key == key {
            let ret = match &f.return_type {
                Some(t) => format!(" → **{:?}**", t),
                None    => String::new(),
            };
            return Some(format!("`{}` — **function**{}", f.name.original, ret));
        }
        for p in &f.params {
            match p {
                Param::Data { name, ty, .. } if name.key == key => {
                    return Some(format!("`{}`: **{:?}** — parameter", name.original, ty));
                }
                Param::Device { name, device_ty, ownership, .. } if name.key == key => {
                    return Some(format!(
                        "`{}`: **{}** — {:?} device parameter",
                        name.original, device_ty.original, ownership
                    ));
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::analysis::analyse_text;

    const SRC: &str = "\
CONFIG {
    TARGET = \"microbit_v2\"
}

DEFINE Led AS OUTPUT

CREATE Led status ON PIN 13

DECLARE Integer counter INIT 0

RUN {
    LOOP {
        TURN status HIGH
        counter = counter + 1
        DELAY 500ms
    }
}
";

    #[test]
    fn expr_at_finds_innermost_ident_not_enclosing_binop() {
        let program = analyse_text(SRC, "t.fe").program.expect("should parse and analyse");
        // Line 14 (1-indexed) -> 0-indexed line 13. RHS "counter" is at
        // columns 19-25 (1-indexed) -> chars 18-25 (0-indexed).
        let expr = expr_at(&program, Position { line: 13, character: 20 })
            .expect("expected to find the RHS ident");
        match expr.kind {
            ExprKind::Ident(ident) => assert_eq!(ident.original, "counter"),
            other => panic!("expected Ident, got {:?}", other),
        }
        assert_eq!(expr.ty, Some(Type::Integer));
    }

    #[test]
    fn expr_at_returns_none_outside_any_expression() {
        let program = analyse_text(SRC, "t.fe").program.expect("should parse and analyse");
        // Column 0 on the LOOP line — inside whitespace, not an expression.
        assert!(expr_at(&program, Position { line: 11, character: 0 }).is_none());
    }

    #[test]
    fn collect_identifiers_includes_every_declared_name() {
        let program = analyse_text(SRC, "t.fe").program.expect("should parse and analyse");
        let names: Vec<String> = collect_identifiers(&program).into_iter().map(|c| c.name).collect();
        assert!(names.contains(&"Led".to_string()));
        assert!(names.contains(&"status".to_string()));
        assert!(names.contains(&"counter".to_string()));
    }

    const SRC_WITH_FN: &str = "\
CONFIG {
    TARGET = \"microbit_v2\"
}

DEFINE Led AS OUTPUT

CREATE Led status ON PIN 13

FUNCTION blink BORROW Led: light, Integer: times {
    TURN light HIGH
}

RUN {
    LOOP {
        CALL blink BORROW status, 3
    }
}
";

    #[test]
    fn collect_identifiers_includes_function_parameters() {
        let program = analyse_text(SRC_WITH_FN, "t.fe").program.expect("should parse and analyse");
        let names: Vec<String> = collect_identifiers(&program).into_iter().map(|c| c.name).collect();
        assert!(names.contains(&"light".to_string()), "device param should be offered");
        assert!(names.contains(&"times".to_string()), "data param should be offered");
    }

    #[test]
    fn describe_name_resolves_device_instance_and_param() {
        let program = analyse_text(SRC_WITH_FN, "t.fe").program.expect("should parse and analyse");
        // Device instance carries its DEFINEd type.
        let status = describe_name(&program, "status").expect("status should resolve");
        assert!(status.contains("Led"), "got: {status}");
        // Device parameter reports its type and ownership.
        let light = describe_name(&program, "light").expect("light should resolve");
        assert!(light.contains("Led"), "got: {light}");
        // Data parameter reports its type.
        let times = describe_name(&program, "times").expect("times should resolve");
        assert!(times.contains("Integer"), "got: {times}");
        // Unknown name resolves to nothing.
        assert!(describe_name(&program, "nonexistent").is_none());
    }
}
