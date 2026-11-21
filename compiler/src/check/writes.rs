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
    scope: &'a super::scope::Scope,
}
impl Visitor for Writes<'_> {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        if let ExpressionKind::MethodCall {
            target,
            mutable: true,
            ..
        } = &expr.kind
            && let Some((name, fields)) = super::methods::path(target)
            && self.scope.get(name).is_some_and(|binding| {
                !super::arena::indirect_path(binding.ty, &fields, self.scope)
            })
            && !super::expression::operand_type(target, self.scope)
                .is_some_and(|ty| ty.pointee().is_some())
            && name == self.name
            && (fields.starts_with(self.fields) || self.fields.starts_with(&fields))
        {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "assign.rhs-write",
                    "assignment RHS calls a # method on the assignment target"
                ),
            ));
        }
        // 本体へ同じ変数が引き継がれるのは、参照キャプチャーの場合だけ。
        if let ExpressionKind::Closure(closure) = &expr.kind
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
    scope: &super::scope::Scope,
) -> Result<(), Diagnostic> {
    Writes {
        name,
        fields,
        scope,
    }
    .expression(value)
}

/// 配列の添字の値は経路の比較に使わない。参照変数そのものへの再代入とは区別する。
#[derive(PartialEq, Eq)]
enum ArrayStep {
    Field(String),
    Index,
}
fn array_path(expr: &Expression) -> Option<(&str, Vec<ArrayStep>)> {
    match &expr.kind {
        ExpressionKind::Group(v) => array_path(v),
        ExpressionKind::Name(name) => Some((name, Vec::new())),
        ExpressionKind::Field { target, name } => {
            let (root, mut path) = array_path(target)?;
            path.push(ArrayStep::Field(name.clone()));
            Some((root, path))
        }
        ExpressionKind::Slice { target, .. } => {
            let (root, mut path) = array_path(target)?;
            path.push(ArrayStep::Index);
            Some((root, path))
        }
        _ => None,
    }
}
struct ArrayWrites<'a> {
    name: &'a str,
    path: Vec<ArrayStep>,
}
impl ArrayWrites<'_> {
    fn target(&self, target: &Expression) -> Result<(), Diagnostic> {
        if let Some((name, path)) = array_path(target)
            && name == self.name
            && path.contains(&ArrayStep::Index)
            && (path.starts_with(&self.path) || self.path.starts_with(&path))
        {
            return Err(target.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "assign.rhs-write",
                    "assignment RHS writes to the same array element path"
                ),
            ));
        }
        Ok(())
    }
}
impl Visitor for ArrayWrites<'_> {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        if let ExpressionKind::Closure(closure) = &expr.kind
            && !closure
                .captures
                .iter()
                .any(|capture| capture.name == self.name)
        {
            return Ok(());
        }
        if let ExpressionKind::MethodCall {
            target,
            mutable: true,
            ..
        } = &expr.kind
        {
            self.target(target)?;
        }
        visit::walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Store { target, .. } = statement {
            self.target(target)?;
        }
        visit::walk_statement(self, statement)
    }
}
pub(super) fn array(value: &Expression, target: &Expression) -> Result<(), Diagnostic> {
    if let Some((name, path)) = array_path(target)
        && path.contains(&ArrayStep::Index)
    {
        ArrayWrites { name, path }.expression(value)?;
    }
    Ok(())
}
