//! 値ブロックの文と末尾の値を検査し、外側の束縛の参照を捕捉する。型のヒント取得と実際の検査を分ける。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[block.value]`。

use super::{
    super::ir::{CheckedExpression, Value},
    super::statements,
};
use crate::check::scope::Scope;
use crate::{
    Diagnostic,
    ast::{
        Expression, ExpressionKind, Statement, Type,
        visit::{self, Visitor},
    },
};
use std::collections::BTreeSet;

// 各値ブロックは独立したC関数になるため、ローカル番号は捕捉した番号とだけ区別できればよい。
fn prefix(
    input: &[Statement],
    bindings: &mut Scope,
) -> Result<(Vec<super::super::ir::CheckedStatement>, Vec<String>), Diagnostic> {
    let mut next_id = bindings
        .locals
        .values()
        .map(|b| b.id + 1)
        .max()
        .unwrap_or(0);
    let mut declared = Vec::new();
    let checked = statements::check_statements(input, bindings, &mut next_id, 0, &mut declared)?;
    Ok((checked, declared))
}

pub(super) fn operand_type(
    input: &[Statement],
    value: &Expression,
    bindings: &Scope,
) -> Option<Type> {
    super::operand_type(value, &local_hints(input, bindings))
}

pub(super) fn has_float_source(input: &[Statement], value: &Expression, bindings: &Scope) -> bool {
    super::has_float_source(value, &local_hints(input, bindings))
}

pub(in crate::check) fn local_hints(input: &[Statement], bindings: &Scope) -> Scope {
    // ここではローカルの型だけを調べる。文の検査を呼ぶと、入れ子の引数全体を何度も構築・再検査してしまう。
    // 名前と値はcheck()で一度だけ検査し、型のヒント取得ではエラーを報告しない。
    let mut local = bindings.clone();
    for statement in input {
        if let Statement::Binding {
            name,
            annotation,
            initializer,
            location,
            dimensions,
            ..
        } = statement
        {
            let ty = annotation
                .and_then(|ty| bindings.resolve_type_at(ty, *location).ok())
                .or_else(|| super::operand_type(initializer, &local))
                .unwrap_or_else(|| super::default_type(initializer, &local));
            local
                .locals
                .insert(name.clone(), statements::Binding::hint(ty, *location));
            for dimension in dimensions {
                local.locals.insert(
                    dimension.name.clone(),
                    statements::Binding::hint(
                        crate::ast::Type::Integer(crate::ast::IntegerType::Len),
                        dimension.location,
                    ),
                );
            }
        }
    }
    local
}

pub(super) fn check(
    input: &[Statement],
    value: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let mut references = References(BTreeSet::new());
    references.scope(input, Some(value))?;
    let captures = bindings
        .locals
        .iter()
        .filter(|(name, _)| references.0.contains(name.as_str()))
        .map(|(_, b)| (b.id, b.ty, b.mutable))
        .collect();
    let mut local = bindings.clone();
    let (statements, declared) = prefix(input, &mut local)?;
    let checked_value = super::coerced(value, expected, &local)?;
    if checked_value.ty == Type::Void {
        return Err(value.location.error(
            crate::ErrorKind::InvalidSource,
            crate::diagnostic!(
                "block.value",
                "value block must end in a non-void expression"
            ),
        ));
    }
    let value = checked_value;
    statements::finish_scope(&mut local, declared)?;
    for (name, binding) in bindings.locals.iter() {
        if local[name].assigned.get() {
            binding.assigned.set(true);
        }
    }
    *bindings.array_bounds.facts.borrow_mut() = local.array_bounds.facts.into_inner();
    Ok(CheckedExpression {
        ty: value.ty,
        value: Value::Block {
            statements,
            value: Box::new(value),
            captures,
        },
    })
}

// シャドーイングを禁止しているため、外側の名前への参照は、入れ子の値ブロック内でも同じ束縛を指す。
struct References(BTreeSet<String>);
impl Visitor for References {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        if let ExpressionKind::Closure(closure) = &expr.kind {
            self.0
                .extend(closure.captures.iter().map(|c| c.name.clone()));
            return Ok(()); // 本体の名前は外側の値ブロックの参照ではない。
        }
        if let ExpressionKind::Name(name) | ExpressionKind::Call { name, .. } = &expr.kind {
            self.0.insert(name.clone());
        }
        visit::walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Assign { name, .. } = statement {
            self.0.insert(name.clone());
        }
        visit::walk_statement(self, statement)
    }
}

/// 非脱出のerrorハンドラーを補助関数にする際に、外側から読む束縛だけを捕捉する。
pub(in crate::check) fn handler_captures(
    call: &Expression,
    arms: &[crate::ast::MatchArm<crate::ast::ErrorArmBody>],
    scope: &Scope,
) -> Vec<(usize, Type, bool)> {
    let mut references = References(BTreeSet::new());
    references.expression(call).unwrap();
    for arm in arms {
        match &arm.body {
            crate::ast::ErrorArmBody::Value(value) => references.expression(value).unwrap(),
            crate::ast::ErrorArmBody::Statements { body, .. } => {
                references.scope(body, None).unwrap()
            }
        }
    }
    scope
        .locals
        .iter()
        .filter(|(name, _)| references.0.contains(name.as_str()))
        .map(|(_, b)| (b.id, b.ty, b.mutable))
        .collect()
}
