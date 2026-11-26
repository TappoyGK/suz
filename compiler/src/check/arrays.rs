//! 配列の初期化と要素へのアクセスを検査する。要素の保存先は寿命解析でも元の配列へ結ぶ。
//! 利用元: 式・代入の意味検査。定数配列は[array.constant]、書き込み可否は[array.readonly]に従う。
pub(super) mod bounds;
pub(super) mod dimensions;
pub(super) mod facts;

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
        keys,
        repeated,
        ..
    } = &expr.kind
    else {
        unreachable!()
    };
    let expected_array = match expected {
        Some(Type::Array(id) | Type::ArrayRef { id, .. }) => {
            Some(scope.structs.arrays.borrow().get(id))
        }
        _ => None,
    };

    // 列挙型添字の反復初期化 `new #[式; E]` / `.[式; E]` の判定。
    let enum_repeated = if *repeated {
        let enum_pos = lengths.iter().position(|length| {
            matches!(
                &length.kind,
                ExpressionKind::Name(name) if crate::parser::names::is_type_spelling(name)
            )
        });
        if let Some(pos) = enum_pos {
            if lengths.len() != 1 {
                return Err(lengths[pos].location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "enum-indexed arrays must be 1-dimensional"
                    ),
                ));
            }
            let ExpressionKind::Name(name) = &lengths[0].kind else {
                unreachable!()
            };
            let Some(decl) = scope
                .enums
                .iter()
                .find(|d| &d.name == name && !d.ty.is_error)
            else {
                return Err(lengths[0].location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "unknown enum type '{name}' in array initializer"
                    ),
                ));
            };
            Some(decl.ty)
        } else {
            None
        }
    } else {
        None
    };

    // 列挙子キー初期化 `new #[E.A: 式, ...]` / `.[E.A: 式, ...]` の判定。
    let (index_enum, slots) = if let Some(keys) = keys {
        let mut enum_ty: Option<crate::ast::EnumType> = None;
        let mut seen = Vec::new();
        let mut slots = Vec::with_capacity(keys.len());
        for key in keys {
            let ExpressionKind::Variant {
                enum_name,
                variant: name,
            } = &key.kind
            else {
                return Err(key.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "enum array key must be an enum variant (Enum.Variant)"
                    ),
                ));
            };
            let Some(decl) = scope
                .enums
                .iter()
                .find(|d| &d.name == enum_name && !d.ty.is_error)
            else {
                return Err(key.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "unknown enum type '{enum_name}' in array key"
                    ),
                ));
            };
            let current = decl.ty;
            if let Some(previous) = enum_ty {
                if previous.id != current.id {
                    return Err(key.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "array.enum-init",
                            "all enum array keys must belong to the same enum type"
                        ),
                    ));
                }
            } else {
                enum_ty = Some(current);
                seen = vec![false; decl.variants.len()];
            }
            let Some(variant_index) = decl.variants.iter().position(|v| v == name) else {
                return Err(key.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "unknown variant '{name}' for enum '{enum_name}'"
                    ),
                ));
            };
            if seen[variant_index] {
                return Err(key.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "duplicate enum array key '{enum_name}.{name}'"
                    ),
                ));
            }
            seen[variant_index] = true;
            slots.push(variant_index);
        }
        let enum_ty = enum_ty.unwrap();
        if let Some(missing) = seen.iter().position(|&present| !present) {
            let decl = &scope.enums[enum_ty.id];
            let missing_name = &decl.variants[missing];
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.enum-init",
                    "missing enum array key '{}.{missing_name}'",
                    decl.name
                ),
            ));
        }
        (Some(enum_ty), slots)
    } else if let Some(enum_ty) = enum_repeated {
        (Some(enum_ty), Vec::new())
    } else {
        (None, Vec::new())
    };

    let required = if let Some(array) = expected_array {
        if let Some(actual_enum) = index_enum {
            if array.rank != 1 || array.index_enum != Some(actual_enum) {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-init",
                        "enum-indexed array type does not match the required type"
                    ),
                ));
            }
        } else if array.index_enum.is_some() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.enum-init",
                    "enum-indexed arrays require explicit enum keys or '; EnumName'"
                ),
            ));
        } else if array.rank != lengths.len() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.type", "array rank does not match the required type"),
            ));
        }
        Some(array.element)
    } else {
        None
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
    let checked_lengths = if let Some(enum_ty) = index_enum {
        vec![CheckedExpression {
            ty: Type::Integer(crate::ast::IntegerType::Len),
            value: Value::Integer {
                value: enum_ty.count as i128,
                minus: None,
            },
        }]
    } else {
        let mut checked_lengths = Vec::new();
        for length in lengths {
            let v = expression::expression(length, None, scope)?;
            if !v.ty.is_integer() {
                return Err(length.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.layout", "array dimensions require integers"),
                ));
            }
            if constants::constant_integer(&v)?
                .is_some_and(|n| !(0..=i32::MAX as i128).contains(&n))
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.layout", "array dimension is outside len_t"),
                ));
            }
            checked_lengths.push(v);
        }
        checked_lengths
    };
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
    let rank = checked_lengths.len();
    let ty = scope
        .structs
        .arrays
        .borrow_mut()
        .intern_indexed(element, rank, index_enum);
    Ok(CheckedExpression {
        ty,
        value: Value::Array {
            values: initial,
            lengths: checked_lengths,
            repeated: *repeated,
            metadata: Box::new(super::ir::ArrayMetadata {
                origins: (0..rank).map(|axis| (expr.location, axis)).collect(),
                constant_counts,
                slots,
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

// 定数名から構造体のフィールドだけをたどった式か。値は評価済みの定数で、副作用も未評価の部分もない。
fn constant_path(expr: &Expression, scope: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Group(v) => constant_path(v, scope),
        ExpressionKind::Field { target, .. } => constant_path(target, scope),
        ExpressionKind::Name(name) => {
            !scope.locals.contains_key(name) && scope.constants.contains_key(name)
        }
        _ => false,
    }
}

// 定数の構造体のフィールド参照から、フィールドの値を取り出す。構造体の全体は評価し直さない。
fn constant_field(expr: CheckedExpression) -> CheckedExpression {
    let CheckedExpression { ty, value } = expr;
    let Value::Field { target, index } = value else {
        return CheckedExpression { ty, value };
    };
    let inner = constant_field(*target);
    match inner.value {
        Value::Struct { mut fields } if fields.iter().any(|(id, _)| *id == index) => {
            let position = fields.iter().position(|(id, _)| *id == index).unwrap();
            fields.swap_remove(position).1
        }
        value => CheckedExpression {
            ty,
            value: Value::Field {
                target: Box::new(CheckedExpression {
                    ty: inner.ty,
                    value,
                }),
                index,
            },
        },
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

fn access(
    expr: &Expression,
    scope: &Scope,
) -> Result<(CheckedExpression, bool, bounds::Proof), Diagnostic> {
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
    // 定数名とそのフィールドの根は、検査の時点で評価済みの値になっている。畳み直すと、利用箇所ごとに全要素を評価し直す。
    if constant_path(root, scope) {
        target = constant_field(target);
    }
    if !matches!(target.value, Value::Array { .. }) {
        target = constants::evaluate(&target)?.unwrap_or(target);
    }
    let mut position = 0;
    let mut fallible = false;
    let mut proof = bounds::Proof::default();
    while position < steps.len() {
        let id = target.ty.array_id().ok_or_else(|| {
            expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.index", "array indexing requires an array reference"),
            )
        })?;
        if target.ty.is_nilable() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.value", "use let before accessing a nilable array"),
            ));
        }
        let array = scope.structs.arrays.borrow().get(id);
        if let Some(enum_ty) = array.index_enum {
            let step = steps[position];
            let ExpressionKind::Slice { start, index, .. } = &step.kind else {
                unreachable!()
            };
            if !*index {
                return Err(step.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-index",
                        "slicing is not supported for enum-indexed arrays"
                    ),
                ));
            }
            let index_expr = start.as_ref().unwrap();
            let value = expression::expression(index_expr, None, scope)?;
            if value.ty != Type::Enum(enum_ty) {
                return Err(index_expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.enum-index",
                        "enum-indexed array requires an index of type '{}'",
                        scope.enums[enum_ty.id].name
                    ),
                ));
            }
            let address = CheckedExpression {
                ty: array.element.pointer(),
                value: Value::ArrayElement {
                    target: Box::new(target),
                    indices: vec![(value, step.location, true)],
                },
            };
            target = CheckedExpression {
                ty: array.element,
                value: Value::ArrayRead {
                    address: Box::new(address),
                    handler: None,
                },
            };
            position += 1;
            if !fallible && position < steps.len() {
                target = constants::evaluate(&target)?.unwrap_or(target);
            }
            continue;
        }
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
                let step_proof = bounds::index(&target, &value, checked.len(), has_handler, scope);
                let proven = step_proof.is_some();
                if let Some(p) = step_proof {
                    proof.merge(p);
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
            // 最後の段は畳まない。読み出しはread_or_textが畳み、書き込みは要素の位置をそのまま使う。
            if !fallible && position < steps.len() {
                target = constants::evaluate(&target)?.unwrap_or(target);
            }
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
            let step_proof = if *index {
                bounds::index(&target, start.as_ref().unwrap(), 0, has_handler, scope)
            } else {
                bounds::slice(&target, start.as_deref(), end.as_deref(), scope)
            };
            let proven = step_proof.is_some();
            if let Some(p) = step_proof {
                proof.merge(p);
            }
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
            if !fallible && position < steps.len() {
                target = constants::evaluate(&target)?.unwrap_or(target);
            }
        }
    }
    Ok((target, fallible, proof))
}

fn address(expr: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let (value, _, _) = access(expr, scope)?;
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
        target,
        fallback,
        fallback_location,
        ..
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
    let (mut result, fallible, proof) = access(expr, scope)?;
    if !fallible {
        if let Some(fb) = fallback {
            // 保証済みのアクセスに付いたorでも代替式の型と形式は通常どおり検査し、警告を報告する。
            // 到達しない代替値で条件集合を汚さないよう事前状態を復元する。
            bounds::with_fallback(scope, true, || match fb {
                FailureHandler::Value(v) => {
                    let _ = expression::coerced(v, Some(result.ty), scope)?;
                    Ok(())
                }
                FailureHandler::Return { value, .. } => {
                    let _ = super::functions::return_value(value, scope)?;
                    Ok(())
                }
                _ => Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.access-handler", "invalid array failure handler"),
                )),
            })?;
            let warn_loc = fallback_location.unwrap_or(expr.location);
            scope
                .outputs
                .redundant_array_handlers
                .borrow_mut()
                .entry((warn_loc.line, warn_loc.column))
                .or_insert_with(|| proof.related_diagnostics());
        }
        return Ok(constants::evaluate(&result)?.unwrap_or(result));
    }
    let exits_on_oob = match fallback {
        Some(FailureHandler::Return { .. }) => true,
        Some(FailureHandler::Value(v)) => matches!(v.kind, ExpressionKind::Panic(_)),
        _ => false,
    };
    let handler = match fallback {
        Some(FailureHandler::Value(v)) if bounds::is_facts_pure(v) => {
            CheckedHandler::Value(Box::new(expression::coerced(v, Some(result.ty), scope)?))
        }
        _ => bounds::with_fallback(scope, exits_on_oob, || match fallback {
            Some(FailureHandler::Value(v)) => Ok(CheckedHandler::Value(Box::new(
                expression::coerced(v, Some(result.ty), scope)?,
            ))),
            Some(FailureHandler::Return { value, location }) => Ok(CheckedHandler::Return {
                value: Box::new(super::functions::return_value(value, scope)?),
                location: *location,
            }),
            None => Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.access-handler", "array access requires or"),
            )),
            _ => Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.access-handler", "invalid array failure handler"),
            )),
        })?,
    };
    if exits_on_oob {
        bounds::record_checked_access(&result, scope);
    }
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
                "constant arrays cannot contain non-nil pointers or non-nil writable arrays"
            ),
        ));
    }
    value.ty = Type::ArrayRef {
        id,
        mutable: false,
        nilable: false,
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
        Type::Pointer { .. } if ty.is_nilable() => true,
        Type::Pointer { .. } | Type::CPointer | Type::Function(_) => false,
        Type::Struct(id) => scope
            .structs
            .get(id)
            .fields
            .iter()
            .all(|f| constant_element_type(f.ty, scope, seen)),
        Type::ArrayRef { nilable: true, .. } => true,
        Type::ArrayRef {
            id, mutable: false, ..
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
