//! if式の条件と各分岐の値を検査し、分岐の結果型を揃える。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[branch.expression]`、`[if.condition]`。

use super::super::ir::{CheckedExpression, Value};
use super::super::matches;
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
    if expected.is_none() {
        super::super::function_values::require_context(expr, bindings)?;
    }
    // 最外側だけの拡大は分岐の外で行い、元の非nil型を寿命検査まで保つ。
    let inferred = operand_type(expr, bindings);
    let expected = match (inferred, expected) {
        (Some(from), Some(to)) if from.lifts_to(to) => Some(from),
        _ => expected,
    };
    Ok(match &expr.kind {
        ExpressionKind::Match { target, arms } => {
            let target = crate::check::exact::use_value(target, bindings)?;
            let patterns = matches::patterns(target.ty, arms, expr.location, bindings)?;
            let ty = expected
                .or_else(|| operand_type(expr, bindings))
                .unwrap_or_else(|| default_type(expr, bindings));
            if ty == Type::Void {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("branch.expression", "value branches cannot return void"),
                ));
            }
            let mut checked = Vec::new();
            let base_facts = bindings.array_bounds.facts.borrow().clone();
            let mut exits = Vec::new();
            for (arm, pattern) in arms.iter().zip(patterns.values) {
                *bindings.array_bounds.facts.borrow_mut() = base_facts.clone();
                if let Some(pat) = &pattern {
                    super::super::arrays::bounds::record_match_arm(
                        &target,
                        pat,
                        arm.location,
                        bindings,
                    );
                }
                checked.push((pattern, super::coerced(&arm.body, Some(ty), bindings)?));
                exits.push(bindings.array_bounds.facts.borrow().clone());
            }
            *bindings.array_bounds.facts.borrow_mut() =
                super::super::arrays::facts::Facts::join_all(&exits);
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
                .unwrap_or_else(|| default_type(expr, bindings));
            if ty == Type::Void {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("branch.expression", "value branches cannot return void"),
                ));
            }
            let mut checked = Vec::new();
            let mut fallthrough = bindings.array_bounds.facts.borrow().clone();
            let mut exits = Vec::new();
            for (condition, value) in branches {
                *bindings.array_bounds.facts.borrow_mut() = fallthrough;
                let (checked_condition, true_facts, false_facts) =
                    super::super::arrays::bounds::eval_condition(condition, bindings)?;
                if checked_condition.ty != Type::Bool {
                    return Err(condition.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "if.condition",
                            "ifexpr and elif conditions must be bool"
                        ),
                    ));
                }
                *bindings.array_bounds.facts.borrow_mut() = true_facts;
                checked.push((
                    checked_condition,
                    super::coerced(value, Some(ty), bindings)?,
                ));
                exits.push(bindings.array_bounds.facts.borrow().clone());
                fallthrough = false_facts;
            }
            *bindings.array_bounds.facts.borrow_mut() = fallthrough;
            let checked_otherwise = super::coerced(otherwise, Some(ty), bindings)?;
            exits.push(bindings.array_bounds.facts.borrow().clone());
            *bindings.array_bounds.facts.borrow_mut() =
                super::super::arrays::facts::Facts::join_all(&exits);
            CheckedExpression {
                ty,
                value: Value::If {
                    branches: checked,
                    otherwise: Box::new(checked_otherwise),
                },
            }
        }
        _ => unreachable!(),
    })
}
