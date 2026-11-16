use super::{
    constants, declarations, diagnostics,
    expression::expression,
    ir::{CheckedExpression, Value},
};
use crate::check::generic::domain;
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Location, MatchArm, Type},
};
use std::collections::BTreeSet;

pub(super) struct Patterns {
    pub values: Vec<Option<CheckedExpression>>,
    pub warnings: Vec<Location>,
}

pub(super) fn patterns<T>(
    ty: Type,
    arms: &[MatchArm<T>],
    location: Location,
    bindings: &Scope,
) -> Result<Patterns, Diagnostic> {
    if !(ty.is_integer()
        || matches!(
            ty,
            Type::Exact | Type::Char | Type::Bool | Type::Enum(_) | Type::Symbol | Type::Text
        ))
    {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "match.target",
                "match requires an integer, char, bool, enum, symbol or text target"
            ),
        ));
    }
    let mut seen = vec![
        BTreeSet::new();
        if bindings.generic_check() {
            domain::NUMERIC_COMBINATIONS
        } else {
            1
        }
    ];
    let mut wildcard = false;
    let mut values = Vec::new();
    let mut warnings = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        let Some(pattern) = &arm.pattern else {
            if !(ty.is_integer()
                || matches!(ty, Type::Exact | Type::Char | Type::Symbol | Type::Text))
                || wildcard
                || index + 1 != arms.len()
            {
                return Err(arm.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("match.pattern", "wildcard is allowed only as the final integer, char, symbol or text match arm" ),
                ));
            }
            wildcard = true;
            values.push(None);
            continue;
        };
        let mut names = Vec::new();
        declarations::dependencies(pattern, &mut names)?;
        for (name, location) in names {
            if !bindings
                .get(&name)
                .is_some_and(|binding| binding.constant.is_some())
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "const.expression",
                        "match patterns can reference only constants"
                    ),
                ));
            }
        }
        let checked = if ty == Type::Exact || ty.symbolic() {
            super::exact::comparison_value(pattern, bindings)?
        } else {
            expression(
                pattern,
                Some(if ty == Type::Symbol { Type::Text } else { ty }),
                bindings,
            )?
        };
        if ty == Type::Exact && !matches!(checked.ty, Type::Exact | Type::Integer(_)) {
            return Err(pattern.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("arith.match", "exact match requires integer patterns"),
            ));
        }
        if ty.symbolic() && checked.ty != Type::Exact && !domain::same(ty, checked.ty) {
            return Err(pattern.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "type.expected",
                    "expected {}, found {}",
                    bindings.type_name(ty),
                    bindings.type_name(checked.ty)
                ),
            ));
        }
        if ty.symbolic() && checked.ty != Type::Exact {
            bindings.defer_rule(|i| {
                let (to, from) = (domain::at(ty, i)?, domain::at(checked.ty, i)?);
                (to != from).then(|| {
                    pattern.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "type.expected",
                            "expected {}, found {}",
                            bindings.type_name(to),
                            bindings.type_name(from)
                        ),
                    )
                })
            });
        }
        if ty.symbolic()
            && let Some(n) = super::exact::constant(&checked)
        {
            let fits = domain::judge_one(ty, |t| crate::numeric::Range::point(n).fits_type(t));
            if !fits.possible() {
                return Err(pattern.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("lit.range", "integer pattern is out of range for int"),
                ));
            }
        }
        diagnostics::collect(&checked, &mut warnings);
        // Constant evaluation is a local rule: type-dependent values and
        // duplicate patterns are deferred, without rechecking any source body.
        let mut representative = None;
        let mut first_error = None;
        let mut all_duplicate = true;
        let mut deferred_errors = vec![None; seen.len()];
        for (i, seen) in seen.iter_mut().enumerate() {
            let evaluated = if bindings.generic_check() {
                evaluate_generic_pattern(&checked, pattern, ty, i, bindings)
            } else {
                constants::evaluate(&checked)
            };
            match evaluated {
                Ok(Some(value)) => {
                    let duplicate = !seen.insert(pattern_key(&value));
                    all_duplicate &= duplicate;
                    if duplicate {
                        deferred_errors[i] = Some(arm.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("match.pattern", "duplicate evaluated pattern"),
                        ));
                    }
                    representative.get_or_insert(value);
                }
                Err(error) => {
                    deferred_errors[i] = Some(error.clone());
                    first_error.get_or_insert(error);
                    all_duplicate = false;
                }
                Ok(None) => {
                    all_duplicate = false;
                }
            }
        }
        let mut value = representative.ok_or_else(|| {
            first_error.unwrap_or_else(|| {
                arm.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("const.expression", "match pattern must be constant"),
                )
            })
        })?;
        if all_duplicate {
            return Err(arm.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("match.pattern", "duplicate evaluated pattern"),
            ));
        }
        if bindings.generic_check() {
            bindings.defer_rule(|i| deferred_errors[i].clone());
        }
        if ty == Type::Exact && value.ty != Type::Exact {
            let n = super::exact::constant(&value).ok_or_else(|| {
                arm.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("match.pattern", "exact match requires integer patterns"),
                )
            })?;
            value = CheckedExpression {
                ty: Type::Exact,
                value: Value::Exact(Box::new(super::exact::Exact {
                    domains: None,
                    arithmetic: false,
                    kind: super::exact::ExactKind::Literal(n),
                    range: Some(crate::numeric::Range::point(n)),
                    location: arm.location,
                })),
            };
        }
        if ty == Type::Symbol {
            value.ty = Type::Symbol;
            super::text::register_constants(&value, bindings);
        }
        values.push(Some(value));
    }
    let exhaustive = match ty {
        Type::Bool => seen.iter().any(|seen| seen.len() == 2),
        Type::Enum(ty) => seen.iter().any(|seen| seen.len() == ty.count),
        _ => wildcard,
    };
    if !exhaustive {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("match.exhaustive", "match must cover every value"),
        ));
    }
    Ok(Patterns { values, warnings })
}

fn pattern_key(value: &CheckedExpression) -> String {
    match &value.value {
        Value::Integer { value, .. } => value.to_string(),
        Value::Exact(e) => super::exact::evaluate(e).unwrap().unwrap().to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Text { text, .. } => text.clone(),
        _ => unreachable!("constant match pattern has a validated type"),
    }
}

// Literal spelling (notably -0) has its own typed rule. Other exact constants
// are evaluated with the target's storage range, just like concrete patterns.
fn evaluate_generic_pattern(
    checked: &CheckedExpression,
    pattern: &crate::ast::Expression,
    target: Type,
    index: usize,
    bindings: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    let Some(target) = domain::at(target, index) else {
        return Ok(None);
    };
    let pattern = super::generic::without_groups(pattern);
    if let (Type::Integer(ty), crate::ast::ExpressionKind::Integer { digits, negative }) =
        (target, &pattern.kind)
    {
        return super::expression::integer_literal(digits, *negative, ty, pattern.location)
            .map(Some);
    }
    let mut stored;
    let checked = if checked.ty == Type::Exact && target.is_integer() {
        stored = checked.clone();
        stored.ty = target;
        &stored
    } else {
        checked
    };
    constants::generic::at(checked, index, bindings)
}
