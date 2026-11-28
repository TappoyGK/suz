//! 条件式の評価順記録・脱出判定・`or` 代替値合流・`match` / 剰余 / 通過済み読み出しの条件抽出を扱う。
//!
//! 利用元: [`super`]、[`crate::check::statements`]、[`crate::check::expression`]、[`crate::check::arrays`]。

use super::{
    super::{
        super::{
            exact::{Exact, ExactKind},
            ir::{CheckedExpression, CheckedStatement, IntegerOperation, Value},
            scope::Scope,
        },
        facts::{Affine, Facts, Origins, PredExpr, Term},
    },
    affine, constant, dim_to_affine, exact_constant, is_nonneg_int, target_dim_affine,
    writes::forget_expr_writes_into,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        BinaryOperator, ComparisonOperator, Expression, ExpressionKind, Location, LogicalOperator,
        Type,
    },
};

pub(in crate::check) fn record_match_arm(
    target: &CheckedExpression,
    pattern: &CheckedExpression,
    location: Location,
    scope: &Scope,
) {
    let Some(t) = affine(target, scope) else {
        return;
    };
    let Some(c) = constant(pattern) else {
        return;
    };
    scope.array_bounds.facts.borrow_mut().add_comparison(
        &t,
        ComparisonOperator::Equal,
        &Affine::constant(c),
        false,
        location,
    );
}

/// `Exact` 式が `0 <= expr` を満たすかを判定し、根拠位置を返す。
fn proves_exact_nonneg(exact: &Exact, scope: &Scope) -> Option<Vec<Location>> {
    if let Some(n) = exact_constant(exact) {
        return (n >= 0).then(Vec::new);
    }
    if let ExactKind::Leaf(expr) = &exact.kind {
        if is_nonneg_int(expr.ty) {
            return Some(Vec::new());
        }
        let a = affine(expr, scope)?;
        return scope
            .array_bounds
            .facts
            .borrow()
            .proves_le(&Affine::constant(0), &a);
    }
    let wrapped = CheckedExpression {
        ty: Type::Integer(crate::ast::IntegerType::I64),
        value: Value::Exact(Box::new(exact.clone())),
    };
    let a = affine(&wrapped, scope)?;
    scope
        .array_bounds
        .facts
        .borrow()
        .proves_le(&Affine::constant(0), &a)
}

/// `Exact` 式が配列の `len a` または寸法名であれば、対応する `Affine` を返す。
fn exact_dim_expr(exact: &Exact, scope: &Scope) -> Option<Affine> {
    let ExactKind::Leaf(expr) = &exact.kind else {
        return None;
    };
    dim_expr_affine(expr, scope)
}

fn dim_expr_affine(expr: &CheckedExpression, scope: &Scope) -> Option<Affine> {
    match &expr.value {
        Value::Local(id) => scope.array_bounds.names.get(id).copied().map(dim_to_affine),
        Value::Len(target) => target_dim_affine(target, 0, scope),
        Value::ArrayLength { target, dimension } => target_dim_affine(target, *dimension, scope),
        Value::Cast(inner) if expr.ty.is_integer() && inner.ty.is_integer() => {
            dim_expr_affine(inner, scope)
        }
        Value::Exact(e) => exact_dim_expr(e, scope),
        _ => None,
    }
}

/// `x % m`（`or` ハンドラなし）で `0 <= x` が導け、かつ `m` が `len a` または寸法名なら `(m, origins)` を返す。
pub(super) fn remainder_dim_proof(
    expr: &CheckedExpression,
    scope: &Scope,
) -> Option<(Affine, Vec<Location>)> {
    let exact = match &expr.value {
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => e.as_ref(),
        Value::Exact(e) => e.as_ref(),
        Value::Cast(inner) if expr.ty.is_integer() && inner.ty.is_integer() => {
            return remainder_dim_proof(inner, scope);
        }
        _ => return None,
    };
    let ExactKind::Binary(BinaryOperator::Remainder, left, right) = &exact.kind else {
        return None;
    };
    let dim = exact_dim_expr(right, scope)?;
    let origins = proves_exact_nonneg(left, scope)?;
    Some((dim, origins))
}

/// `fix idx = x % len a` のように剰余を束縛したとき、`0 <= idx < len a` を覚える。
pub(in crate::check) fn record_binding_remainder(
    id: usize,
    initializer: &CheckedExpression,
    location: Location,
    scope: &Scope,
) {
    let Some((dim, mut origins)) = remainder_dim_proof(initializer, scope) else {
        return;
    };
    if !origins.contains(&location) {
        origins.push(location);
    }
    let idx = Affine::term(Term::Local(id));
    let origins = Origins::from_vec(origins);
    let mut facts = scope.array_bounds.facts.borrow_mut();
    facts.add_diff(Term::Zero, Term::Local(id), 0, origins.clone());
    if let Some(bound) = dim.offset.checked_sub(1) {
        facts.add_diff(idx.term, dim.term, bound, origins);
    }
}

/// 式が条件集合を一切変化させない単純なリテラル・変数参照であるかを返す。
pub(in crate::check) fn is_facts_pure(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Nil
        | ExpressionKind::Void
        | ExpressionKind::Name(_)
        | ExpressionKind::Variant { .. } => true,
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner) => is_facts_pure(inner),
        _ => false,
    }
}

/// 条件付きで評価される代替値（`or 式`・`or return`・`or error`）を検査する。
/// 脱出するハンドラでは事前状態を復元し、値を返すハンドラでは成功経路と失敗経路を合流する。
pub(in crate::check) fn with_fallback<T>(
    scope: &Scope,
    exits: bool,
    f: impl FnOnce() -> Result<T, Diagnostic>,
) -> Result<T, Diagnostic> {
    let before = scope.array_bounds.facts.borrow().clone();
    let result = f();
    if result.is_ok() {
        if exits {
            *scope.array_bounds.facts.borrow_mut() = before;
        } else {
            let after = scope.array_bounds.facts.borrow().clone();
            *scope.array_bounds.facts.borrow_mut() = Facts::join_all(&[before, after]);
        }
    } else {
        *scope.array_bounds.facts.borrow_mut() = before;
    }
    result
}

/// 範囲外で抜ける（`or return` / `or panic`）配列読み出し・切り出しの後ろで、通過した範囲条件を覚える。
pub(in crate::check) fn record_checked_access(expr: &CheckedExpression, scope: &Scope) {
    match &expr.value {
        Value::ArrayRead { address, .. } => {
            if let Value::ArrayElement { target, indices } = &address.value {
                record_checked_access(target, scope);
                for (axis, (idx_expr, loc, _)) in indices.iter().enumerate() {
                    if let (Some(dim), Some(idx)) = (
                        target_dim_affine(target, axis, scope),
                        affine(idx_expr, scope),
                    ) {
                        let mut facts = scope.array_bounds.facts.borrow_mut();
                        facts.add_comparison(
                            &Affine::constant(0),
                            ComparisonOperator::LessEqual,
                            &idx,
                            false,
                            *loc,
                        );
                        facts.add_comparison(&idx, ComparisonOperator::Less, &dim, false, *loc);
                    }
                }
            }
        }
        Value::ArrayView {
            target,
            start,
            end,
            index,
            location,
            ..
        } => {
            record_checked_access(target, scope);
            if let Some(dim) = target_dim_affine(target, 0, scope) {
                if *index {
                    if let Some(idx_expr) = start.as_deref()
                        && let Some(idx) = affine(idx_expr, scope)
                    {
                        let mut facts = scope.array_bounds.facts.borrow_mut();
                        facts.add_comparison(
                            &Affine::constant(0),
                            ComparisonOperator::LessEqual,
                            &idx,
                            false,
                            *location,
                        );
                        facts.add_comparison(
                            &idx,
                            ComparisonOperator::Less,
                            &dim,
                            false,
                            *location,
                        );
                    }
                } else {
                    let s = match start.as_deref() {
                        None => Some(Affine::constant(0)),
                        Some(e) => affine(e, scope),
                    };
                    let e = match end.as_deref() {
                        None => Some(dim.clone()),
                        Some(ex) => affine(ex, scope),
                    };
                    if let (Some(s), Some(e)) = (s, e) {
                        let mut facts = scope.array_bounds.facts.borrow_mut();
                        facts.add_comparison(
                            &Affine::constant(0),
                            ComparisonOperator::LessEqual,
                            &s,
                            false,
                            *location,
                        );
                        facts.add_comparison(
                            &s,
                            ComparisonOperator::LessEqual,
                            &e,
                            false,
                            *location,
                        );
                        facts.add_comparison(
                            &e,
                            ComparisonOperator::LessEqual,
                            &dim,
                            false,
                            *location,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

fn pred_expr_from_exact(exact: &Exact) -> Option<PredExpr> {
    match &exact.kind {
        ExactKind::Literal(n) => Some(PredExpr::Int(n.to_i128()?)),
        ExactKind::Leaf(value) => pred_expr_from_checked(value),
        ExactKind::Negate(inner) => Some(PredExpr::Neg(Box::new(pred_expr_from_exact(inner)?))),
        ExactKind::Binary(op, left, right) => Some(PredExpr::Binary(
            *op,
            Box::new(pred_expr_from_exact(left)?),
            Box::new(pred_expr_from_exact(right)?),
        )),
    }
}

/// 検査済みの純粋な式から、条件一致判定用の正規化済み `PredExpr` を構築する。
pub(in crate::check) fn pred_expr_from_checked(expr: &CheckedExpression) -> Option<PredExpr> {
    match &expr.value {
        Value::Bool(b) => Some(PredExpr::Bool(*b)),
        Value::Integer { value, .. } => Some(PredExpr::Int(*value)),
        Value::Float(bits) => Some(PredExpr::FloatBits(*bits)),
        Value::Text { text } => Some(PredExpr::Symbol(text.clone())),
        Value::Nil => Some(PredExpr::Nil),
        Value::Local(id) => Some(PredExpr::Local(*id)),
        Value::Field { target, index } => Some(PredExpr::Field(
            Box::new(pred_expr_from_checked(target)?),
            *index,
        )),
        Value::SoaColumn {
            target,
            field_index,
            ..
        } => Some(PredExpr::Field(
            Box::new(pred_expr_from_checked(target)?),
            *field_index,
        )),
        Value::Deref(target) => Some(PredExpr::Deref(Box::new(pred_expr_from_checked(target)?))),
        Value::Receiver { target, .. } => pred_expr_from_checked(target),
        Value::Len(target) => Some(PredExpr::Len(Box::new(pred_expr_from_checked(target)?), 0)),
        Value::ArrayLength { target, dimension } => Some(PredExpr::Len(
            Box::new(pred_expr_from_checked(target)?),
            *dimension,
        )),
        Value::ArrayRead { address, .. } => {
            let Value::ArrayElement { target, indices } = &address.value else {
                return None;
            };
            let t = Box::new(pred_expr_from_checked(target)?);
            let mut out = Vec::with_capacity(indices.len());
            for (idx, _, _) in indices {
                out.push(pred_expr_from_checked(idx)?);
            }
            Some(PredExpr::Index(t, out))
        }
        Value::Exact(e) => pred_expr_from_exact(e),
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => pred_expr_from_exact(e),
        Value::Negate(inner) => Some(PredExpr::Neg(Box::new(pred_expr_from_checked(inner)?))),
        Value::Not(inner) => Some(pred_expr_from_checked(inner)?.negate()),
        Value::Binary {
            operator,
            left,
            right,
        } => Some(PredExpr::Binary(
            *operator,
            Box::new(pred_expr_from_checked(left)?),
            Box::new(pred_expr_from_checked(right)?),
        )),
        Value::Comparison {
            operators,
            operands,
            ..
        } if !operators.is_empty() => {
            let mut acc: Option<PredExpr> = None;
            for k in 0..operators.len() {
                let invertible =
                    is_integer_operand(operands[k].ty) && is_integer_operand(operands[k + 1].ty);
                let pair = PredExpr::compare(
                    operators[k],
                    pred_expr_from_checked(&operands[k])?,
                    pred_expr_from_checked(&operands[k + 1])?,
                    invertible,
                );
                acc = Some(match acc {
                    Some(prev) => PredExpr::And(Box::new(prev), Box::new(pair)),
                    None => pair,
                });
            }
            acc
        }
        Value::Logical {
            operator: LogicalOperator::And,
            left,
            right,
        } => Some(PredExpr::And(
            Box::new(pred_expr_from_checked(left)?),
            Box::new(pred_expr_from_checked(right)?),
        )),
        Value::Logical {
            operator: LogicalOperator::Or,
            left,
            right,
        } => Some(PredExpr::Or(
            Box::new(pred_expr_from_checked(left)?),
            Box::new(pred_expr_from_checked(right)?),
        )),
        Value::Cast(inner) => {
            let inner_pred = pred_expr_from_checked(inner)?;
            if (expr.ty.is_integer() && inner.ty.is_integer())
                || (expr.ty.pointee().is_some() && inner.ty.pointee().is_some())
                || (expr.ty.array_id().is_some() && inner.ty.array_id().is_some())
            {
                Some(inner_pred)
            } else {
                Some(PredExpr::Cast(Box::new(inner_pred), expr.ty))
            }
        }
        _ => None,
    }
}

fn is_integer_operand(ty: Type) -> bool {
    ty.is_integer() || ty == Type::Exact
}

#[inline(never)]
fn record_comparison_predicates(
    operators: &[ComparisonOperator],
    locations: &[Location],
    checked_operands: &[CheckedExpression],
    ast_operands: &[Expression],
    affines: &[Option<Affine>],
    scope: &Scope,
    (true_facts, false_facts): (&mut Facts, &mut Facts),
) {
    // すでに差分制約（Affine）で完全に表現済みの比較（!= 以外）は PredExpr へ重複登録しない。
    let mut need_any = false;
    for k in 0..operators.len() {
        let affine_pair = affines[k].is_some() && affines[k + 1].is_some();
        if !affine_pair || operators[k] == ComparisonOperator::NotEqual {
            need_any = true;
            break;
        }
    }
    let need_false = (operators.len() == 1
        && (!(affines[0].is_some() && affines[1].is_some())
            || operators[0] == ComparisonOperator::Equal))
        || operators.len() > 1;
    if !need_any && !need_false {
        return;
    }
    let preds: Vec<Option<PredExpr>> = checked_operands
        .iter()
        .map(pred_expr_from_checked)
        .collect();
    if need_any {
        for k in 0..operators.len() {
            let affine_pair = affines[k].is_some() && affines[k + 1].is_some();
            if (!affine_pair || operators[k] == ComparisonOperator::NotEqual)
                && let (Some(l), Some(r)) = (&preds[k], &preds[k + 1])
            {
                let invertible = is_integer_operand(checked_operands[k].ty)
                    && is_integer_operand(checked_operands[k + 1].ty);
                true_facts.add_predicate(
                    PredExpr::compare(operators[k], l.clone(), r.clone(), invertible),
                    locations[k],
                );
            }
            for later in &ast_operands[k + 1..] {
                forget_expr_writes_into(later, scope, true_facts);
            }
        }
    }
    if need_false {
        if operators.len() == 1
            && let (Some(l), Some(r)) = (&preds[0], &preds[1])
        {
            let invertible = is_integer_operand(checked_operands[0].ty)
                && is_integer_operand(checked_operands[1].ty);
            false_facts.add_predicate(
                PredExpr::compare(operators[0], l.clone(), r.clone(), invertible).negate(),
                locations[0],
            );
            forget_expr_writes_into(&ast_operands[1], scope, false_facts);
        } else if operators.len() > 1 && preds.iter().all(Option::is_some) {
            let mut chain: Option<PredExpr> = None;
            for k in 0..operators.len() {
                let l = preds[k].as_ref().unwrap().clone();
                let r = preds[k + 1].as_ref().unwrap().clone();
                let invertible = is_integer_operand(checked_operands[k].ty)
                    && is_integer_operand(checked_operands[k + 1].ty);
                let pair = PredExpr::compare(operators[k], l, r, invertible);
                chain = Some(match chain {
                    Some(prev) => PredExpr::And(Box::new(prev), Box::new(pair)),
                    None => pair,
                });
            }
            if let Some(chain) = chain {
                false_facts.add_predicate(chain.negate(), locations[0]);
            }
        }
    }
}

#[inline(never)]
fn record_bool_predicate(
    checked: &CheckedExpression,
    location: Location,
    true_facts: &mut Facts,
    false_facts: &mut Facts,
) {
    if checked.ty == Type::Bool
        && let Some(pred) = pred_expr_from_checked(checked)
    {
        true_facts.add_predicate(pred.clone(), location);
        false_facts.add_predicate(pred.negate(), location);
    }
}

/// 論理式・比較式を評価順（短絡評価・比較連鎖を含む）で検査し、
/// 真の側・偽の側で成り立つ `Facts` をそれぞれ返す。
pub(in crate::check) fn eval_condition(
    expr: &Expression,
    scope: &Scope,
) -> Result<(CheckedExpression, Facts, Facts), Diagnostic> {
    match &expr.kind {
        ExpressionKind::Group(inner) => eval_condition(inner, scope),
        ExpressionKind::Not(inner) => {
            // ユーザー定義の not 演算子メソッドの場合は通常の式として扱う。
            if scope.has_operator_methods
                && super::super::super::operator_methods::unary_name("not", inner, scope)
                    .is_some_and(|name| scope.functions.contains_key(&name))
            {
                let checked = super::super::super::expression::expression(expr, None, scope)?;
                let after = scope.array_bounds.facts.borrow().clone();
                return Ok((checked, after.clone(), after));
            }
            let (operand, t, f) = eval_condition(inner, scope)?;
            if operand.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    if matches!(operand.ty, Type::Struct(_) | Type::Enum(_)) {
                        crate::diagnostic!(
                            "operator.unary",
                            "no unary operator not defined for this type"
                        )
                    } else {
                        crate::diagnostic!("expr.not", "not requires a bool operand")
                    },
                ));
            }
            Ok((
                CheckedExpression {
                    ty: Type::Bool,
                    value: Value::Not(Box::new(operand)),
                },
                f,
                t,
            ))
        }
        ExpressionKind::Logical {
            operator,
            left,
            right,
        } => {
            let (checked_left, left_t, left_f) = eval_condition(left, scope)?;
            if checked_left.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.operator-types",
                        "{} requires bool operands",
                        operator.symbol()
                    ),
                ));
            }
            let saved = scope.array_bounds.facts.borrow().clone();
            let (checked_right, right_t, right_f) = match operator {
                LogicalOperator::And => {
                    *scope.array_bounds.facts.borrow_mut() = left_t.clone();
                    eval_condition(right, scope)?
                }
                LogicalOperator::Or => {
                    *scope.array_bounds.facts.borrow_mut() = left_f.clone();
                    eval_condition(right, scope)?
                }
            };
            *scope.array_bounds.facts.borrow_mut() = saved;
            if checked_right.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.operator-types",
                        "{} requires bool operands",
                        operator.symbol()
                    ),
                ));
            }
            let (mut true_facts, mut false_facts) = match operator {
                LogicalOperator::And => (right_t, Facts::join_all(&[left_f, right_f])),
                LogicalOperator::Or => (Facts::join_all(&[left_t, right_t]), right_f),
            };
            if let (Some(lp), Some(rp)) = (
                pred_expr_from_checked(&checked_left),
                pred_expr_from_checked(&checked_right),
            ) {
                match operator {
                    LogicalOperator::And => {
                        false_facts.add_predicate(
                            PredExpr::And(Box::new(lp), Box::new(rp)).negate(),
                            expr.location,
                        );
                    }
                    LogicalOperator::Or => {
                        true_facts
                            .add_predicate(PredExpr::Or(Box::new(lp), Box::new(rp)), expr.location);
                    }
                }
            }
            Ok((
                CheckedExpression {
                    ty: Type::Bool,
                    value: Value::Logical {
                        operator: *operator,
                        left: Box::new(checked_left),
                        right: Box::new(checked_right),
                    },
                },
                true_facts,
                false_facts,
            ))
        }
        ExpressionKind::Comparison {
            locations,
            operators,
            operands: ast_operands,
        } => {
            let checked = super::super::super::expression::expression(expr, None, scope)?;
            let after = scope.array_bounds.facts.borrow().clone();
            let mut true_facts = after.clone();
            let mut false_facts = after;
            if let Value::Comparison {
                operands: checked_operands,
                ..
            } = &checked.value
            {
                let affines: Vec<Option<Affine>> =
                    checked_operands.iter().map(|o| affine(o, scope)).collect();
                for k in 0..operators.len() {
                    if let (Some(l), Some(r)) = (&affines[k], &affines[k + 1]) {
                        true_facts.add_comparison(l, operators[k], r, false, locations[k]);
                    }
                    // 比較連鎖の後半オペランドに副作用があれば、直前の比較で覚えた項のうち変わるものを忘れる。
                    for later in &ast_operands[k + 1..] {
                        forget_expr_writes_into(later, scope, &mut true_facts);
                    }
                }
                if operators.len() == 1
                    && let (Some(l), Some(r)) = (&affines[0], &affines[1])
                {
                    false_facts.add_comparison(l, operators[0], r, true, locations[0]);
                    forget_expr_writes_into(&ast_operands[1], scope, &mut false_facts);
                }
                record_comparison_predicates(
                    operators,
                    locations,
                    checked_operands,
                    ast_operands,
                    &affines,
                    scope,
                    (&mut true_facts, &mut false_facts),
                );
            }
            Ok((checked, true_facts, false_facts))
        }
        _ => {
            let checked = super::super::super::expression::expression(expr, None, scope)?;
            let after = scope.array_bounds.facts.borrow().clone();
            let mut true_facts = after.clone();
            let mut false_facts = after;
            record_bool_predicate(&checked, expr.location, &mut true_facts, &mut false_facts);
            Ok((checked, true_facts, false_facts))
        }
    }
}

/// ブロックが必ず return / fail / break / continue / panic で抜けるかを判定する。
pub(in crate::check) fn block_exits(statements: &[CheckedStatement]) -> bool {
    statements.iter().any(statement_exits)
}

fn statement_exits(stmt: &CheckedStatement) -> bool {
    match stmt {
        CheckedStatement::Return { .. }
        | CheckedStatement::Fail(_)
        | CheckedStatement::Break
        | CheckedStatement::Continue => true,
        CheckedStatement::Expression(e) => matches!(e.value, Value::Panic { .. }),
        CheckedStatement::Block(body)
        | CheckedStatement::Arena { body, .. }
        | CheckedStatement::Objection(body) => block_exits(body),
        CheckedStatement::If {
            branches,
            otherwise: Some(otherwise),
        } => branches.iter().all(|(_, b)| block_exits(b)) && block_exits(otherwise),
        CheckedStatement::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|(_, b)| block_exits(b))
        }
        _ => false,
    }
}
