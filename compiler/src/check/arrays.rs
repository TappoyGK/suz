//! 配列の初期化と要素へのアクセスを検査する。要素の保存先は寿命解析でも元の配列へ結ぶ。
//! 利用元: 式・代入の意味検査。定数配列は[array.constant]、書き込み可否は[array.readonly]に従う。
pub(super) mod bounds;
pub(super) mod dimensions;

use super::{
    constants, expression,
    ir::{CheckedExpression, CheckedHandler, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Type},
};

pub(super) fn initializer(
    expr: &Expression,
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::ArrayInit {
        values,
        lengths,
        repeated,
    } = &expr.kind
    else {
        unreachable!()
    };
    let required = match expected {
        Some(Type::Array(id) | Type::ArrayRef { id, .. }) => {
            let array = scope.structs.arrays.borrow().get(id);
            if array.rank != lengths.len() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.type", "array rank does not match the required type"),
                ));
            }
            Some(array.element)
        }
        _ => None,
    };
    let mut initial = Vec::new();
    for v in values {
        initial.push(expression::coerced(v, required, scope)?);
    }
    let element = required
        .or_else(|| initial.first().map(|v| v.ty))
        .ok_or_else(|| {
            expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.initializer-type",
                    "empty array requires an element type"
                ),
            )
        })?;
    if element == Type::Void || matches!(element, Type::Function(_)) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.type", "array elements must be stored values"),
        ));
    }
    if initial.iter().any(|v| v.ty != element) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.initializer-type",
                "array elements must have the same type"
            ),
        ));
    }
    let mut checked_lengths = Vec::new();
    for length in lengths {
        let v = expression::expression(length, None, scope)?;
        if !v.ty.is_integer() {
            return Err(length.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.layout", "array dimensions require integers"),
            ));
        }
        if constants::constant_integer(&v)?.is_some_and(|n| !(0..=i32::MAX as i128).contains(&n)) {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.layout", "array dimension is outside len_t"),
            ));
        }
        checked_lengths.push(v);
    }
    // 内側の積も表せることを確認する。外側が0でも内側の配置のあふれを隠さない。
    let mut count = Some(1_u64);
    let mut constant_counts = Vec::new();
    let minimum = minimum_size(element, scope);
    for length in checked_lengths.iter().rev() {
        count = match (count, constants::constant_integer(length)?) {
            (Some(n), Some(m)) => Some(n.checked_mul(m as u64).ok_or_else(|| {
                expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.layout", "array element count overflows size_t"),
                )
            })?),
            _ => None,
        };
        if let Some(count) = count {
            if count.checked_mul(minimum).is_none() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.layout", "array byte size overflows size_t"),
                ));
            }
            constant_counts.push(count);
        }
    }
    let ty = scope
        .structs
        .arrays
        .borrow_mut()
        .intern(element, lengths.len());
    Ok(CheckedExpression {
        ty,
        value: Value::Array {
            values: initial,
            lengths: checked_lengths,
            repeated: *repeated,
            metadata: Box::new(super::ir::ArrayMetadata {
                origins: (0..lengths.len())
                    .map(|axis| (expr.location, axis))
                    .collect(),
                constant_counts,
            }),
        },
    })
}

/// 明示的なorで区切られるまで、括弧をまたぐアクセスを一つの失敗単位にする。
fn chain<'a>(expr: &'a Expression, steps: &mut Vec<&'a Expression>, outer: bool) -> &'a Expression {
    match &expr.kind {
        ExpressionKind::Group(v) => chain(v, steps, outer),
        ExpressionKind::Slice {
            target, fallback, ..
        } if outer || fallback.is_none() => {
            let root = chain(target, steps, false);
            steps.push(expr);
            root
        }
        _ => expr,
    }
}

fn bound(expr: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let value = expression::expression(expr, None, scope)?;
    if !value.ty.is_integer() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.index", "array bounds require integers"),
        ));
    }
    Ok(value)
}

fn access(expr: &Expression, scope: &Scope) -> Result<(CheckedExpression, bool), Diagnostic> {
    let mut steps = Vec::new();
    let root = chain(expr, &mut steps, true);
    let has_handler = matches!(
        expr.kind,
        ExpressionKind::Slice {
            fallback: Some(_),
            ..
        }
    );
    let mut target = expression::expression(root, None, scope)?;
    let mut position = 0;
    let mut fallible = false;
    while position < steps.len() {
        let id = target.ty.array_id().ok_or_else(|| {
            expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.index", "array indexing requires an array reference"),
            )
        })?;
        if target.ty.is_nullable() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.value", "use let before accessing a nullable array"),
            ));
        }
        let array = scope.structs.arrays.borrow().get(id);
        let indices = steps[position..]
            .iter()
            .take_while(|s| matches!(s.kind, ExpressionKind::Slice { index: true, .. }))
            .count();
        if indices >= array.rank {
            let mut checked = Vec::new();
            for step in &steps[position..position + array.rank] {
                let ExpressionKind::Slice { start: Some(i), .. } = &step.kind else {
                    unreachable!()
                };
                let mut value = bound(i, scope)?;
                if !has_handler
                    && target.ty.is_readonly_array()
                    && bounds::invalid_constant_index(&target, &value, checked.len(), scope)
                {
                    return Err(step.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "array.constant-index",
                            "constant array index is out of bounds"
                        ),
                    ));
                }
                let proven = bounds::index(&target, &value, checked.len(), scope);
                if proven {
                    bounds::narrow(&mut value, scope);
                }
                fallible |= !proven;
                checked.push((value, step.location, proven));
            }
            let address = CheckedExpression {
                ty: array.element.pointer(),
                value: Value::ArrayElement {
                    target: Box::new(target),
                    indices: checked,
                },
            };
            target = CheckedExpression {
                ty: array.element,
                value: Value::ArrayRead {
                    address: Box::new(address),
                    handler: None,
                },
            };
            position += array.rank;
        } else {
            let step = steps[position];
            let ExpressionKind::Slice {
                start, end, index, ..
            } = &step.kind
            else {
                unreachable!()
            };
            let ty = if *index {
                scope
                    .structs
                    .arrays
                    .borrow_mut()
                    .intern(array.element, array.rank - 1)
                    .array_reference(target.ty.is_readonly_array())
            } else {
                target.ty
            };
            let start = start
                .as_ref()
                .map(|v| bound(v, scope).map(Box::new))
                .transpose()?;
            let end = end
                .as_ref()
                .map(|v| bound(v, scope).map(Box::new))
                .transpose()?;
            if *index
                && !has_handler
                && target.ty.is_readonly_array()
                && bounds::invalid_constant_index(&target, start.as_ref().unwrap(), 0, scope)
            {
                return Err(step.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.constant-index",
                        "constant array index is out of bounds"
                    ),
                ));
            }
            let proven = if *index {
                bounds::index(&target, start.as_ref().unwrap(), 0, scope)
            } else {
                bounds::slice(&target, start.as_deref(), end.as_deref(), scope)
            };
            fallible |= !proven;
            target = CheckedExpression {
                ty,
                value: Value::ArrayView {
                    target: Box::new(target),
                    start,
                    end,
                    proven,
                    index: *index,
                    location: step.location,
                    handler: None,
                },
            };
            position += 1;
        }
    }
    Ok((target, fallible))
}

fn address(expr: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let (value, _) = access(expr, scope)?;
    if let Value::ArrayRead {
        address,
        handler: None,
    } = value.value
    {
        if let Value::ArrayElement { target, .. } = &address.value
            && target.ty.is_readonly_array()
        {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.readonly",
                    "readonly array elements cannot be assigned"
                ),
            ));
        }
        Ok(*address)
    } else {
        Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.write",
                "assign to an element, not a partial array or slice"
            ),
        ))
    }
}

pub(super) fn read_or_text(
    expr: &Expression,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::Slice {
        target, fallback, ..
    } = &expr.kind
    else {
        unreachable!()
    };
    // 型推論のキャッシュはASTの住所をキーにするため、一時的な複製を検査しない。
    // 末尾のorだけを探索時に無視し、元のASTを最後まで参照する。
    let root = chain(expr, &mut Vec::new(), true);
    if !expression::operand_type(root, scope).is_some_and(|t| t.array_id().is_some())
        && !expression::operand_type(target, scope).is_some_and(|t| t.array_id().is_some())
    {
        return expression::slices::check(expr, scope);
    }
    let (mut result, fallible) = access(expr, scope)?;
    if !fallible {
        if fallback.is_some() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.access-handler",
                    "statically proven array access does not accept or"
                ),
            ));
        }
        return Ok(constants::evaluate(&result)?.unwrap_or(result));
    }
    let handler = match fallback {
        Some(FailureHandler::Value(v)) => {
            CheckedHandler::Value(Box::new(expression::coerced(v, Some(result.ty), scope)?))
        }
        Some(FailureHandler::Return { value, location }) => CheckedHandler::Return {
            value: Box::new(super::functions::return_value(value, scope)?),
            location: *location,
        },
        None => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.access-handler", "array access requires or"),
            ));
        }
        _ => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.access-handler", "invalid array failure handler"),
            ));
        }
    };
    match &mut result.value {
        Value::ArrayRead { handler: h, .. } | Value::ArrayView { handler: h, .. } => {
            *h = Some(handler)
        }
        _ => unreachable!(),
    }
    Ok(result)
}

pub(super) fn write_target(
    expr: &Expression,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Group(v) => write_target(v, scope),
        ExpressionKind::Slice { .. } => {
            let address = address(expr, scope)?;
            Ok(CheckedExpression {
                ty: address.ty.pointee().unwrap(),
                value: Value::Deref(Box::new(address)),
            })
        }
        ExpressionKind::Field { target, name } => {
            let value = write_target(target, scope)?;
            let value = if value.ty.pointee().is_some() {
                super::arena::deref(value, expr.location)?
            } else {
                value
            };
            let Type::Struct(id) = value.ty else {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("struct.field", "field target must be a struct"),
                ));
            };
            let declaration = scope.structs.get(id);
            let (index, field) = declaration
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| &f.name == name)
                .ok_or_else(|| {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("struct.field", "unknown field"),
                    )
                })?;
            Ok(CheckedExpression {
                ty: field.ty,
                value: Value::Field {
                    target: Box::new(value),
                    index,
                },
            })
        }
        _ => expression::expression(expr, None, scope),
    }
}

/// Cのパディングやポインタ幅によらず必要な最小バイト数。環境依存の配置は生成Cで追加検査する。
fn minimum_size(ty: Type, scope: &Scope) -> u64 {
    match ty {
        Type::Integer(t) => u64::from(t.bits().div_ceil(8)),
        Type::Enum(t) => u64::from(t.base.bits().div_ceil(8)),
        Type::Float(crate::ast::FloatType::F64) => 8,
        Type::Float(crate::ast::FloatType::F32) | Type::Char => 4,
        Type::Text => 17,
        Type::Struct(id) => scope
            .structs
            .get(id)
            .fields
            .iter()
            .map(|f| minimum_size(f.ty, scope))
            .sum(),
        _ => 1,
    }
}

/// 定数配列の要素と寸法はコンパイル時に確定させる。newの初期値はこの制限を受けない。
pub(super) fn constant_initializer(
    expr: &Expression,
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let mut value = initializer(expr, expected, scope)?;
    let Type::Array(id) = value.ty else {
        unreachable!()
    };
    if !constant_element_type(
        scope.structs.arrays.borrow().get(id).element,
        scope,
        &mut std::collections::BTreeSet::new(),
    ) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.constant",
                "constant arrays cannot contain pointer or function values"
            ),
        ));
    }
    value.ty = Type::ArrayRef {
        id,
        mutable: false,
        nullable: false,
    };
    constants::evaluate(&value)?.ok_or_else(|| {
        expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.constant",
                "array elements and dimensions must be constant expressions"
            ),
        )
    })
}

fn constant_element_type(
    ty: Type,
    scope: &Scope,
    seen: &mut std::collections::BTreeSet<Type>,
) -> bool {
    if !seen.insert(ty) {
        return true;
    }
    match ty {
        Type::Pointer { .. } | Type::CPointer | Type::Function(_) | Type::StaticFunction(_) => {
            false
        }
        Type::Struct(id) => scope
            .structs
            .get(id)
            .fields
            .iter()
            .all(|f| constant_element_type(f.ty, scope, seen)),
        Type::ArrayRef {
            id,
            mutable: false,
            nullable: false,
        } => constant_element_type(scope.structs.arrays.borrow().get(id).element, scope, seen),
        Type::ArrayRef { .. } => false,
        _ => true,
    }
}

/// 配列への参照を含む値にも比較を認めない。フィールドを通した比較も同じ規則で診断する。
pub(super) fn contains_reference(ty: Type, scope: &Scope) -> bool {
    if ty.array_id().is_some() {
        return true;
    }
    match ty {
        Type::Struct(id) => scope
            .structs
            .get(id)
            .fields
            .iter()
            .any(|f| contains_reference(f.ty, scope)),
        _ => false,
    }
}
