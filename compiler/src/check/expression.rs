//! 式の型を推論して検査し、型と参照先を持つIRの式へ変換する。構文別の判定は子モジュールへ渡す。
//!
//! 利用元: [`crate::check::statements`]、[`crate::check::functions`]。
//! 主な対象規則: `[decl.local-scope]`、`[enum.access]`、`[expr.operator-types]`、`[lit.float-range]`、`[lit.range]`、`[num.fn-convert]`。

use crate::check::generic::domain;
use crate::check::scope::Scope;
pub(super) mod blocks;
mod branches;
pub(in crate::check) mod casts;
mod operators;
pub(super) mod slices;
pub(crate) mod structs;

use super::ir::{CheckedExpression, Value};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
};

// オペランドの型を保つ式をたどって、既知の型を探す。数値リテラルは既定型を選ぶ前に他方の型を受け取る。
// 比較結果のboolをオペランドの型として使わない。
pub(super) fn operand_type(expr: &Expression, bindings: &Scope) -> Option<Type> {
    let key = std::ptr::from_ref(expr) as usize;
    if bindings.hint_depth.get() > 0
        && let Some(ty) = bindings.type_hints.borrow().get(&key)
    {
        return *ty;
    }
    let ty = operand_type_inner(expr, bindings);
    if bindings.hint_depth.get() > 0 {
        bindings.type_hints.borrow_mut().insert(key, ty);
    }
    ty
}

fn operand_type_inner(expr: &Expression, bindings: &Scope) -> Option<Type> {
    if !bindings.named.needs.is_empty()
        && bindings.named.type_arguments.is_none()
        && let Some(ty) = super::type_parameters::hint(expr, bindings)
    {
        return Some(ty);
    }
    #[cfg(test)]
    super::generic::work();
    match &expr.kind {
        ExpressionKind::ArrayInit {
            values,
            lengths,
            key_groups,
            repeated,
            soa,
            ..
        } => {
            let element = values
                .first()
                .map(|v| operand_type(v, bindings).unwrap_or_else(|| default_type(v, bindings)))?;
            let dims: Vec<Option<crate::ast::EnumType>> = if *repeated {
                lengths
                    .iter()
                    .map(|length| match &length.kind {
                        ExpressionKind::Name(name)
                            if crate::parser::names::is_type_spelling(name) =>
                        {
                            bindings
                                .enums
                                .iter()
                                .find(|d| &d.name == name && !d.ty.is_error)
                                .map(|d| d.ty)
                        }
                        _ => None,
                    })
                    .collect()
            } else {
                key_groups
                    .iter()
                    .map(|group| {
                        group
                            .as_ref()
                            .and_then(|brackets| brackets.first())
                            .and_then(|keys| keys.first())
                            .and_then(|key| match &key.kind {
                                ExpressionKind::Variant { enum_name, .. } => bindings
                                    .enums
                                    .iter()
                                    .find(|d| &d.name == enum_name && !d.ty.is_error)
                                    .map(|d| d.ty),
                                _ => None,
                            })
                    })
                    .collect()
            };
            Some(
                bindings
                    .structs
                    .arrays
                    .borrow_mut()
                    .intern_dims(element, dims, *soa)
                    .array_reference(true),
            )
        }
        ExpressionKind::MethodCall {
            target,
            name,
            arguments,
            ..
        } => super::methods::hint(target, name, arguments, bindings),
        ExpressionKind::Receiver { target, .. } => {
            operand_type(target, bindings).map(|t| t.pointee().unwrap_or(t))
        }
        ExpressionKind::Closure(_) | ExpressionKind::Panic(_) => None,
        ExpressionKind::FunctionValue {
            name,
            type_arguments,
        } => {
            let signature = bindings.functions.get(name)?;
            if signature.type_parameters != 0 || !type_arguments.is_empty() {
                return super::type_parameters::function_value_type(
                    signature,
                    type_arguments,
                    bindings,
                    expr.location,
                )
                .ok();
            }
            if signature.is_generic() || name == "main" {
                return None;
            }
            Some(bindings.intern_function(crate::ast::FunctionType {
                parameters: signature.parameters.clone(),
                result: signature.result,
                errors: signature.errors.clone(),
            }))
        }
        ExpressionKind::Invoke { target, .. } => {
            let (Type::StaticFunction(id) | Type::Function(id)) = operand_type(target, bindings)?
            else {
                return None;
            };
            Some(bindings.function_types.borrow()[id].result)
        }
        ExpressionKind::Slice { target, index, .. } => {
            let target_type = operand_type(target, bindings);
            if let Some(id) = target_type.and_then(Type::array_id) {
                let readonly = target_type.is_some_and(Type::is_readonly_array);
                let (rank, element, sub_dims, soa) = {
                    let arrays = bindings.structs.arrays.borrow();
                    let a = arrays.get(id);
                    (
                        a.rank(),
                        a.element,
                        (a.rank() > 1).then(|| a.sub_dims()),
                        a.soa,
                    )
                };
                return Some(if !index {
                    Type::Array(id).array_reference(readonly)
                } else if rank == 1 {
                    element
                } else {
                    bindings
                        .structs
                        .arrays
                        .borrow_mut()
                        .intern_dims(element, sub_dims.unwrap(), soa)
                        .array_reference(readonly)
                });
            }
            Some(if *index { Type::Char } else { Type::Text })
        }
        ExpressionKind::New(value) => operand_type(value, bindings).map(|ty| {
            if let ExpressionKind::ArrayInit { readonly, .. } = &value.kind {
                Type::Array(ty.array_id().unwrap()).array_reference(*readonly)
            } else {
                ty.pointer()
            }
        }),
        ExpressionKind::Deref(value) => operand_type(value, bindings).and_then(Type::pointee),
        ExpressionKind::Nil => None,
        ExpressionKind::CBytes(_) => Some(Type::CPointer),
        ExpressionKind::CByteLen(_) => Some(Type::CInteger(crate::ast::CIntegerType::Size)),
        ExpressionKind::Len(inner) => {
            if bindings.has_operator_methods
                && super::operator_methods::owner(inner, bindings).is_some()
            {
                super::operator_methods::unary_hint("len", inner, bindings)
            } else {
                Some(Type::Integer(IntegerType::Len))
            }
        }
        ExpressionKind::TextAppend { .. }
        | ExpressionKind::Template(_)
        | ExpressionKind::TextBuffer { .. } => Some(Type::Text),
        ExpressionKind::Struct { ty, .. } => bindings.resolve_type_at(*ty, expr.location).ok(),
        ExpressionKind::With { target, .. } => operand_type(target, bindings),
        ExpressionKind::Field { target, name } => {
            let known = operand_type(target, bindings)?;
            if let Some(array_id) = known.array_id()
                && !known.is_nilable()
            {
                let soa_struct = {
                    let arrays = bindings.structs.arrays.borrow();
                    let array = arrays.get(array_id);
                    if array.soa
                        && let Type::Struct(id) = array.element
                    {
                        Some((id, array.dims.clone()))
                    } else {
                        None
                    }
                };
                if let Some((id, dims)) = soa_struct {
                    let field_ty = bindings
                        .structs
                        .get(id)
                        .fields
                        .iter()
                        .find(|field| &field.name == name)
                        .map(|field| field.ty)?;
                    let col_soa = matches!(field_ty, Type::Struct(_));
                    return Some(
                        bindings
                            .structs
                            .arrays
                            .borrow_mut()
                            .intern_dims(field_ty, dims, col_soa)
                            .array_reference(known.is_readonly_array()),
                    );
                }
            }
            let Type::Struct(id) = known.pointee().unwrap_or(known) else {
                return None;
            };
            bindings
                .structs
                .get(id)
                .fields
                .iter()
                .find(|field| &field.name == name)
                .map(|field| field.ty)
        }
        ExpressionKind::Variant { enum_name, .. } => bindings
            .enums
            .iter()
            .find(|e| &e.name == enum_name)
            .map(|e| Type::Enum(e.ty)),
        ExpressionKind::Void => Some(Type::Void),
        ExpressionKind::Call {
            name,
            arguments,
            type_arguments,
        } => {
            if let Some(signature) = bindings
                .functions
                .get(name)
                .filter(|s| s.type_parameters != 0)
            {
                return super::type_parameters::call_hint(
                    signature,
                    type_arguments,
                    arguments,
                    bindings,
                    expr.location,
                );
            }
            super::generic::call_operand_type(name, arguments, bindings)
        }
        ExpressionKind::Block { statements, value } => {
            blocks::operand_type(statements, value, bindings)
        }
        ExpressionKind::Match { arms, .. } => {
            branch_type(arms.iter().map(|arm| &arm.body), bindings)
        }
        ExpressionKind::Cast { target, .. } => {
            bindings.resolve_type_at(*target, expr.location).ok()
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => branch_type(
            branches
                .iter()
                .map(|(_, value)| value)
                .chain(std::iter::once(otherwise.as_ref())),
            bindings,
        ),
        ExpressionKind::Char(_) => Some(Type::Char),
        ExpressionKind::Integer { .. } | ExpressionKind::Float(_) => None,
        ExpressionKind::Name(name) => bindings.get(name).map(|binding| binding.ty),
        ExpressionKind::Negate(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Not(inner)
            if bindings.has_operator_methods
                && super::operator_methods::owner(inner, bindings).is_some() =>
        {
            super::operator_methods::unary_hint(
                super::operator_methods::unary(expr).unwrap().0,
                inner,
                bindings,
            )
        }
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner)
        | ExpressionKind::Negate(inner) => operand_type(inner, bindings),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let a = operand_type(left, bindings);
            if bindings.has_operator_methods
                && super::operator_methods::supported(*operator)
                && matches!(a, Some(Type::Struct(_) | Type::Enum(_)))
            {
                return super::operator_methods::hint(operator.symbol(), left, right, bindings);
            }
            let b = operand_type(right, bindings);
            if operator.is_shift() {
                return a;
            }
            if operator.is_arithmetic() {
                let floats = [a, b].into_iter().flatten().filter(|t| t.is_float());
                if let Some(ty) = joined_type(floats.map(Some)) {
                    return Some(ty);
                }
                if has_float_source(left, bindings) || has_float_source(right, bindings) {
                    return None;
                }
            }
            match (a, b) {
                (Some(a), Some(b)) => Some(joined_or_first(a, b)),
                _ => a.or(b),
            }
        }

        ExpressionKind::Bool(_)
        | ExpressionKind::Not(_)
        | ExpressionKind::Logical { .. }
        | ExpressionKind::Comparison { .. } => Some(Type::Bool),
    }
}

// 浮動小数点リテラルと汎用floatの結果は、浮動小数点演算を選択する。
// 結果を作るオペランドだけを調べ、条件や型変換前の値は含めない。
pub(super) fn has_float_source(expr: &Expression, bindings: &Scope) -> bool {
    // 算術式の型ヒントから繰り返し呼ばれる探索。再帰中の判定と呼び出し枠を
    // 既存の整数式へ持ち込まないよう、定義の有無は入口で一度だけ選ぶ。
    if bindings.has_operator_methods {
        float_source::<true>(expr, bindings)
    } else {
        float_source::<false>(expr, bindings)
    }
}

fn float_source<const OPERATOR_METHODS: bool>(expr: &Expression, bindings: &Scope) -> bool {
    let key = std::ptr::from_ref(expr) as usize;
    if bindings.hint_depth.get() > 0
        && let Some(found) = bindings.float_hints.borrow().get(&key)
    {
        return *found;
    }
    let found = has_float_source_inner::<OPERATOR_METHODS>(expr, bindings);
    if bindings.hint_depth.get() > 0 {
        bindings.float_hints.borrow_mut().insert(key, found);
    }
    found
}

fn has_float_source_inner<const OPERATOR_METHODS: bool>(
    expr: &Expression,
    bindings: &Scope,
) -> bool {
    match &expr.kind {
        ExpressionKind::MethodCall { .. }
        | ExpressionKind::Receiver { .. }
        | ExpressionKind::Invoke { .. }
        | ExpressionKind::Name(_)
        | ExpressionKind::Field { .. }
        | ExpressionKind::Cast { .. } => operand_type(expr, bindings).is_some_and(Type::is_float),
        ExpressionKind::Call { name, .. } => {
            if bindings.contains_key(name) {
                operand_type(expr, bindings).is_some_and(Type::is_float)
            } else {
                bindings
                    .functions
                    .get(name)
                    .is_some_and(|f| f.result.is_float())
            }
        }
        ExpressionKind::Match { arms, .. } => arms
            .iter()
            .any(|arm| float_source::<OPERATOR_METHODS>(&arm.body, bindings)),
        ExpressionKind::Block { statements, value } => {
            blocks::has_float_source(statements, value, bindings)
        }
        ExpressionKind::Float(_) => true,
        ExpressionKind::Negate(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Not(inner)
            if OPERATOR_METHODS && super::operator_methods::owner(inner, bindings).is_some() =>
        {
            super::operator_methods::unary_hint(
                super::operator_methods::unary(expr).unwrap().0,
                inner,
                bindings,
            )
            .is_some_and(Type::is_float)
        }
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner)
        | ExpressionKind::Negate(inner) => float_source::<OPERATOR_METHODS>(inner, bindings),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            if OPERATOR_METHODS
                && super::operator_methods::supported(*operator)
                && super::operator_methods::owner(left, bindings).is_some()
            {
                return super::operator_methods::hint(operator.symbol(), left, right, bindings)
                    .is_some_and(Type::is_float);
            }
            float_source::<OPERATOR_METHODS>(left, bindings)
                || (!operator.is_shift() && float_source::<OPERATOR_METHODS>(right, bindings))
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            branches
                .iter()
                .any(|(_, value)| float_source::<OPERATOR_METHODS>(value, bindings))
                || float_source::<OPERATOR_METHODS>(otherwise, bindings)
        }
        _ => false,
    }
}
fn default_type(expr: &Expression, bindings: &Scope) -> Type {
    if has_float_source(expr, bindings) {
        Type::Float(FloatType::F64)
    } else {
        Type::Integer(IntegerType::I64)
    }
}
pub(in crate::check) fn float_literal(
    text: &str,
    ty: FloatType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let invalid = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("lit.float-range", "floating-point literal is out of range"),
        )
    };
    let bits = float_bits(text, ty).ok_or_else(invalid)?;
    Ok(CheckedExpression {
        ty: Type::Float(ty),
        value: Value::Float(bits),
    })
}

pub(in crate::check) fn float_bits(text: &str, ty: FloatType) -> Option<u64> {
    let special = matches!(text, "nan" | "-nan" | "inf" | "-inf");
    let text = text.replace('_', "");
    let bits = match ty {
        FloatType::F32 => {
            let value = text.parse::<f32>().ok()?;
            if value.is_infinite() && !special {
                return None;
            }
            u64::from(value.to_bits())
        }
        FloatType::F64 => {
            let value = text.parse::<f64>().ok()?;
            if value.is_infinite() && !special {
                return None;
            }
            value.to_bits()
        }
    };
    Some(bits)
}

// 型のヒントは、生存している1つのAST式の検査中だけ共有する。境界で消去し、
// ポインターを鍵とする項目が式や束縛より長く残らないようにする。
struct HintSession<'a>(&'a Scope);
impl<'a> HintSession<'a> {
    fn new(scope: &'a Scope) -> Self {
        if scope.hint_depth.get() == 0 {
            scope.type_hints.borrow_mut().clear();
            scope.float_hints.borrow_mut().clear();
        }
        scope.hint_depth.set(scope.hint_depth.get() + 1);
        Self(scope)
    }
}
impl Drop for HintSession<'_> {
    fn drop(&mut self) {
        self.0.hint_depth.set(self.0.hint_depth.get() - 1);
        if self.0.hint_depth.get() == 0 {
            self.0.type_hints.borrow_mut().clear();
            self.0.float_hints.borrow_mut().clear();
        }
    }
}

pub(super) fn expression(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, expected, bindings, Widening::None, None)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

// 数値の拡大変換は、conv.widen-placesに列挙した文脈だけで有効にする。
pub(super) fn coerced(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, expected, bindings, Widening::All, None)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

// この文脈は引数の境界だけに適用する。括弧はその境界を保ち、
// その他の部分式はそれぞれ通常の検査文脈を使う。
pub(super) fn generic_argument(
    expr: &Expression,
    expected: Type,
    family: super::generic::Family,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, Some(expected), bindings, Widening::All, Some(family))?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

#[derive(Clone, Copy)]
enum Widening {
    None,
    All,
    FloatDestination,
}
impl Widening {
    fn accepts(self, from: Type, to: Type) -> bool {
        from == to
            || from.lifts_to(to)
            || (match self {
                Self::None => false,
                Self::All => true,
                Self::FloatDestination => to.is_float(),
            } && super::exact::widening(from, to))
    }
}

pub(super) fn floating_operand(
    expr: &Expression,
    ty: Type,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let _hints = HintSession::new(bindings);
    let checked = expression_inner(expr, Some(ty), bindings, Widening::FloatDestination, None)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

// 型検査と同時に必要な解析を記録し、無関係な本体をもう一度走査しない。
fn expression_inner(
    expr: &Expression,
    required: Option<Type>,
    bindings: &Scope,
    widen: Widening,
    argument: Option<super::generic::Family>,
) -> Result<CheckedExpression, Diagnostic> {
    let checked = expression_checked(expr, required, bindings, widen, argument)?;
    super::arena::observe_type(checked.ty, bindings);
    if matches!(
        &checked.value,
        Value::Call { .. } | Value::Invoke { .. } | Value::HandledCall { .. }
    ) || matches!(&checked.value, Value::Comparison { methods, .. } if methods.iter().any(Option::is_some))
    {
        bindings.calls_used.set(true);
    }
    Ok(checked)
}

fn expression_checked(
    expr: &Expression,
    required: Option<Type>,
    bindings: &Scope,
    widen: Widening,
    argument: Option<super::generic::Family>,
) -> Result<CheckedExpression, Diagnostic> {
    if required.is_some_and(|ty| ty.pointee().is_some() && matches!(ty.leaf(), Type::Function(_))) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("arena.ptr-type", "pointer requires a stored value type"),
        ));
    }
    if let Some(Type::CInteger(_)) = required
        && let Some(value) = super::ffi::literal(expr, required.unwrap())?
    {
        return Ok(value);
    }
    // 互換性のない数値の種類は独立して推論する。入れ子の汎用呼び出しでも、
    // 変換の診断は外側の仮引数に対して報告する。
    let inference_hint = if argument == Some(super::generic::Family::Int)
        && (has_float_source(expr, bindings)
            || operand_type(expr, bindings).is_some_and(Type::is_float))
    {
        None
    } else {
        required
    };
    let checked = if bindings.named.type_arguments.is_none()
        && !bindings.named.type_parameters.is_empty()
        && let Some(checked) = super::type_parameters::operation(expr, bindings)?
    {
        checked
    } else if bindings.has_operator_methods
        && let ExpressionKind::Binary {
            operator,
            left,
            right,
        } = &expr.kind
        && super::operator_methods::owner(left, bindings).is_some()
        && super::operator_methods::supported(*operator)
    {
        super::operator_methods::call(
            expr,
            operator.symbol(),
            left,
            right,
            inference_hint,
            bindings,
        )?
    } else if bindings.has_operator_methods
        && let Some((op, operand)) = super::operator_methods::unary(expr)
        && super::operator_methods::owner(operand, bindings).is_some()
    {
        super::operator_methods::unary_call(expr, op, operand, inference_hint, bindings)?
    } else if super::exact::is_operator(expr) && super::exact::integer(expr, bindings) {
        super::exact::check(expr, inference_hint, bindings, false)?
    } else {
        match &expr.kind {
            ExpressionKind::ArrayInit { .. } => {
                super::arrays::constant_initializer(expr, required, bindings)?
            }
            ExpressionKind::MethodCall { .. } => {
                super::methods::call(expr, inference_hint, bindings, None)?
            }
            ExpressionKind::Receiver { target, mutable } => {
                super::methods::receiver(target, *mutable, bindings)?
            }
            ExpressionKind::Panic(parts) => super::errors::panic(
                parts,
                expr.location,
                required.unwrap_or(Type::Void),
                bindings,
            )?,
            ExpressionKind::Closure(_) => {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "closure.place",
                        "closures are only allowed as direct arguments to fn parameters"
                    ),
                ));
            }
            ExpressionKind::FunctionValue {
                name,
                type_arguments,
            } => super::function_values::value(expr, name, type_arguments, required, bindings)?,
            ExpressionKind::Invoke { target, arguments } => {
                super::function_values::invoke(expr, target, arguments, bindings, None)?
            }
            ExpressionKind::Slice { .. } => super::arrays::read_or_text(expr, bindings)?,
            ExpressionKind::TextAppend { .. }
            | ExpressionKind::TextBuffer { .. }
            | ExpressionKind::Template(_) => super::text::expression(expr, required, bindings)?,
            ExpressionKind::Struct { .. }
            | ExpressionKind::With { .. }
            | ExpressionKind::Field { .. } => structs::check(expr, bindings)?,
            ExpressionKind::Variant { enum_name, variant } => {
                let decl = bindings
                    .enums
                    .iter()
                    .find(|e| &e.name == enum_name)
                    .ok_or_else(|| {
                        expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("enum.access", "unknown enum type"),
                        )
                    })?;
                let value = decl
                    .variants
                    .iter()
                    .position(|name| name == variant)
                    .ok_or_else(|| {
                        expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("enum.access", "unknown enum variant"),
                        )
                    })?;
                CheckedExpression {
                    ty: Type::Enum(decl.ty),
                    value: Value::Integer {
                        value: value as i128,
                        minus: None,
                    },
                }
            }
            ExpressionKind::Void => CheckedExpression {
                ty: Type::Void,
                value: Value::Void,
            },
            ExpressionKind::Call {
                name, arguments, ..
            } => super::functions::call(expr, name, arguments, inference_hint, bindings, None)?,
            ExpressionKind::Block { statements, value } => {
                blocks::check(statements, value, inference_hint, bindings)?
            }
            ExpressionKind::New(_) | ExpressionKind::Deref(_) | ExpressionKind::Nil => {
                super::arena::expression(expr, required, bindings)?
            }
            ExpressionKind::CBytes(inner) | ExpressionKind::CByteLen(inner) => {
                super::ffi::bytes(expr, inner, bindings)?
            }
            ExpressionKind::Len(inner) => {
                let operand = expression(inner, None, bindings)?;
                if !operand.ty.is_text()
                    && !(!operand.ty.is_nilable() && operand.ty.array_id().is_some())
                {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        if matches!(operand.ty, Type::Struct(_) | Type::Enum(_)) {
                            crate::diagnostic!(
                                "operator.unary",
                                "no unary operator len defined for this type"
                            )
                        } else {
                            crate::diagnostic!("len.targets", "len requires symbol or text")
                        },
                    ));
                }
                CheckedExpression {
                    ty: Type::Integer(IntegerType::Len),
                    value: Value::Len(Box::new(operand)),
                }
            }
            ExpressionKind::ArithmeticHandler { .. }
            | ExpressionKind::BitNot(_)
            | ExpressionKind::WrapNegate(_)
            | ExpressionKind::SaturatingNegate(_)
            | ExpressionKind::Negate(_)
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Not(_)
            | ExpressionKind::Logical { .. }
            | ExpressionKind::Comparison { .. } => {
                operators::check(expr, inference_hint, bindings)?
            }
            ExpressionKind::Cast { .. } => casts::check(expr, bindings)?,
            ExpressionKind::Float(text) => literal(
                text,
                false,
                true,
                inference_hint.unwrap_or(Type::Float(FloatType::F64)),
                expr.location,
                bindings,
            )?,
            ExpressionKind::Match { .. } | ExpressionKind::If { .. } => {
                branches::check(expr, inference_hint, bindings)?
            }
            ExpressionKind::Integer { digits, negative } => literal(
                digits,
                *negative,
                false,
                inference_hint.unwrap_or(Type::Integer(IntegerType::I64)),
                expr.location,
                bindings,
            )?,

            ExpressionKind::Char(value) => CheckedExpression {
                ty: Type::Char,
                value: Value::Integer {
                    value: i128::from(*value),
                    minus: None,
                },
            },
            ExpressionKind::Bool(value) => CheckedExpression {
                ty: Type::Bool,
                value: Value::Bool(*value),
            },
            ExpressionKind::Name(name) => {
                if bindings.functions.contains_key(name) {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "fn.value-mark",
                            "use fn {name} to create a function value"
                        ),
                    ));
                }
                let binding = bindings.get(name).ok_or_else(|| {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        if bindings.uncaptured.contains(name) {
                            crate::diagnostic!(
                                "closure.scope",
                                "outer binding '{name}' must be listed in the capture list"
                            )
                        } else if name == "self" {
                            crate::diagnostic!(
                                "method.self-keyword",
                                "self is only available in a method or an explicit capture"
                            )
                        } else {
                            crate::diagnostic!(
                                "decl.local-scope",
                                "binding '{name}' is not in scope"
                            )
                        },
                    )
                })?;
                if matches!(binding.ty, Type::Function(_)) {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "fn.call-only",
                            "call-only parameters can only be called or forwarded by name"
                        ),
                    ));
                }
                binding.constant.clone().unwrap_or(CheckedExpression {
                    ty: binding.ty,
                    value: Value::Local(binding.id),
                })
            }
            // 括弧は引数の境界を保つ。新しい式の文脈や独立した変換を作らない。
            ExpressionKind::Group(inner) => {
                expression_inner(inner, required, bindings, widen, argument)?
            }
        }
    };

    if matches!(checked.ty, Type::Pointer { shape, .. } if shape.depth() > 256) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("arena.ptr-type", "pointer type nesting exceeds 256"),
        ));
    }
    let generic_result = matches!(&expr.kind, ExpressionKind::Call { name, .. }
        if bindings.functions.get(name).is_some_and(|f| f.result.generic()))
        || super::methods::generic_result(expr, bindings);
    let conversion = required.map(|to| {
        if checked.ty == to {
            (domain::Judgment::Valid, 0)
        } else {
            domain::judge_with_failures(checked.ty, to, |from, to| widen.accepts(from, to))
        }
    });
    if let Some((domain::Judgment::Deferred, failures)) = conversion {
        let (from, to, location) = (checked.ty, required.unwrap(), expr.location);
        bindings.defer_check(failures, move |i| {
            let (from, to) = (domain::at(from, i).unwrap(), domain::at(to, i).unwrap());
            conversion_error(
                location,
                from,
                to,
                argument,
                generic_result,
                from.name(),
                to.name(),
            )
        });
    }
    if let Some(ty) = required
        && ty != checked.ty
        && conversion.is_some_and(|(judgment, _)| judgment.possible())
    {
        return Ok(CheckedExpression {
            ty,
            value: Value::Cast(Box::new(checked)),
        });
    }
    if required == Some(Type::Text) && checked.ty == Type::Symbol {
        return Ok(CheckedExpression {
            ty: Type::Text,
            value: Value::TextView(Box::new(checked)),
        });
    }
    if required.is_some_and(|ty| !domain::same(ty, checked.ty)) {
        return Err(conversion_error(
            expr.location,
            checked.ty,
            required.unwrap(),
            argument,
            generic_result,
            &bindings.type_name(checked.ty),
            &bindings.type_name(required.unwrap()),
        ));
    }
    Ok(checked)
}

fn conversion_error(
    location: Location,
    from: Type,
    to: Type,
    argument: Option<super::generic::Family>,
    generic_result: bool,
    from_name: &str,
    to_name: &str,
) -> Diagnostic {
    if Type::is_numeric(from) && Type::is_numeric(to) && (argument.is_some() || generic_result) {
        let message = if argument == Some(super::generic::Family::Int) && from.is_float() {
            crate::diagnostic!(
                "num.fn-convert",
                "floating-point values cannot be passed to an int parameter"
            )
            .to_owned()
        } else {
            crate::diagnostic!(
                "num.fn-convert",
                "{} {} cannot be {} as {} without loss",
                if argument.is_some() {
                    "argument"
                } else {
                    "generic result"
                },
                from_name,
                if argument.is_some() {
                    "passed"
                } else {
                    "received"
                },
                to_name
            )
        };
        return location.error(ErrorKind::InvalidSource, message);
    }
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!("type.expected", "expected {}, found {}", to_name, from_name),
    )
}

// 他の分岐にも型の手がかりがなければ、newの初期値に通常の既定型を使う。
// 既知の分岐がある場合は先にそちらを使い、数値リテラルの文脈依存を保つ。
fn allocation_type(value: &Expression, bindings: &Scope) -> Option<Type> {
    match &value.kind {
        ExpressionKind::New(inner) => Some(
            operand_type(inner, bindings)
                .or_else(|| allocation_type(inner, bindings))
                .unwrap_or_else(|| default_type(inner, bindings))
                .pointer(),
        ),
        ExpressionKind::Group(inner) | ExpressionKind::Block { value: inner, .. } => {
            allocation_type(inner, bindings)
        }
        _ => None,
    }
}

fn has_nil_branch(value: &Expression) -> bool {
    match &value.kind {
        ExpressionKind::Nil => true,
        ExpressionKind::Group(inner) | ExpressionKind::Block { value: inner, .. } => {
            has_nil_branch(inner)
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => branches.iter().any(|(_, v)| has_nil_branch(v)) || has_nil_branch(otherwise),
        ExpressionKind::Match { arms, .. } => arms.iter().any(|a| has_nil_branch(&a.body)),
        _ => false,
    }
}

// nilの分岐には、他の分岐から決めた参照先の型を要求する。
fn branch_type<'a>(
    values: impl Iterator<Item = &'a Expression> + Clone,
    bindings: &Scope,
) -> Option<Type> {
    let ty = joined_type(values.clone().map(|v| operand_type(v, bindings))).or_else(|| {
        let mut result: Option<Type> = None;
        for ty in values.clone().filter_map(|v| allocation_type(v, bindings)) {
            if result.is_some_and(|previous| previous != ty) {
                return None;
            }
            result = Some(ty);
        }
        result
    })?;
    if ty.is_reference() && values.clone().any(has_nil_branch) {
        if matches!(ty, Type::Pointer { shape, .. } if shape.depth() > 256) {
            return None;
        }
        Some(ty.with_nilable(true))
    } else {
        Some(ty)
    }
}

pub(super) fn joined_type(types: impl Iterator<Item = Option<Type>>) -> Option<Type> {
    types.flatten().reduce(joined_or_first)
}

// 代替の型選択も型の代入ごとに適用する。共通型がなくても、
// 先頭オペランドの型への通常の変換判定まで進む。
fn joined_or_first(a: Type, b: Type) -> Type {
    if a.is_text() && b.is_text() && a != b {
        return Type::Text;
    }
    if let (Type::Pointer { .. }, Type::Pointer { .. }) = (a, b)
        && a.pointee() == b.pointee()
    {
        return a
            .with_readonly(a.is_readonly_pointer() || b.is_readonly_pointer())
            .with_nilable(a.is_nilable() || b.is_nilable());
    }
    if let (
        Type::ArrayRef {
            id: a_id,
            mutable: a_mut,
            nilable: a_nil,
        },
        Type::ArrayRef {
            id: b_id,
            mutable: b_mut,
            nilable: b_nil,
        },
    ) = (a, b)
        && a_id == b_id
    {
        return Type::ArrayRef {
            id: a_id,
            mutable: a_mut && b_mut,
            nilable: a_nil || b_nil,
        };
    }
    if a.lifts_to(b) {
        return b;
    }
    if b.lifts_to(a) {
        return a;
    }
    if a.symbolic() || b.symbolic() {
        return domain::from_types(std::array::from_fn(|i| {
            let (a, b) = (domain::at(a, i)?, domain::at(b, i)?);
            Some(super::exact::join(a, b).unwrap_or(a))
        }))
        .unwrap_or(a);
    }
    super::exact::join(a, b).unwrap_or(a)
}

pub(in crate::check) fn integer_literal(
    digits: &str,
    negative: bool,
    ty: IntegerType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let value = digits
        .parse::<i128>()
        .ok()
        .and_then(|n| if negative { n.checked_neg() } else { Some(n) });
    let value = integer_literal_value(value, negative, ty, location)?;
    Ok(CheckedExpression {
        ty: Type::Integer(ty),
        value: Value::Integer {
            value,
            minus: negative.then_some(location),
        },
    })
}
fn range_literal(
    text: &str,
    negative: bool,
    ty: crate::ast::RangeType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let digits = text.replace('_', "");
    let value = digits
        .parse::<i128>()
        .ok()
        .and_then(|n| if negative { n.checked_neg() } else { Some(n) });
    let fail = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "lit.range",
                "integer literal is out of range for range type"
            ),
        )
    };
    let value = value.ok_or_else(fail)?;
    let (min, max) = ty.bounds();
    if value < min || value > max {
        return Err(fail());
    }
    if negative && ty.base.bounds().0 == 0 {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "expr.operator-types",
                "integer negation requires a signed type"
            ),
        ));
    }
    Ok(CheckedExpression {
        ty: Type::Range(ty),
        value: Value::Integer {
            value,
            minus: negative.then_some(location),
        },
    })
}
fn integer_literal_value(
    value: Option<i128>,
    negative: bool,
    ty: IntegerType,
    location: Location,
) -> Result<i128, Diagnostic> {
    let fail = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "lit.range",
                "integer literal is out of range for {}",
                ty.name()
            ),
        )
    };
    let value = value.ok_or_else(fail)?;
    let (min, max) = ty.bounds();
    if value < min || value > max {
        return Err(fail());
    }
    if negative && min == 0 {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "expr.operator-types",
                "integer negation requires a signed type"
            ),
        ));
    }
    Ok(value)
}

// リテラルの規則は通常検査と記号的な検査で同じスカラー検査を使う。
// 型によって表現が変わる場合は、具体化まで元の綴りを保持する。
fn literal(
    text: &str,
    negative: bool,
    floating: bool,
    ty: Type,
    location: Location,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if !ty.symbolic() {
        return scalar_literal(text, negative, floating, ty, location);
    }
    let results: [_; domain::NUMERIC_COMBINATIONS] = std::array::from_fn(|i| {
        domain::at(ty, i).map(|ty| scalar_literal(text, negative, floating, ty, location))
    });
    let representative = results.iter().flatten().find_map(|r| r.as_ref().ok());
    let Some(representative) = representative else {
        // 宣言の診断に任意のスカラー型名を使わず、記号的な名前を保つ。
        // 値域の判定には通常のリテラル規則を使う。
        let mut diagnostic = location.error(
            ErrorKind::InvalidSource,
            if floating && !ty.is_float() {
                crate::diagnostic!(
                    "type.expected",
                    "a floating-point literal requires a floating-point type"
                )
                .to_owned()
            } else if ty.is_integer() {
                crate::diagnostic!("lit.range", "integer literal is out of range for int")
                    .to_owned()
            } else if !floating
                && results.iter().flatten().any(|result| {
                    result
                        .as_ref()
                        .is_err_and(|error| error.rule() == "lit.integer-as-float")
                })
            {
                crate::diagnostic!(
                    "lit.integer-as-float",
                    "{}{} cannot be represented exactly as float; write {}{}.0 or use as",
                    if negative { "-" } else { "" },
                    text,
                    if negative { "-" } else { "" },
                    text
                )
                .to_owned()
            } else {
                crate::diagnostic!(
                    "lit.float-range",
                    "floating-point literal is out of range for float"
                )
                .to_owned()
            },
        );
        if diagnostic.rule() == "lit.integer-as-float" {
            // 記号的なfloatを任意の具体型で代表させず、各候補の診断を示す。
            // 他の数値型との組み合わせで同じfloat型が現れても、一度だけ示す。
            let mut seen = Vec::new();
            for (i, result) in results.iter().enumerate() {
                let Some(Type::Float(concrete)) = domain::at(ty, i) else {
                    continue;
                };
                if seen.contains(&concrete) {
                    continue;
                }
                seen.push(concrete);
                if let Some(Err(cause)) = result {
                    diagnostic
                        .related
                        .push(if cause.rule() == "lit.float-range" {
                            location.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "lit.float-range",
                                    "{}{} is out of range for {}",
                                    if negative { "-" } else { "" },
                                    text,
                                    concrete.name()
                                ),
                            )
                        } else {
                            cause.clone()
                        });
                }
            }
        }
        return Err(diagnostic);
    };
    bindings.defer_rule(|i| results[i].as_ref()?.as_ref().err().cloned());
    Ok(CheckedExpression {
        ty,
        value: if ty.is_integer() {
            representative.value.clone()
        } else {
            Value::GenericNumericLiteral {
                text: text.to_owned(),
                negative,
                floating,
                location,
            }
        },
    })
}

pub(in crate::check) fn scalar_literal(
    text: &str,
    negative: bool,
    floating: bool,
    ty: Type,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    match ty {
        Type::Integer(ty) if !floating => integer_literal(text, negative, ty, location),
        Type::Range(ty) if !floating => range_literal(text, negative, ty, location),
        Type::Float(ty) => {
            let spelling = if negative {
                format!("-{text}")
            } else {
                text.to_owned()
            };
            let checked = float_literal(&spelling, ty, location)?;
            if !floating {
                let Value::Float(bits) = checked.value else {
                    unreachable!()
                };
                let value = match ty {
                    FloatType::F32 => f64::from(f32::from_bits(bits as u32)),
                    FloatType::F64 => f64::from_bits(bits),
                };
                // 整数型へのキャストは大きな値を飽和させるため使わない。
                // 整数リテラルを丸めた値は整数なので、小数点以下0桁で
                // 正確な十進表記を取り出し、元の桁列と比較できる。
                let digits = text.replace('_', "");
                if format!("{:.0}", value.abs()).trim_start_matches('0')
                    != digits.trim_start_matches('0')
                {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "lit.integer-as-float",
                            "{} cannot be represented exactly as {} (nearest is {:.1}); write {}.0 or use as",
                            spelling,
                            ty.name(),
                            value,
                            spelling
                        ),
                    ));
                }
            }
            Ok(checked)
        }
        _ => Err(location.error(
            ErrorKind::InvalidSource,
            if floating {
                crate::diagnostic!(
                    "type.expected",
                    "a floating-point literal requires a floating-point type"
                )
            } else {
                crate::diagnostic!(
                    "type.expected",
                    "an integer literal requires a numeric type"
                )
            },
        )),
    }
}
