//! メソッドを暗黙のself引数を持つ関数へ接続する。受け手の更新許可は呼び出し位置で検査する。
use super::{
    expression, functions,
    ir::{CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Type},
};

fn name(ty: Type, member: &str, scope: &Scope) -> Option<String> {
    let owner = match ty.pointee().unwrap_or(ty) {
        Type::Struct(id) => &scope.structs[id].name,
        Type::Enum(ty) if !ty.is_error => &scope.enums.iter().find(|e| e.ty == ty)?.name,
        _ => return None,
    };
    Some(format!("{owner}.{member}"))
}

fn arguments(target: &Expression, mutable: bool, arguments: &[Expression]) -> Vec<Expression> {
    let mut result = Vec::with_capacity(arguments.len() + 1);
    result.push(Expression {
        kind: ExpressionKind::Receiver {
            target: Box::new(target.clone()),
            mutable,
        },
        location: target.location,
        height: target.height,
    });
    result.extend_from_slice(arguments);
    result
}

pub(super) fn hint(
    target: &Expression,
    member: &str,
    args: &[Expression],
    scope: &Scope,
) -> Option<Type> {
    let ty = expression::operand_type(target, scope)?;
    let name = name(ty, member, scope)?;
    let Some(signature) = scope.functions.get(&name) else {
        let Type::Struct(id) = ty.pointee().unwrap_or(ty) else {
            return None;
        };
        let field = scope.structs[id].fields.iter().find(|f| f.name == member)?;
        let (Type::StaticFunction(shape) | Type::Function(shape)) = field.ty else {
            return None;
        };
        return Some(scope.function_types.borrow()[shape].result);
    };
    // 一時的に作るself引数のアドレスを、元のASTのヒントキャッシュへ残さない。
    let mut hints = scope.clone();
    hints.type_hints = Default::default();
    hints.float_hints = Default::default();
    super::generic::call_operand_type(
        &name,
        &arguments(target, signature.method?.mutable, args),
        &hints,
    )
}

pub(super) fn generic_result(expr: &Expression, scope: &Scope) -> bool {
    let ExpressionKind::MethodCall {
        target,
        name: member,
        ..
    } = &expr.kind
    else {
        return false;
    };
    expression::operand_type(target, scope)
        .and_then(|ty| name(ty, member, scope))
        .and_then(|name| scope.functions.get(&name))
        .is_some_and(|f| f.result.generic())
}

pub(super) fn call(
    expr: &Expression,
    expected: Option<Type>,
    scope: &Scope,
    handler: Option<&FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    // self引数や関数値フィールドの一時ASTは、この呼び出しの検査中だけ生存する。
    // アドレスを鍵にした型ヒントを、次の呼び出しへ持ち越さない。
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    let result = call_inner(expr, expected, scope, handler);
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    result
}

fn call_inner(
    expr: &Expression,
    expected: Option<Type>,
    scope: &Scope,
    handler: Option<&FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::MethodCall {
        target,
        name: member,
        mutable,
        arguments: args,
    } = &expr.kind
    else {
        unreachable!()
    };
    let mutable = *mutable;
    let ty = match expression::operand_type(target, scope) {
        Some(ty) => ty,
        None => expression::expression(target, None, scope)?.ty,
    };
    let resolved = name(ty, member, scope);
    if let Some(name) = resolved
        .as_ref()
        .filter(|name| scope.functions.contains_key(*name))
    {
        let method = scope.functions[name].method.expect("method signature");
        if method.mutable != mutable {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "method.call",
                    "method receiver symbol does not match its declaration"
                ),
            ));
        }
        return functions::call(
            expr,
            name,
            &arguments(target, mutable, args),
            expected,
            scope,
            handler,
        );
    }
    // フィールドに格納した関数値は従来の呼び出しを使う。名前衝突は宣言時に拒否する。
    if !mutable
        && let Type::Struct(id) = ty.pointee().unwrap_or(ty)
        && scope.structs[id].fields.iter().any(|f| f.name == *member)
    {
        let field = Expression {
            kind: ExpressionKind::Field {
                target: target.clone(),
                name: member.into(),
            },
            location: expr.location,
            height: target.height + 1,
        };
        return super::function_values::invoke(expr, &field, args, scope, handler);
    }
    Err(expr.location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!("method.call", "unknown method '{member}'"),
    ))
}

/// メンバーの経路を構成する名前。ポインタの参照先を通るかは型検査後に判定する。
pub(super) fn path(expr: &Expression) -> Option<(&str, Vec<String>)> {
    match &expr.kind {
        ExpressionKind::Name(name) => Some((name, vec![])),
        ExpressionKind::Group(inner) => path(inner),
        ExpressionKind::Field { target, name } => {
            let (root, mut fields) = path(target)?;
            fields.push(name.clone());
            Some((root, fields))
        }
        _ => None,
    }
}

pub(super) fn receiver(
    target: &Expression,
    mutable: bool,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let mut value = expression::expression(target, None, scope)?;
    if value.ty.pointee().is_some() {
        value = super::arena::deref(value, target.location)?;
    }
    if mutable {
        fn indirect(value: &Value) -> bool {
            match value {
                Value::Deref(_) => true,
                Value::Field { target, .. } => indirect(&target.value),
                _ => false,
            }
        }
        if !indirect(&value.value) {
            let binding = path(target).and_then(|(name, _)| scope.locals.get(name));
            if !binding.is_some_and(|binding| binding.mutable) {
                return Err(target.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "method.write-target",
                        "a # method requires a writable variable, field or pointee"
                    ),
                ));
            }
            binding.unwrap().assigned.set(true);
        }
    }
    Ok(CheckedExpression {
        ty: value.ty,
        value: Value::Receiver {
            target: Box::new(value),
            mutable,
        },
    })
}
