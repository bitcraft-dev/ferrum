// ferrum/compiler/src/ast/mod.rs
//
// Re-exports everything from nodes.rs so other modules can write:
//
//   use crate::ast::*;
//
// and have every AST type available without qualifying paths.

mod nodes;
pub use nodes::*;