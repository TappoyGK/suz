//! 明示した値だけを環境に写し、クロージャ本体を独立した関数として検査する。
//! キャプチャーは読み取り専用の仮引数と同じ扱いにし、既存の関数・文の検査を再利用する。

use super::{
    functions,
    ir::{CheckedClosure, CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Closure, Parameter, Type},
};
use std::collections::BTreeSet;

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
        .map(|p| p.ty)
        .collect::<Vec<_>>()
        != signature.parameters
        || closure.function.result != signature.result
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
        captures.push(binding.constant.clone().unwrap_or(CheckedExpression {
            ty: binding.ty,
            value: Value::Local(binding.id),
        }));
        function.parameters.push(Parameter {
            name: capture.name.clone(),
            ty: binding.ty,
            location: capture.location,
        });
    }
    let mut scope = outer.clone();
    scope.uncaptured.extend(scope.locals.keys().cloned());
    scope.locals.clear();
    scope.value_captures = closure.captures.iter().map(|c| c.name.clone()).collect();
    // 外側の式の型ヒントは、独立したローカル番号を持つ本体では使わない。
    scope.type_hints = Default::default();
    scope.float_hints = Default::default();
    scope.hint_depth = Default::default();
    let (function, _) = functions::check_function(&function, scope, 0)?;
    Ok(CheckedExpression {
        ty: Type::Function(shape),
        value: Value::Closure(Box::new(CheckedClosure { function, captures })),
    })
}
