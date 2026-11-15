use super::super::ir::{CheckedExpression, Value};
use super::super::matches;
use super::expression;
use super::{default_type, operand_type};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

pub(super) fn check(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    Ok(match &expr.kind {
        ExpressionKind::Match { target, arms } => {
            let target = crate::check::exact::use_value(target, bindings)?;
            let patterns = matches::patterns(target.ty, arms, expr.location, bindings)?;
            let ty = expected
                .or_else(|| operand_type(expr, bindings))
                .unwrap_or_else(|| default_type(expr));
            if ty == Type::Void {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "value branches cannot return void [branch.expression]",
                ));
            }
            let mut checked = Vec::new();
            for (arm, pattern) in arms.iter().zip(patterns.values) {
                checked.push((pattern, super::coerced(&arm.body, Some(ty), bindings)?));
            }
            CheckedExpression {
                ty,
                value: Value::Match {
                    target: Box::new(target),
                    arms: checked,
                    pattern_warnings: patterns.warnings,
                },
            }
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            let ty = expected
                .or_else(|| operand_type(expr, bindings))
                .unwrap_or_else(|| default_type(expr));
            if ty == Type::Void {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "value branches cannot return void [branch.expression]",
                ));
            }
            let mut checked = Vec::new();
            for (condition, value) in branches {
                let checked_condition = expression(condition, None, bindings)?;
                if checked_condition.ty != Type::Bool {
                    return Err(condition.location.error(
                        ErrorKind::InvalidSource,
                        "ifexpr and elif conditions must be bool [if.condition]",
                    ));
                }
                checked.push((
                    checked_condition,
                    super::coerced(value, Some(ty), bindings)?,
                ));
            }
            CheckedExpression {
                ty,
                value: Value::If {
                    branches: checked,
                    otherwise: Box::new(super::coerced(otherwise, Some(ty), bindings)?),
                },
            }
        }
        _ => unreachable!(),
    })
}
