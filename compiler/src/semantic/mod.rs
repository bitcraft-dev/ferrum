// Semantic pass orchestrator.
//
// Runs all sub-passes in order and collects diagnostics:
//   1. DeclarationCollector  — register all symbols
//   2. TypeChecker           — fill types, validate literals, check expressions
//   3. OwnershipChecker      — enforce GIVE/LEND/BORROW rules
//   4. DeviceChecker         — PIN uniqueness, ambiguous SET
//
// Returns a SemanticResult carrying the annotated program and
// all diagnostics (errors + warnings) from every pass.

pub mod declaration_collector;
pub mod device_checker;
pub mod diagnostic;
pub mod ownership;
pub mod symbol_table;
pub mod type_checker;

use crate::ast::Program;
use diagnostic::{Diagnostic, Severity};
use symbol_table::SymbolTable;

pub struct SemanticResult {
    /// The annotated program — all Expr.ty and return_type slots filled.
    /// None if any error was reported (program is not safe to codegen).
    pub program:     Option<Program>,
    pub diagnostics: Vec<Diagnostic>,
}

impl SemanticResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_error()).collect()
    }

    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.is_warning()).collect()
    }
}

/// Run the full semantic pass on a parsed program.
pub fn analyse(mut program: Program) -> SemanticResult {
    let mut symbols     = SymbolTable::new();
    let mut diagnostics = Vec::new();

    // Pass 1: declaration collection
    let errs = declaration_collector::DeclarationCollector::new(&mut symbols)
        .collect(&program);
    diagnostics.extend(errs);

    // Pass 2: type checking (fills Expr.ty and return_type)
    let errs = type_checker::TypeChecker::new(&mut symbols)
        .check(&mut program);
    diagnostics.extend(errs);

    // Pass 3: ownership checking
    let errs = ownership::OwnershipChecker::new(&mut symbols)
        .check(&mut program);
    diagnostics.extend(errs);

    // Pass 4: device checking
    let errs = device_checker::DeviceChecker::new(&symbols)
        .check(&program);
    diagnostics.extend(errs);

    let has_errors = diagnostics.iter().any(|d| d.is_error());

    SemanticResult {
        program:     if has_errors { None } else { Some(program) },
        diagnostics,
    }
}

#[cfg(test)]
mod semantic_integration_tests {
    use super::*;
    use crate::lexer::lexer::lex;
    use crate::parser::parser::parse;

    fn analyse_src(src: &str) -> SemanticResult {
        let lex_result   = lex(src, "test.fe");
        let parse_result = parse(lex_result.tokens, "test.fe");
        let program      = parse_result.program
            .expect("parse failed in semantic integration test");
        analyse(program)
    }

    #[test]
    fn well_formed_program_has_no_errors() {
        let src = r#"
DEFINE Button AS INPUT
DEFINE Led AS OUTPUT

CONFIG {
    TARGET = "microbit_v2"
    DEBUG = TRUE
}

CREATE Button mode_btn ON PIN 14
CREATE Led status ON PIN 13

DECLARE Boolean auto_mode INIT TRUE

RUN {
    LOOP {
        IF mode_btn IS LOW {
            TURN status HIGH
        } ELSE {
            TURN status LOW
        }
        DELAY 100ms
    }
}
        "#;
        let result = analyse_src(src);
        assert!(
            !result.has_errors(),
            "unexpected errors: {:?}",
            result.errors().iter().map(|e| e.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn undefined_variable_is_reported() {
        let src = r#"
RUN {
    LOOP {
        PRINT unknown_var
    }
}
        "#;
        let result = analyse_src(src);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|e|
            e.message.contains("unknown_var") || e.message.contains("not defined")
        ));
    }

    #[test]
    fn assign_to_constant_is_error() {
        let src = r#"
DECLARE CONSTANT Integer MAX = 100

RUN {
    LOOP {
        MAX = 200
    }
}
        "#;
        let result = analyse_src(src);
        assert!(result.has_errors());
    }

    #[test]
    fn percentage_out_of_range_is_error() {
        let src = r#"
DECLARE Percentage level INIT 105.0

RUN {}
        "#;
        let result = analyse_src(src);
        assert!(result.has_errors());
        assert!(result.errors().iter().any(|e|
            e.message.contains("out of range") || e.message.contains("Percentage")
        ));
    }
}