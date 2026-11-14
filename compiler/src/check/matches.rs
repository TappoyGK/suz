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
        Type::Integer(_) | Type::Bool | Type::Enum(_) | Type::Text
    ) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "match requires an integer, bool, enum or text target [match.target]",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut wildcard = false;
    let mut values = Vec::new();
    let mut warnings = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        let Some(pattern) = &arm.pattern else {
            if !matches!(ty, Type::Integer(_) | Type::Text) || wildcard || index + 1 != arms.len() {
                return Err(arm.location.error(
                    ErrorKind::InvalidSource,
                    "wildcard is allowed only as the final integer or text match arm [match.pattern]",
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
        let checked = expression(pattern, Some(ty), bindings)?;
        diagnostics::collect(&checked, &mut warnings);
        let value = constants::evaluate(&checked)?.ok_or_else(|| {
            arm.location.error(
                ErrorKind::InvalidSource,
                "match pattern must be constant [const.expression]",
            )
        })?;
        let key = match &value.value {
            Value::Integer { value, .. } => value.to_string(),
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
