//! 代入の右辺が代入先と重なる場所へ書き込まないか、式の内部を走査して検査する。
//!
//! 利用元: [`crate::check::statements`]。
//! 主な対象規則: `[assign.rhs-write]`。

use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Expression, Statement,
        visit::{self, Visitor},
    },
};

struct Writes<'a>(&'a str, &'a [String]);
impl Visitor for Writes<'_> {
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Assign {
            name,
            fields,
            location,
            ..
        } = statement
            && name == self.0
            && (fields.starts_with(self.1) || self.1.starts_with(fields))
        {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "assign.rhs-write",
                    "assignment RHS writes to the assignment target"
                ),
            ));
        }
        visit::walk_statement(self, statement)
    }
}
pub(super) fn check(value: &Expression, name: &str, fields: &[String]) -> Result<(), Diagnostic> {
    Writes(name, fields).expression(value)
}
