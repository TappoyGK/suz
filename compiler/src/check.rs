mod constants;
mod control;
mod declarations;
mod diagnostics;
pub(crate) mod exact;
mod expression;
mod functions;
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
