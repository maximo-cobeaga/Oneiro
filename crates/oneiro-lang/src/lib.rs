//! Oneiro language front end: lexer, parser, AST and diagnostics.

pub mod ast;
pub mod diag;
pub mod lexer;
pub mod parser;
pub mod suggest;

pub use ast::*;
pub use diag::{to_json_array, Diagnostic, Pos, Severity};
pub use parser::parse;
