//! 演算子の右辺から定義を選び、通常の関数呼び出しへ接続する。
//! 種類の判定は型付けに先行し、リテラルを既定の具体型へ確定させない。
use super::{expression, functions, ir::CheckedExpression, scope::Scope};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

pub(super) fn owner(expr: &Expression, scope: &Scope) -> Option<Type> {
    expression::operand_type(expr, scope).filter(|ty| matches!(ty, Type::Struct(_) | Type::Enum(_)))
}

fn select<'a>(
    op: &str,
    left: &Expression,
    right: &Expression,
    scope: &'a Scope,
) -> Option<&'a str> {
    let owner = owner(left, scope)?;
    let candidates: Vec<_> = scope
        .functions
        .iter()
        .filter(|(_, f)| {
            f.method
                .is_some_and(|m| m.owner == owner && m.operator == Some(op))
        })
        .collect();
    let known = expression::operand_type(right, scope);
    // C固有整数型やポインタには、数値の種類による選択や暗黙変換を使わない。
    let numeric = known.is_none_or(|ty| ty.is_numeric());
    if numeric {
        let float = expression::has_float_source(right, scope) || known.is_some_and(Type::is_float);
        let wanted = candidates.iter().find(|(_, f)| {
            if float {
                f.parameters[1].is_float()
            } else {
                f.parameters[1].is_integer()
            }
        });
        let fallback = candidates.iter().find(|(_, f)| {
            if float {
                f.parameters[1].is_integer()
            } else {
                f.parameters[1].is_float()
            }
        });
        wanted.or(fallback).map(|(name, _)| name.as_str())
    } else {
        candidates
            .iter()
            .find(|(_, f)| Some(f.parameters[1]) == known)
            .map(|(name, _)| name.as_str())
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
                f.method
                    .is_some_and(|m| Some(m.owner) == owner && m.operator == Some(op))
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
    matches!(op, Add | Subtract | Multiply | Divide)
}

pub(super) fn binary(expr: &Expression, scope: &Scope) -> bool {
    matches!(&expr.kind, ExpressionKind::Binary { operator, left, .. } if supported(*operator) && owner(left, scope).is_some())
}

pub(super) fn hint(op: &str, left: &Expression, right: &Expression, scope: &Scope) -> Option<Type> {
    let name = select(op, left, right, scope)?;
    // 複製したASTのアドレスを元のヒントキャッシュへ残さない。
    let mut hints = scope.clone();
    hints.type_hints = Default::default();
    hints.float_hints = Default::default();
    super::generic::call_operand_type(name, &[left.clone(), right.clone()], &hints)
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
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    let result = functions::call(
        expr,
        name,
        &[left.clone(), right.clone()],
        expected,
        scope,
        None,
    );
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    result
}

/// 比較連鎖は各値を一度だけ束縛し、成立した場合だけ次の比較へ進む。
/// 通常の値ブロック・分岐へ下げるので、呼び出しや寿命の検査を別に持たない。
pub(super) fn comparison(
    expr: &Expression,
    scope: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    use crate::ast::Statement;
    let ExpressionKind::Comparison {
        operators,
        operands,
        locations,
    } = &expr.kind
    else {
        return Ok(None);
    };
    if !operators
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
    let make = |kind| Expression {
        kind,
        location: expr.location,
        height: expr.height,
    };
    let names: Vec<_> = (0..operands.len())
        .map(|i| {
            format!(
                "$comparison_{}_{}_{}",
                expr.location.line, expr.location.column, i
            )
        })
        .collect();
    let reference = |i: usize| make(ExpressionKind::Name(names[i].clone()));
    let binding = |i: usize, annotation: Option<Type>| Statement::Binding {
        pointer: annotation
            .or_else(|| expression::operand_type(&operands[i], scope))
            .is_some_and(|ty| ty.pointee().is_some()),
        mutable: false,
        name: names[i].clone(),
        location: operands[i].location,
        annotation,
        initializer: Box::new(operands[i].clone()),
    };
    let mut tail = make(ExpressionKind::Bool(true));
    for i in (0..operators.len()).rev() {
        let op = operators[i];
        let annotation = if !op.is_equality() && owner(&operands[i], scope).is_some() {
            let name = resolve(op.symbol(), &operands[i], &operands[i + 1], scope)?;
            let signature = &scope.functions[name];
            let types = super::generic::resolve(
                signature,
                &operands[i..=i + 1],
                None,
                scope,
                locations[i],
            )?;
            Some(types.resolve(signature.parameters[1]))
        } else {
            None
        };
        let condition = make(ExpressionKind::Comparison {
            operators: vec![op],
            operands: vec![reference(i), reference(i + 1)],
            locations: vec![locations[i]],
        });
        let value = if i == operators.len() - 1 {
            condition
        } else {
            make(ExpressionKind::If {
                branches: vec![(condition, tail)],
                otherwise: Box::new(make(ExpressionKind::Bool(false))),
            })
        };
        tail = make(ExpressionKind::Block {
            statements: vec![binding(i + 1, annotation)],
            value: Box::new(value),
        });
    }
    let lowered = make(ExpressionKind::Block {
        statements: vec![binding(0, None)],
        value: Box::new(tail),
    });
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    let result = expression::expression(&lowered, Some(Type::Bool), scope);
    scope.type_hints.borrow_mut().clear();
    scope.float_hints.borrow_mut().clear();
    result.map(Some)
}
