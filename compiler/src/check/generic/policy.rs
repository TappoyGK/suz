use super::{
    super::{functions, scope::Scope},
    GenericTypes,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{FloatType, Function, IntegerType, Location},
};
use std::collections::{BTreeMap, BTreeSet};

fn substitutions(scope: &Scope, function: &Function) -> Vec<GenericTypes> {
    let signature = &scope.functions[&function.name];
    let ints = if signature.declares(super::Family::Int) {
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
        .map(Some)
        .collect()
    } else {
        vec![None]
    };
    let floats = if signature.declares(super::Family::Float) {
        vec![Some(FloatType::F32), Some(FloatType::F64)]
    } else {
        vec![None]
    };
    ints.into_iter()
        .flat_map(|int| floats.iter().map(move |&float| GenericTypes { int, float }))
        .collect()
}

// [num.fn-unused], [num.fn-cast], [num.fn-warning]. Analyze each substitution
// once per template, walking its body in order with the ordinary checker.
// There are at most 16 substitutions, independent of body length or call count.
// Speculation never schedules emitted instances or registers runtime data.
pub(crate) fn analyze(function: &Function, globals: &Scope) -> Result<Vec<Location>, Diagnostic> {
    let mut common_errors: Option<BTreeMap<(usize, usize, String), Diagnostic>> = None;
    let mut comparisons: Option<BTreeMap<(usize, usize), bool>> = None;
    let mut divisions = BTreeSet::new();
    let mut arithmetic = BTreeMap::new();
    for types in substitutions(globals, function) {
        let mut scope = globals.speculative();
        scope.analyzing_generic = true;
        // A new substitution needs fresh caches; nested blocks within it share them.
        scope.type_hints = Default::default();
        scope.float_hints = Default::default();
        scope.hint_depth = Default::default();
        let result = functions::check_function(function, scope.clone(), 0, types);
        let mut errors = scope.analysis.errors.borrow().clone();
        match result {
            Ok((_, sites)) => arithmetic.extend(
                sites
                    .into_iter()
                    .map(|site| ((site.line, site.column), site)),
            ),
            Err(error) => errors.push(error),
        }
        for &(location, risk) in scope.analysis.casts.borrow().iter() {
            if !matches!(risk, super::super::expression::casts::CastRisk::Safe) {
                return Err(location.error(ErrorKind::InvalidSource,
                    "this conversion can fail for a generic numeric type and requires or [num.fn-cast]"));
            }
        }
        let mut found_errors = BTreeMap::new();
        for error in errors {
            found_errors.insert((error.line, error.column, error.rule_id.clone()), error);
        }
        match &mut common_errors {
            None => common_errors = Some(found_errors),
            Some(common) => common.retain(|site, _| found_errors.contains_key(site)),
        }
        // A failed substitution cannot establish a universal warning for sites
        // it never reached. Intersect only diagnostics actually observed in all
        // substitutions; type-dependent failures remain errors at actual calls.
        let found = scope.comparison_warnings.borrow();
        match &mut comparisons {
            None => comparisons = Some(found.clone()),
            Some(common) => common.retain(|site, value| found.get(site) == Some(value)),
        }
        // [diagnostic.integer-division-to-float] Substitution changes numeric
        // widths, not integer/float families. A site needs no all-types value
        // proof; retain it even if another width fails a later conversion.
        divisions.extend(scope.division_warnings.borrow().iter().copied());
    }
    if let Some((_, error)) = common_errors.unwrap_or_default().into_iter().next() {
        return Err(error);
    }
    globals
        .comparison_warnings
        .borrow_mut()
        .extend(comparisons.unwrap_or_default());
    globals.division_warnings.borrow_mut().extend(divisions);
    Ok(arithmetic.into_values().collect())
}
