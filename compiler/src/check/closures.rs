//! 明示したキャプチャーを解決し、クロージャ本体を独立した関数として検査する。
//! 値は読み取り専用の仮引数、参照は外側のvarへの別名として、既存の関数・文の検査を使う。

use super::{
    functions,
    ir::{CheckedCapture, CheckedClosure, CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Closure, Parameter, Type},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check(
    closure: &Closure,
    shape: usize,
    outer: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let signature = outer.function_types.borrow()[shape].clone();
    if closure
        .function
        .parameters
        .iter()
        .map(|p| outer.resolve_type_at(p.ty, p.location))
        .collect::<Result<Vec<_>, _>>()?
        != signature.parameters
        || outer.resolve_type_at(closure.function.result, closure.function.location)?
            != signature.result
        || closure.function.errors != signature.errors
    {
        return Err(closure.function.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "closure.place",
                "closure signature does not match the required fn type"
            ),
        ));
    }
    let mut names: BTreeSet<_> = closure
        .function
        .parameters
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let mut function = closure.function.clone();
    let mut captures = Vec::new();
    let mut references = BTreeMap::new();
    for capture in &closure.captures {
        if !names.insert(capture.name.clone()) {
            return Err(capture.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "closure.capture-list",
                    "capture name is duplicated or conflicts with a parameter"
                ),
            ));
        }
        let binding = outer.locals.get(&capture.name).ok_or_else(|| {
            capture.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "closure.capture-list",
                    "capture must name a local binding visible here"
                ),
            )
        })?;
        if capture.by_reference {
            if !binding.mutable {
                return Err(capture.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "closure.capture-list",
                        "reference capture requires a local var or an outer reference capture"
                    ),
                ));
            }
            references.insert(capture.name.clone(), binding.clone());
        }
        captures.push(CheckedCapture {
            value: binding.constant.clone().unwrap_or(CheckedExpression {
                ty: binding.ty,
                value: Value::Local(binding.id),
            }),
            by_reference: capture.by_reference,
        });
        function.parameters.push(Parameter {
            name: capture.name.clone(),
            ty: binding.ty,
            location: capture.location,
        });
    }
    let mut scope = outer.clone();
    scope.uncaptured.extend(scope.locals.keys().cloned());
    scope.locals.clear();
    scope.value_captures = closure
        .captures
        .iter()
        .filter(|c| !c.by_reference)
        .map(|c| c.name.clone())
        .collect();
    // 外側の式の型ヒントは、独立したローカル番号を持つ本体では使わない。
    scope.type_hints = Default::default();
    scope.float_hints = Default::default();
    scope.hint_depth = Default::default();
    let (function, _) = functions::check_with_captures(&function, scope, 0, &references)?;
    for (name, binding) in references {
        if binding.assigned.get() {
            outer.locals[&name].assigned.set(true);
        }
    }
    Ok(CheckedExpression {
        ty: Type::Function(shape),
        value: Value::Closure(Box::new(CheckedClosure { function, captures })),
    })
}
