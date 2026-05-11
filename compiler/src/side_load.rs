// ================================================================
// FERRUM COMPILER — REMAINING MODULES
// ferrum_compiler_remaining.rs
//
// This file contains all remaining compiler modules in order.
// Each module is clearly delimited by:
//
//   ╔══ MODULE: path/to/module.rs ══╗
//   ...
//   ╚══ END MODULE: path/to/module.rs ══╝
//
// Modules contained:
//   1.  semantic/declaration_collector.rs
//   2.  semantic/ownership.rs
//   3.  semantic/device_checker.rs
//   4.  semantic/mod.rs
//   5.  diagnostics/reporter.rs
//   6.  main.rs
//   7.  Cargo.toml
// ================================================================







// ╔══ FILE: ferrum/compiler/Cargo.toml ══╗
//
// [package]
// name        = "ferrum"
// version     = "0.1.0"
// edition     = "2021"
// description = "Ferrum embedded DSL compiler — Rust codegen for .fe source files"
// license     = "MIT"
//
// [[bin]]
// name = "ferrum"
// path = "src/main.rs"
//
// [dependencies]
// # No external dependencies for the core compiler pipeline.
// # The lexer, parser, semantic pass, and AST are all pure Rust.
//
// [dev-dependencies]
// # No additional test dependencies beyond std.
//
// ╚══ END FILE: ferrum/compiler/Cargo.toml ══╝


// ╔══ FILE: ferrum/compiler/src/diagnostics/mod.rs ══╗
//
// pub mod reporter;
//
// ╚══ END FILE: ferrum/compiler/src/diagnostics/mod.rs ══╝


// ================================================================
// COMPLETE MODULE INVENTORY
// ================================================================
//
// ferrum/compiler/
// ├── Cargo.toml
// └── src/
//     ├── main.rs
//     ├── lexer/
//     │   ├── mod.rs          [previously written: lexer_mod.rs]
//     │   ├── token.rs        [previously written: token.rs]
//     │   └── lexer.rs        [previously written: lexer.rs]
//     ├── ast/
//     │   ├── mod.rs          [previously written: ast_mod.rs]
//     │   └── nodes.rs        [previously written: ast_nodes.rs]
//     ├── parser/
//     │   ├── mod.rs          ← add: pub mod parser;
//     │   └── parser.rs       [previously written: parser.rs]
//     ├── semantic/
//     │   ├── mod.rs          [this file]
//     │   ├── diagnostic.rs   [previously written: diagnostic.rs]
//     │   ├── symbol_table.rs [previously written: symbol_table.rs]
//     │   ├── type_checker.rs [previously written: type_checker.rs]
//     │   ├── declaration_collector.rs  [this file]
//     │   ├── ownership.rs    [this file]
//     │   └── device_checker.rs [this file]
//     └── diagnostics/
//         ├── mod.rs          [this file]
//         └── reporter.rs     [this file]
//
// ================================================================