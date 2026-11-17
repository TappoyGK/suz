//! 代入の右辺が代入先と重なる場所へ書き込まないか、式の内部を走査して検査する。
//!
//! 利用元: [`crate::check::statements`]。
//! 主な対象規則: `[assign.rhs-write]`。

use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Expression, ExpressionKind, Statement,
        visit::{self, Visitor},
    },
};

struct Writes<'a> {
    name: &'a str,
    fields: &'a [String],
    global: bool,
}
impl Visitor for Writes<'_> {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        // 値キャプチャーは外側の変数を書き換えない。本体から同じ名前で書けるのは大域変数だけ。
        if matches!(expr.kind, ExpressionKind::Closure(_)) && !self.global {
            return Ok(());
        }
        visit::walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Assign {
            name,
            fields,
            location,
            ..
        } = statement
            && name == self.name
            && (fields.starts_with(self.fields) || self.fields.starts_with(fields))
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
pub(super) fn check(
    value: &Expression,
    name: &str,
    fields: &[String],
    global: bool,
) -> Result<(), Diagnostic> {
    Writes {
        name,
        fields,
        global,
    }
    .expression(value)
}
