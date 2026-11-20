//! 演算子の定義を選び、通常の関数呼び出しへ接続する。
//! 二項演算の右辺では種類の判定が型付けに先行し、リテラルを既定の具体型へ確定させない。
use super::{expression, functions, ir::CheckedExpression, scope::Scope};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

pub(super) fn owner(expr: &Expression, scope: &Scope) -> Option<Type> {
    if !scope.has_operator_methods {
        return None;
    }
    expression::operand_type(expr, scope).filter(|ty| matches!(ty, Type::Struct(_) | Type::Enum(_)))
}

pub(super) fn select<'a>(
    op: &str,
    left: &Expression,
    right: &Expression,
    scope: &'a Scope,
) -> Option<&'a str> {
    let owner = owner(left, scope)?;
    // 宣言時に付けたキーで直接引く。数値の種類だけを選び、具体型の検査は呼び出しに任せる。
    let prefix = format!("{}.{}@", scope.type_name(owner), op);
    let lookup = |key: &str| {
        scope
            .functions
            .get_key_value(&format!("{prefix}{key}"))
            .map(|(name, _)| name.as_str())
    };
    let known = expression::operand_type(right, scope);
    if known.is_none_or(|ty| ty.is_numeric()) {
        let float = expression::has_float_source(right, scope) || known.is_some_and(Type::is_float);
        if float {
            lookup("float")
        } else {
            lookup("integer").or_else(|| lookup("float"))
        }
    } else {
        lookup(&format!("{:?}", known?))
    }
}

fn resolve<'a>(
    op: &str,
    left: &Expression,
    right: &Expression,
    scope: &'a Scope,
) -> Result<&'a str, Diagnostic> {
    select(op, left, right, scope).ok_or_else(|| {
        let owner = owner(left, scope);
        let accepted = scope
            .functions
            .values()
            .filter(|f| {
                f.method.is_some_and(|m| {
                    Some(m.owner) == owner && m.operator == Some(op) && f.parameters.len() == 2
                })
            })
            .map(|f| scope.type_name(f.parameters[1]))
            .collect::<Vec<_>>()
            .join(", ");
        left.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "operator.overload",
                "no matching operator {op}; accepted right-hand types: {}",
                if accepted.is_empty() {
                    "none"
                } else {
                    &accepted
                }
            ),
        )
    })
}

pub(super) fn supported(op: crate::ast::BinaryOperator) -> bool {
    use crate::ast::BinaryOperator::*;
    matches!(
        op,
        Add | Subtract | Multiply | Divide | Remainder | And | Or | Xor | ShiftLeft | ShiftRight
    )
}

pub(super) fn is_call(expr: &Expression, scope: &Scope) -> bool {
    if !scope.has_operator_methods {
        return false;
    }
    match &expr.kind {
        ExpressionKind::Binary { operator, left, .. } if supported(*operator) => {
            owner(left, scope).is_some()
        }
        _ => unary(expr).is_some_and(|(_, operand)| owner(operand, scope).is_some()),
    }
}

pub(super) fn hint(op: &str, left: &Expression, right: &Expression, scope: &Scope) -> Option<Type> {
    let name = select(op, left, right, scope)?;
    let result = scope.functions[name].result;
    if !result.leaf().generic() {
        return Some(result);
    }
    // 複製したASTのアドレスを元のヒントキャッシュへ残さない。
    let mut hints = scope.clone();
    hints.type_hints = Default::default();
    hints.float_hints = Default::default();
    super::generic::call_operand_type(name, &[left.clone(), right.clone()], &hints)
}

/// 値を一度評価して渡す単項演算。型や格納先を指定する構文は含めない。
pub(super) fn unary(expr: &Expression) -> Option<(&'static str, &Expression)> {
    match &expr.kind {
        ExpressionKind::Negate(inner) => Some(("-", inner)),
        ExpressionKind::BitNot(inner) => Some(("~", inner)),
        ExpressionKind::Not(inner) => Some(("not", inner)),
        ExpressionKind::Len(inner) => Some(("len", inner)),
        _ => None,
    }
}

pub(super) fn unary_name(op: &str, operand: &Expression, scope: &Scope) -> Option<String> {
    Some(format!(
        "{}.{op}@unary",
        scope.type_name(owner(operand, scope)?)
    ))
}

pub(super) fn unary_hint(op: &str, operand: &Expression, scope: &Scope) -> Option<Type> {
    let name = unary_name(op, operand, scope)?;
    super::generic::call_operand_type(&name, std::slice::from_ref(operand), scope)
}

pub(super) fn unary_call(
    expr: &Expression,
    op: &str,
    operand: &Expression,
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let name = unary_name(op, operand, scope).expect("operator receiver checked");
    if !scope.functions.contains_key(&name) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "operator.unary",
                "no unary operator {op} defined for this type"
            ),
        ));
    }
    invoke(expr, &name, std::slice::from_ref(operand), expected, scope)
}

pub(super) fn call(
    expr: &Expression,
    op: &str,
    left: &Expression,
    right: &Expression,
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let name = resolve(op, left, right, scope)?;
    invoke(expr, name, &[left.clone(), right.clone()], expected, scope)
}

fn invoke(
    expr: &Expression,
    name: &str,
    arguments: &[Expression],
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if scope.constant_initializer {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "const.expression",
                "operator method calls are not constant expressions"
            ),
        ));
    }
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    let result = functions::call(expr, name, arguments, expected, scope, None);
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    result
}

/// 複合代入は、通常の呼び出しと代入の評価順を使う。
/// 戻り値には代入による変換ではなく、代入先の型との一致を要求する。
pub(super) fn compound(
    expr: &Expression,
    target: Type,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if scope.has_operator_methods
        && matches!(target, Type::Struct(_) | Type::Enum(_))
        && let ExpressionKind::Binary {
            operator,
            left,
            right,
        } = &expr.kind
        && supported(*operator)
    {
        let name = resolve(operator.c_symbol(), left, right, scope)?;
        let result = scope.functions[name].result;
        if result != target {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "operator.compound",
                    "operator {}= requires a result of type {}, found {}",
                    operator.c_symbol(),
                    scope.type_name(target),
                    scope.type_name(result)
                ),
            ));
        }
    }
    expression::coerced(expr, Some(target), scope)
}

/// 中間の値は検査済みの値として次の比較へ渡す。IRにも各オペランドを一つだけ残し、
/// コード生成では通常の比較と同じ一時変数と短絡評価を使う。
pub(super) fn comparison(
    expr: &Expression,
    scope: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    use super::ir::Value;
    let ExpressionKind::Comparison {
        operators,
        operands,
        locations,
    } = &expr.kind
    else {
        return Ok(None);
    };
    if !scope.has_operator_methods
        || !operators
            .iter()
            .zip(operands)
            .any(|(op, left)| !op.is_equality() && owner(left, scope).is_some())
    {
        return Ok(None);
    }
    if operators.len() == 1 {
        return call(
            expr,
            operators[0].symbol(),
            &operands[0],
            &operands[1],
            None,
            scope,
        )
        .map(Some);
    }
    let mut scope = scope.clone();
    scope.type_hints = Default::default();
    scope.float_hints = Default::default();
    let mut checked = Vec::with_capacity(operands.len());
    let mut methods = Vec::with_capacity(operators.len());
    let mut left = operands[0].clone();
    for (i, op) in operators.iter().enumerate() {
        let (method, mut pair) = if !op.is_equality() && owner(&left, &scope).is_some() {
            let value = call(expr, op.symbol(), &left, &operands[i + 1], None, &scope)?;
            let Value::Call { id, arguments, .. } = value.value else {
                unreachable!()
            };
            (Some(id), arguments)
        } else {
            let pair = Expression {
                kind: ExpressionKind::Comparison {
                    operators: vec![*op],
                    operands: vec![left.clone(), operands[i + 1].clone()],
                    locations: vec![locations[i]],
                },
                location: expr.location,
                height: expr.height,
            };
            let value = expression::expression(&pair, Some(Type::Bool), &scope)?;
            let Value::Comparison { operands, .. } = value.value else {
                unreachable!()
            };
            (None, operands)
        };
        let right = pair.pop().unwrap();
        if i == 0 {
            checked.push(pair.pop().unwrap());
        }
        methods.push(method);
        // ソースには書けない名前。次の比較の型検査専用であり、実行時のローカルは作らない。
        let name = "$comparison".to_owned();
        let mut binding = super::statements::Binding::hint(right.ty, operands[i + 1].location);
        binding.constant = Some(right.clone());
        scope.locals.insert(name.clone(), binding);
        left = Expression {
            kind: ExpressionKind::Name(name),
            location: operands[i + 1].location,
            height: 1,
        };
        checked.push(right);
        scope.type_hints.borrow_mut().clear();
        scope.float_hints.borrow_mut().clear();
    }
    Ok(Some(CheckedExpression {
        ty: Type::Bool,
        value: Value::Comparison {
            operators: operators.clone(),
            operands: checked,
            methods,
        },
    }))
}
