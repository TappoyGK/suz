//! 配列の初期化と要素へのアクセスを検査する。要素の保存先は寿命解析でも元の配列へ結ぶ。
use super::{
    constants, expression,
    ir::{CheckedExpression, CheckedHandler, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Location, Type},
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
        Some(Type::Array(id)) => {
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
            constant_counts,
        },
    })
}

fn chain<'a>(
    expr: &'a Expression,
    indices: &mut Vec<(&'a Expression, Location)>,
) -> Result<&'a Expression, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Group(v) => chain(v, indices),
        ExpressionKind::Slice {
            target,
            start: Some(index),
            end: None,
            index: true,
            fallback: None,
        } => {
            let root = chain(target, indices)?;
            indices.push((index, expr.location));
            Ok(root)
        }
        _ => Ok(expr),
    }
}
fn address(expr: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let mut indices = Vec::new();
    let root = chain(expr, &mut indices)?;
    let mut target = expression::expression(root, None, scope)?;
    let mut position = 0;
    loop {
        let id = target.ty.array_id().ok_or_else(|| {
            expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.index", "array indexing requires an array reference"),
            )
        })?;
        if !target.ty.is_nonnull() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.value", "use let before accessing a nullable array"),
            ));
        }
        let array = scope.structs.arrays.borrow().get(id);
        if indices.len() - position < array.rank {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.index",
                    "partial array indexing is not implemented yet"
                ),
            ));
        }
        let end = position + array.rank;
        let checked = indices[position..end]
            .iter()
            .map(|(i, location)| {
                let value = expression::expression(i, None, scope)?;
                if !value.ty.is_integer() {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("array.index", "array index requires an integer"),
                    ));
                }
                Ok((value, *location))
            })
            .collect::<Result<_, Diagnostic>>()?;
        let address = CheckedExpression {
            ty: array.element.pointer(),
            value: Value::ArrayElement {
                target: Box::new(target),
                indices: checked,
            },
        };
        if end == indices.len() {
            return Ok(address);
        }
        // 配列参照を要素に持つ場合は、一つの実体の検査を終えてから次の参照を読む。
        // 途中の読み取りにハンドラーを付けず、末尾のorへ全体の失敗を伝える。
        target = CheckedExpression {
            ty: array.element,
            value: Value::ArrayRead {
                address: Box::new(address),
                handler: None,
            },
        };
        position = end;
    }
}

pub(super) fn read_or_text(
    expr: &Expression,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::Slice {
        target,
        index,
        fallback,
        ..
    } = &expr.kind
    else {
        unreachable!()
    };
    let mut stripped = expr.clone();
    let ExpressionKind::Slice { fallback: h, .. } = &mut stripped.kind else {
        unreachable!()
    };
    *h = None;
    let mut indices = Vec::new();
    let root = chain(&stripped, &mut indices)?;
    let root_ty = expression::operand_type(root, scope);
    if !root_ty.is_some_and(|t| t.array_id().is_some()) {
        // 単独の切り出しでも対象の配列型を調べる。
        if !expression::operand_type(target, scope).is_some_and(|t| t.array_id().is_some()) {
            return expression::slices::check(expr, scope);
        }
    }
    if !index {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.slice", "array slicing is not implemented yet"),
        ));
    }
    let address = address(&stripped, scope)?;
    let ty = address.ty.pointee().unwrap();
    let handler = match fallback {
        Some(FailureHandler::Value(v)) => {
            CheckedHandler::Value(Box::new(expression::coerced(v, Some(ty), scope)?))
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
    Ok(CheckedExpression {
        ty,
        value: Value::ArrayRead {
            address: Box::new(address),
            handler: Some(handler),
        },
    })
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
