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
    let has = |ty| signature.result == ty || signature.parameters.contains(&ty);
    let ints = if has(crate::ast::Type::GenericInt) {
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
    let floats = if has(crate::ast::Type::GenericFloat) {
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
pub(crate) fn analyze(function: &Function, globals: &Scope) -> Result<(), Diagnostic> {
    let mut common_errors: Option<BTreeMap<(usize, usize, String), Diagnostic>> = None;
    let mut comparisons: Option<BTreeMap<(usize, usize), bool>> = None;
    let mut divisions = BTreeSet::new();
    for types in substitutions(globals, function) {
        let mut scope = globals.speculative();
        scope.analyzing_generic = true;
        let result = functions::check_function(function, &scope, 0, types);
        let mut errors = scope.analysis_errors.borrow().clone();
        if let Err(error) = result {
            errors.push(error);
        }
        let mut found_errors = BTreeMap::new();
        for error in errors {
            if error.generic_cast {
                return Err(Location { line: error.line, column: error.column }.error(ErrorKind::InvalidSource,
                    "this conversion can fail for a generic numeric type and requires or [num.fn-cast]"));
            }
            found_errors.insert((error.line, error.column, error.message.clone()), error);
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
    Ok(())
}
