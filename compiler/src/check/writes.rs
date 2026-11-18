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
        // 本体へ同じ変数が引き継がれるのは、参照キャプチャーか大域変数の場合だけ。
        if let ExpressionKind::Closure(closure) = &expr.kind
            && !self.global
            && !closure
                .captures
                .iter()
                .any(|c| c.by_reference && c.name == self.name)
        {
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
