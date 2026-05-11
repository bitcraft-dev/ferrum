// ferrum/compiler/src/lexer/mod.rs
//
// Public surface of the lexer module.
//
// Other modules import from here:
//
//   use crate::lexer::{lex, LexResult, LexError};
//   use crate::lexer::token::{Span, Ident, Token, SpannedToken};
//
// Span and Ident are defined in token.rs and are the canonical
// definitions used by every other module in the compiler.
// They are NOT re-exported at the crate root to keep the import
// path explicit: crate::lexer::token::Span makes provenance clear.

pub mod token;
pub mod lexer;

pub use lexer::{lex, LexError, LexResult};