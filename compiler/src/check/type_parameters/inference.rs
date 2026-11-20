//! 実引数から名前付き型引数を決める。確定型を先に集め、リテラルの既定型は後で使う。
use super::super::{
    expression,
    scope::{Scope, Signature},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
};

#[derive(Clone, Copy)]
struct Candidate {
    ty: Type,
    location: Location,
}

pub(super) fn resolve(
    signature: &Signature,
    arguments: &[Expression],
    scope: &Scope,
    location: Location,
    defer_checks: bool,
) -> Result<Vec<Type>, Diagnostic> {
    let mut candidates = vec![None; signature.type_parameters];
    let known: Vec<_> = arguments.iter().map(|arg| known_type(arg, scope)).collect();
    for ((argument, parameter), actual) in arguments.iter().zip(&signature.parameters).zip(&known) {
        if let Some(actual) = actual {
            constrain(
                *parameter,
                *actual,
                argument.location,
                scope,
                &mut candidates,
                &[],
                defer_checks,
            )?;
        }
    }
    // この集合は既定型を選ぶ前に固定する。先に置かれたリテラルの既定型を、
    // 後ろのリテラルへの要求型として使うと、引数順で結果が変わってしまう。
    let fixed: Vec<_> = candidates.iter().map(Option::is_some).collect();
    if fixed.iter().any(|fixed| !fixed) {
        for ((argument, parameter), known) in arguments.iter().zip(&signature.parameters).zip(known)
        {
            if known.is_none()
                && let Some(actual) = defaulted(argument, scope)
            {
                constrain(
                    *parameter,
                    actual,
                    argument.location,
                    scope,
                    &mut candidates,
                    &fixed,
                    defer_checks,
                )?;
            }
        }
    }

    candidates
        .into_iter()
        .enumerate()
        .map(|(id, candidate)| {
            candidate.map(|c| c.ty).ok_or_else(|| {
                location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.inference",
                        "cannot infer type argument {}; specify all type arguments explicitly",
                        id + 1
                    ),
                )
            })
        })
        .collect()
}

// 仮引数の型構造だけを分解する。固定部分の変換可否は、型引数の決定後に
// 通常の引数検査で確認し、構造体の型引数には暗黙変換を適用しない。
fn constrain(
    parameter: Type,
    actual: Type,
    location: Location,
    scope: &Scope,
    candidates: &mut [Option<Candidate>],
    fixed: &[bool],
    defer_checks: bool,
) -> Result<(), Diagnostic> {
    if let Type::Parameter(id) = parameter {
        if actual.leaf() == Type::Void {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.type-arguments",
                    "type arguments must be complete value types"
                ),
            ));
        }
        if fixed.get(id) == Some(&true) {
            return Ok(());
        }
        if let Some(previous) = candidates[id] {
            if previous.ty != actual {
                let conflict = |a, b| {
                    let mut error = location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "generic.inference",
                            "conflicting types {} and {} for type argument {}",
                            scope.type_name(a),
                            scope.type_name(b),
                            id + 1
                        ),
                    );
                    error.related.push(previous.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "generic.inference",
                            "this argument requires {}",
                            scope.type_name(a)
                        ),
                    ));
                    error
                };
                // 既存のint・floatの定義検査では、一部の具体型だけが矛盾することがある。
                // 通常の数値汎用検査と同じく、その組み合わせを使用時まで保留する。
                let domain =
                    super::super::generic::domain::judge(previous.ty, actual, |a, b| a == b);
                if !domain.possible() {
                    return Err(conflict(previous.ty, actual));
                }
                if defer_checks {
                    scope.defer_rule(|i| {
                        let a = super::super::generic::domain::at(previous.ty, i)?;
                        let b = super::super::generic::domain::at(actual, i)?;
                        (a != b).then(|| conflict(a, b))
                    });
                }
            }
        } else {
            candidates[id] = Some(Candidate {
                ty: actual,
                location,
            });
        }
    } else if let (Some(parameter), Some(actual)) = (parameter.pointee(), actual.pointee()) {
        constrain(
            parameter,
            actual,
            location,
            scope,
            candidates,
            fixed,
            defer_checks,
        )?;
    } else if let (Type::Struct(parameter), Type::Struct(actual)) = (parameter, actual) {
        let parameter = scope.structs.get(parameter);
        let actual = scope.structs.get(actual);
        if let (Some(parameter), Some(actual)) = (&parameter.application, &actual.application)
            && parameter.declaration == actual.declaration
        {
            for (&parameter, &actual) in parameter.arguments.iter().zip(&actual.arguments) {
                constrain(
                    parameter,
                    actual,
                    location,
                    scope,
                    candidates,
                    fixed,
                    defer_checks,
                )?;
            }
        }
    }
    Ok(())
}

/// 型ヒントのない式にも、既存の数値の既定型を適用する。nullには型を作らない。
fn defaulted(expr: &Expression, scope: &Scope) -> Option<Type> {
    if let Some(ty) = known_type(expr, scope) {
        return Some(ty);
    }
    match &expr.kind {
        ExpressionKind::Integer { .. } => Some(Type::Integer(IntegerType::I64)),
        ExpressionKind::Float(_) => Some(Type::Float(FloatType::F64)),
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::ArithmeticHandler { operand: inner, .. } => defaulted(inner, scope),
        ExpressionKind::New(inner) => defaulted(inner, scope).map(Type::pointer),
        ExpressionKind::Binary {
            left,
            right,
            operator,
        } => {
            let a = defaulted(left, scope)?;
            let b = defaulted(right, scope)?;
            if operator.is_shift() {
                Some(a)
            } else if a.is_float() || b.is_float() {
                Some(Type::Float(FloatType::F64))
            } else {
                expression::joined_type([Some(a), Some(b)].into_iter())
            }
        }
        ExpressionKind::Block { statements, value } => {
            defaulted(value, &expression::blocks::local_hints(statements, scope))
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => join_defaults(
            branches
                .iter()
                .map(|(_, value)| defaulted(value, scope))
                .chain([defaulted(otherwise, scope)]),
        ),
        ExpressionKind::Match { arms, .. } => {
            join_defaults(arms.iter().map(|arm| defaulted(&arm.body, scope)))
        }
        _ => None,
    }
}

// クロージャの引数と戻り値は宣言に明示されている。本体を検査し直さずに形を使い、
// キャプチャーと呼び出し中限定の制約は、決まった仮引数への通常の検査に任せる。
fn known_type(expr: &Expression, scope: &Scope) -> Option<Type> {
    if let ExpressionKind::Closure(closure) = &expr.kind {
        let function = &closure.function;
        let ty = scope.intern_function(crate::ast::FunctionType {
            parameters: function
                .parameters
                .iter()
                .map(|p| scope.resolve_type_at(p.ty, p.location).ok())
                .collect::<Option<_>>()?,
            result: scope
                .resolve_type_at(function.result, function.location)
                .ok()?,
            errors: function.errors.clone(),
        });
        let Type::StaticFunction(id) = ty else {
            unreachable!()
        };
        return Some(Type::Function(id));
    }
    if let ExpressionKind::Group(inner) = &expr.kind {
        return known_type(inner, scope);
    }
    expression::operand_type(expr, scope)
}

fn join_defaults(types: impl Iterator<Item = Option<Type>>) -> Option<Type> {
    let types: Vec<_> = types.flatten().collect();
    if types.iter().any(|ty| ty.is_float())
        && types.iter().all(|ty| ty.is_float() || ty.is_integer())
    {
        Some(Type::Float(FloatType::F64))
    } else {
        expression::joined_type(types.into_iter().map(Some))
    }
}
