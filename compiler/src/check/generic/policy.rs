use super::super::{exact, expression, scope::Scope};
use crate::{
    Diagnostic, ErrorKind,
    ast::{ComparisonOperator, Expression, ExpressionKind, FloatType, IntegerType, Type},
};

fn variants(scope: &Scope) -> Vec<[Option<Type>; 2]> {
    let ints = if scope.generic_types[0].is_some() {
        [
            IntegerType::I8,
            IntegerType::I16,
            IntegerType::I32,
            IntegerType::I64,
            IntegerType::U8,
            IntegerType::U16,
            IntegerType::U32,
            IntegerType::U64,
        ]
        .into_iter()
        .map(|t| Some(Type::Integer(t)))
        .collect()
    } else {
        vec![None]
    };
    let floats = if scope.generic_types[1].is_some() {
        vec![
            Some(Type::Float(FloatType::F32)),
            Some(Type::Float(FloatType::F64)),
        ]
    } else {
        vec![None]
    };
    ints.into_iter()
        .flat_map(|i| floats.iter().map(move |f| [i, *f]))
        .collect()
}
// Probes must not create generated functions, register constants, mutate local
// assignment state, or publish diagnostics. Recompute inferred aliases in order.
fn probe(scope: &Scope, types: [Option<Type>; 2]) -> Option<Scope> {
    let mut copy = scope.clone();
    copy.generic_types = types;
    if let Some(result) = copy.generic_result {
        copy.result = copy.resolve_type(result);
    }
    copy.probing_generic = true;
    copy.missing_cast_handler = Default::default();
    copy.instances = Default::default();
    copy.comparison_warnings = Default::default();
    copy.division_warnings = Default::default();
    copy.constant_texts = Default::default();
    copy.type_hints = Default::default();
    copy.float_hints = Default::default();
    copy.hint_depth = Default::default();
    let mut bindings: Vec<_> = scope
        .iter()
        .map(|(name, b)| (name.clone(), b.clone()))
        .collect();
    bindings.sort_by_key(|(_, b)| b.id);
    for (name, mut binding) in bindings {
        if let Some(source) = &binding.generic_source {
            binding.ty = match source.as_ref() {
                super::BindingSource::Initializer { annotation, value } => {
                    expression::coerced(value, annotation.map(|t| copy.resolve_type(t)), &copy)
                        .ok()?
                        .ty
                }
                super::BindingSource::Range { start, end } => {
                    super::super::ranges::bounds(start, end, &copy).ok()?.0.ty
                }
            };
        } else if let Some(ty) = binding.generic {
            binding.ty = copy.resolve_type(ty);
        }
        copy.insert(name, binding);
    }
    Some(copy)
}

pub(crate) fn validate_cast(expr: &Expression, scope: &Scope) -> Result<(), Diagnostic> {
    if scope.probing_generic || scope.generic_types.iter().all(Option::is_none) {
        return Ok(());
    }
    let ExpressionKind::Cast { fallback, wrap, .. } = &expr.kind else {
        unreachable!()
    };
    if fallback.is_some() || *wrap {
        return Ok(());
    }
    // The ordinary checker retains the type-specific conversion rules. Applying
    // it to every substitution also preserves the constant-expression exception.
    for types in variants(scope) {
        let Some(copy) = probe(scope, types) else {
            continue;
        };
        let _ = expression::casts::check(expr, &copy);
        if let Some(location) = copy.missing_cast_handler.get() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "this conversion can fail for a generic numeric type and requires or [num.fn-cast]",
            ));
        }
    }
    Ok(())
}

pub(crate) fn comparison_warning(
    left: &Expression,
    right: &Expression,
    operator: ComparisonOperator,
    scope: &Scope,
    result: bool,
) -> bool {
    if scope.probing_generic || scope.generic_types.iter().all(Option::is_none) {
        return true;
    }
    variants(scope).into_iter().all(|types| {
        let Some(copy) = probe(scope, types) else {
            return false;
        };
        let Ok(left) = exact::comparison_value(left, &copy) else {
            return false;
        };
        let Ok(right) = exact::comparison_value(right, &copy) else {
            return false;
        };
        exact::constant_comparison(&left, &right, operator) == Some(result)
    })
}
