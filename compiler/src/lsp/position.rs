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
    }

    out
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
}
