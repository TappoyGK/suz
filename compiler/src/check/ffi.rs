//! 外部関数の名前と、C環境に依存する数値の受け渡しを検査する。
use super::scope::Signature;
use crate::{Diagnostic, ErrorKind, ast::Program};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn signatures(
    program: &Program,
    signatures: &mut BTreeMap<String, Signature>,
) -> Result<(), Diagnostic> {
    let mut c_names = BTreeSet::new();
    for (index, function) in program.externs.iter().enumerate() {
        if !c_names.insert(&function.c_name) {
            return Err(function.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "ffi.duplicate",
                    "C function is already declared in this file"
                ),
            ));
        }
        if signatures.contains_key(&function.name)
            || program.bindings.iter().any(|d| d.name == function.name)
            || program.enums.iter().any(|d| d.name == function.name)
            || program.structs.iter().any(|d| d.name == function.name)
            || program.fnshapes.iter().any(|d| d.name == function.name)
        {
            return Err(function.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("decl.duplicate", "duplicate top-level name"),
            ));
        }
        signatures.insert(
            function.name.clone(),
            Signature {
                external: true,
                id: program.functions.len() + index,
                parameters: function.parameters.iter().map(|p| p.ty).collect(),
                result: function.result,
                errors: Vec::new(),
            },
        );
    }
    for function in &program.externs {
        for parameter in &function.parameters {
            if signatures.contains_key(&parameter.name)
                || program.bindings.iter().any(|d| d.name == parameter.name)
                || program.enums.iter().any(|d| d.name == parameter.name)
                || program.structs.iter().any(|d| d.name == parameter.name)
                || program.fnshapes.iter().any(|d| d.name == parameter.name)
            {
                return Err(parameter.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.shadowing", "parameter shadows a top-level name"),
                ));
            }
        }
    }
    Ok(())
}

use super::{
    ir::{CheckedExpression, Value},
    scope::Scope,
};
use crate::ast::{Expression, ExpressionKind, FailureHandler, Location, TemplatePart, Type};

/// C固有型は文字列パーサーのスキーマに含めない。構造体内でも同じ制約を適用する。
pub(super) fn contains_c_type(ty: Type, scope: &Scope) -> bool {
    match ty {
        Type::CInteger(_) => true,
        Type::Struct(id) => scope.structs[id]
            .fields
            .iter()
            .any(|f| contains_c_type(f.ty, scope)),
        _ => false,
    }
}

/// 整数リテラルだけを期待されるC型へ渡す。式の演算をC型で行う入口にはしない。
pub(super) fn literal(
    expr: &Expression,
    target: Type,
    scope: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Group(inner) => return literal(inner, target, scope),
        ExpressionKind::Integer { .. } => {}
        ExpressionKind::Negate(inner) if matches!(inner.kind, ExpressionKind::Integer { .. }) => {}
        _ => return Ok(None),
    }
    let operand = super::exact::comparison_value(expr, scope)?;
    Ok(Some(CheckedExpression {
        ty: target,
        value: Value::CheckedCast {
            location: expr.location,
            constant_struct: None,
            wrap: false,
            operand: Box::new(operand),
            fallback: Box::new(CheckedExpression {
                ty: target,
                value: Value::Panic {
                    location: expr.location,
                    parts: vec![TemplatePart::Text(
                        "[ffi.literal-range] integer literal is outside the C type range".into(),
                    )],
                },
            }),
        },
    }))
}

pub(super) fn cast(
    operand: CheckedExpression,
    target: Type,
    wrap: bool,
    fallback: &Option<FailureHandler>,
    location: Location,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let integral = |t| matches!(t, Type::CInteger(_) | Type::Integer(_) | Type::Exact);
    let valid = |t| integral(t) || matches!(t, Type::Float(_));
    let allowed =
        |from, to| valid(from) && valid(to) && (!wrap || (integral(from) && integral(to)));
    let error = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("ffi.cast", "C numeric conversions require numeric types"),
        )
    };
    use super::generic::domain;
    match domain::judge(operand.ty, target, allowed) {
        domain::Judgment::Invalid => return Err(error()),
        domain::Judgment::Deferred => scope.defer_rule(|i| {
            (!allowed(domain::at(operand.ty, i)?, domain::at(target, i)?)).then(error)
        }),
        domain::Judgment::Valid => {}
    }
    if wrap {
        if fallback.is_some() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "expr.cast-wrap-handler-syntax",
                    "integer as% does not accept or"
                ),
            ));
        }
        return Ok(CheckedExpression {
            ty: target,
            value: Value::Truncate(Box::new(operand)),
        });
    }
    if let Some(FailureHandler::Return { value, .. }) = fallback {
        let return_value = super::functions::return_value(value, scope)?;
        return Ok(CheckedExpression {
            ty: target,
            value: Value::CastReturn {
                location,
                wrap: false,
                operand: Box::new(operand),
                return_value: Box::new(return_value),
            },
        });
    }
    let value = match fallback {
        Some(FailureHandler::Value(value)) => Value::CheckedCast {
            location,
            constant_struct: None,
            wrap: false,
            operand: Box::new(operand),
            fallback: Box::new(super::expression::coerced(value, Some(target), scope)?),
        },
        Some(_) => return Err(super::errors::invalid_handler(location)),
        None if operand.ty == target
            || domain::judge_one(target, |t| matches!(t, Type::Float(_)))
                == domain::Judgment::Valid =>
        {
            Value::Cast(Box::new(operand))
        }
        None => {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "expr.cast-handler",
                    "conversion involving a C integer type requires or"
                ),
            ));
        }
    };
    Ok(CheckedExpression { ty: target, value })
}
