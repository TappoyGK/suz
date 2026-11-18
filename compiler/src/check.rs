//! ASTの名前・型・値と制御の条件を検査し、C生成へ渡すIRを作る。具体的な判定は子モジュールに分担する。
//!
//! 利用元: [`crate::analyze`]。

mod arena;
mod closures;
mod constants;
mod control;
mod declarations;
mod diagnostics;
mod errors;
pub(crate) mod exact;
mod expression;
mod ffi;
mod function_values;
mod functions;
mod generic;
pub(crate) mod ir;
mod matches;
mod ranges;
mod scope;
mod statements;
mod text;
mod text_parse;
mod text_structure;
mod writes;

use crate::{Diagnostic, ast::Program};
pub(crate) use ir::{
    CheckedExpression, CheckedHandler, CheckedProgram, CheckedStatement, IntegerOperation, Value,
};

pub(crate) fn check(program: &Program) -> Result<CheckedProgram, Diagnostic> {
    functions::check(program)
}

#[cfg(test)]
#[path = "check/tests/bools.rs"]
mod bool_tests;
#[cfg(test)]
#[path = "check/tests/chains.rs"]
mod chain_tests;
#[cfg(test)]
#[path = "check/tests/comparisons.rs"]
mod comparison_tests;
#[cfg(test)]
#[path = "check/tests/floats.rs"]
mod float_tests;
#[cfg(test)]
#[path = "check/tests/branches.rs"]
mod if_tests;
#[cfg(test)]
#[path = "check/tests/logical.rs"]
mod logical_tests;
#[cfg(test)]
#[path = "check/tests/loops.rs"]
mod loop_tests;
#[cfg(test)]
#[path = "check/tests/bindings.rs"]
mod tests;

#[cfg(test)]
#[path = "check/tests/generic.rs"]
mod generic_tests;
