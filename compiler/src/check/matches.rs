use super::{
    constants, declarations, diagnostics,
    expression::expression,
    ir::{CheckedExpression, Value},
};
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
    if !matches!(
        ty,
        Type::Exact
            | Type::Char
            | Type::Integer(_)
            | Type::Bool
            | Type::Enum(_)
            | Type::Symbol
            | Type::Text
    ) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "match requires an integer, char, bool, enum, symbol or text target [match.target]",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut wildcard = false;
    let mut values = Vec::new();
    let mut warnings = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        let Some(pattern) = &arm.pattern else {
            if !matches!(
                ty,
                Type::Exact | Type::Char | Type::Integer(_) | Type::Symbol | Type::Text
            ) || wildcard
                || index + 1 != arms.len()
            {
                return Err(arm.location.error(
                    ErrorKind::InvalidSource,
                    "wildcard is allowed only as the final integer, char, symbol or text match arm [match.pattern]",
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
                    "match patterns can reference only constants [const.expression]",
                ));
            }
        }
        let checked = if ty == Type::Exact {
            super::exact::comparison_value(pattern, bindings)?
        } else {
            expression(
                pattern,
                Some(if ty == Type::Symbol { Type::Text } else { ty }),
                bindings,
            )?
        };
        diagnostics::collect(&checked, &mut warnings);
        let mut value = constants::evaluate(&checked)?.ok_or_else(|| {
            arm.location.error(
                ErrorKind::InvalidSource,
                "match pattern must be constant [const.expression]",
            )
        })?;
        if ty == Type::Exact && value.ty != Type::Exact {
            let n = super::exact::constant(&value).ok_or_else(|| {
                arm.location.error(
                    ErrorKind::InvalidSource,
                    "exact match requires integer patterns [match.pattern]",
                )
            })?;
            value = CheckedExpression {
                ty: Type::Exact,
                value: Value::Exact(Box::new(super::exact::Exact {
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
        let key = match &value.value {
            Value::Integer { value, .. } => value.to_string(),
            Value::Exact(e) => super::exact::evaluate(e).unwrap().unwrap().to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Text { text, .. } => text.clone(),
            _ => unreachable!(),
        };
        if !seen.insert(key) {
            return Err(arm.location.error(
                ErrorKind::InvalidSource,
                "duplicate evaluated pattern [match.pattern]",
            ));
        }
        values.push(Some(value));
    }
    let exhaustive = match ty {
        Type::Bool => seen.len() == 2,
        Type::Enum(ty) => seen.len() == ty.count,
        _ => wildcard,
    };
    if !exhaustive {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "match must cover every value [match.exhaustive]",
        ));
    }
    Ok(Patterns { values, warnings })
}
