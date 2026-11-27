//! 関数の契約（`@requires`・`@ensures`）の式検証・静的証明・事後条件の引き継ぎを扱う。
//!
//! 利用元: [`crate::check::functions`]、[`crate::check::type_parameters`]、[`crate::check::statements`]。
//! 主な対象規則: `[contract.decl]`、`[contract.expr]`、`[contract.requires]`、`[contract.ensures]`、`[contract.fn-value]`。

use super::{
    arrays::{
        bounds::{affine, eval_condition, pred_expr_from_checked},
        facts::{Affine, PredExpr, Term},
    },
    constants::constant_integer,
    exact::{Exact, ExactKind},
    ir::{CheckedExpression, IntegerOperation, Value},
    scope::{Scope, Signature},
    statements::Binding,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        BinaryOperator, ComparisonOperator, Contract, ContractKind, Expression, ExpressionKind,
        Location, LogicalOperator, Type,
    },
    numeric::{Int, Range},
};
use std::collections::BTreeMap;

fn expr_error(location: Location, message: &str) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!("contract.expr", "{message}"),
    )
}

/// 契約式の AST が許可された純粋・組み込み操作のみで構成されているかを検査する。
fn validate_ast(expr: &Expression, allow_return: bool, scope: &Scope) -> Result<(), Diagnostic> {
    match &expr.kind {
        ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Nil
        | ExpressionKind::Variant { .. } => Ok(()),
        ExpressionKind::Template(parts) => {
            if parts
                .iter()
                .all(|p| matches!(p, crate::ast::TemplatePart::Text(_)))
            {
                Ok(())
            } else {
                Err(expr_error(
                    expr.location,
                    "template interpolation is not allowed in contract expressions",
                ))
            }
        }
        ExpressionKind::Name(name) => {
            if name == "return" && !allow_return {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "contract.return",
                        "return can only be referenced in @ensures"
                    ),
                ));
            }
            if name == "return" && scope.result == Type::Void {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "contract.return",
                        "void functions cannot reference return in @ensures"
                    ),
                ));
            }
            Ok(())
        }
        ExpressionKind::Group(inner)
        | ExpressionKind::Field { target: inner, .. }
        | ExpressionKind::Deref(inner)
        | ExpressionKind::Len(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::Not(inner) => validate_ast(inner, allow_return, scope),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            if !matches!(
                operator,
                BinaryOperator::Add
                    | BinaryOperator::Subtract
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                    | BinaryOperator::Remainder
                    | BinaryOperator::EuclideanRemainder
            ) {
                return Err(expr_error(
                    expr.location,
                    "this operator is not allowed in contract expressions",
                ));
            }
            validate_ast(left, allow_return, scope)?;
            validate_ast(right, allow_return, scope)
        }
        ExpressionKind::Logical { left, right, .. } => {
            validate_ast(left, allow_return, scope)?;
            validate_ast(right, allow_return, scope)
        }
        ExpressionKind::Comparison { operands, .. } => {
            for op in operands {
                validate_ast(op, allow_return, scope)?;
            }
            Ok(())
        }
        ExpressionKind::Cast {
            kind,
            operand,
            fallback,
            ..
        } => {
            if *kind != crate::ast::CastKind::Checked || fallback.is_some() {
                return Err(expr_error(
                    expr.location,
                    "only infallible as casts without or are allowed in contract expressions",
                ));
            }
            validate_ast(operand, allow_return, scope)
        }
        ExpressionKind::Slice {
            target,
            start,
            end,
            index,
            fallback,
            ..
        } => {
            if !*index || fallback.is_some() || end.is_some() {
                return Err(expr_error(
                    expr.location,
                    "only statically proven array indexing without or is allowed in contract expressions",
                ));
            }
            validate_ast(target, allow_return, scope)?;
            if let Some(s) = start {
                validate_ast(s, allow_return, scope)?;
            }
            Ok(())
        }
        _ => Err(expr_error(
            expr.location,
            "this expression is not allowed in contract expressions",
        )),
    }
}

fn validate_exact(exact: &Exact, location: Location, scope: &Scope) -> Result<(), Diagnostic> {
    match &exact.kind {
        ExactKind::Literal(_) => Ok(()),
        ExactKind::Leaf(value) => validate_checked(value, location, scope),
        ExactKind::Negate(inner) => validate_exact(inner, location, scope),
        ExactKind::Binary(op, left, right) => {
            validate_exact(left, location, scope)?;
            validate_exact(right, location, scope)?;
            if matches!(
                op,
                BinaryOperator::Divide
                    | BinaryOperator::Remainder
                    | BinaryOperator::EuclideanRemainder
            ) {
                let nonzero = right
                    .range
                    .is_some_and(|r| r.min > Int::from_i128(0) || r.max < Int::from_i128(0))
                    || exact_leaf_nonzero(right, scope);
                if !nonzero {
                    return Err(expr_error(
                        exact.location,
                        "division or remainder in a contract requires a statically non-zero divisor",
                    ));
                }
            }
            Ok(())
        }
    }
}

fn exact_leaf_nonzero(exact: &Exact, scope: &Scope) -> bool {
    let ExactKind::Leaf(value) = &exact.kind else {
        return false;
    };
    let Some(a) = affine(value, scope) else {
        return false;
    };
    scope.array_bounds.facts.borrow().proves_nonzero(&a)
}

/// 検査済み IR が失敗しうる操作やユーザー定義呼び出しを含まないことを確認する。
fn validate_checked(
    expr: &CheckedExpression,
    location: Location,
    scope: &Scope,
) -> Result<(), Diagnostic> {
    match &expr.value {
        Value::Bool(_)
        | Value::Integer { .. }
        | Value::Float(_)
        | Value::Text { .. }
        | Value::Nil
        | Value::Local(_)
        | Value::Void => Ok(()),
        Value::Field { target, .. }
        | Value::Deref(target)
        | Value::Receiver { target, .. }
        | Value::Len(target)
        | Value::ArrayLength { target, .. }
        | Value::Not(target)
        | Value::Cast(target) => validate_checked(target, location, scope),
        Value::Negate(target) => {
            if expr.ty.is_float() {
                return Err(expr_error(
                    location,
                    "floating-point arithmetic is not allowed in contract expressions",
                ));
            }
            validate_checked(target, location, scope)
        }
        Value::Exact(e) => validate_exact(e, location, scope),
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => validate_exact(e, location, scope),
        Value::Binary { left, right, .. } => {
            if expr.ty.is_float() {
                return Err(expr_error(
                    location,
                    "floating-point arithmetic is not allowed in contract expressions",
                ));
            }
            validate_checked(left, location, scope)?;
            validate_checked(right, location, scope)
        }
        Value::Logical {
            operator,
            left,
            right,
        } => {
            validate_checked(left, location, scope)?;
            if matches!(*operator, LogicalOperator::And) {
                let saved = scope.array_bounds.facts.borrow().clone();
                if let Some(pred) = pred_expr_from_checked(left) {
                    scope
                        .array_bounds
                        .facts
                        .borrow_mut()
                        .add_predicate(pred, location);
                }
                let res = validate_checked(right, location, scope);
                *scope.array_bounds.facts.borrow_mut() = saved;
                res
            } else {
                validate_checked(right, location, scope)
            }
        }
        Value::Comparison { operands, .. } => {
            for op in operands {
                validate_checked(op, location, scope)?;
            }
            Ok(())
        }
        Value::ArrayRead { address, .. } => {
            let Value::ArrayElement { target, indices } = &address.value else {
                return Err(expr_error(
                    location,
                    "this array operation is not allowed in contract expressions",
                ));
            };
            validate_checked(target, location, scope)?;
            for (idx, _, _) in indices {
                validate_checked(idx, location, scope)?;
            }
            Ok(())
        }
        _ => Err(expr_error(
            location,
            "calls, operator methods, and fallible operations are not allowed in contract expressions",
        )),
    }
}

/// 関数定義の入口で全契約式を検証し、`@requires` の条件を本体スコープへ記憶する。
pub(super) fn validate_and_assume(
    contracts: &[Contract],
    scope: &mut Scope,
) -> Result<(), Diagnostic> {
    if contracts.is_empty() {
        return Ok(());
    }
    // 1. @requires を記述順に検証し、成立した条件を後続と本体スコープへ記憶する。
    for contract in contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Requires)
    {
        validate_ast(&contract.condition, false, scope)?;
        let (checked, true_facts, _) = eval_condition(&contract.condition, scope)?;
        if checked.ty != Type::Bool {
            return Err(expr_error(
                contract.location,
                "contract condition must have type bool",
            ));
        }
        validate_checked(&checked, contract.location, scope)?;
        *scope.array_bounds.facts.borrow_mut() = true_facts;
    }

    // 2. @ensures を @requires 成立済みスコープ（+ return 束縛）で検証する。
    let mut ensures_scope = scope.clone();
    if scope.result != Type::Void {
        let return_id = usize::MAX - 1;
        ensures_scope.locals.insert(
            "return".into(),
            Binding {
                constant: None,
                id: return_id,
                ty: scope.result,
                mutable: false,
                parameter: true,
                loop_variable: false,
                assigned: Default::default(),
                location: contracts[0].location,
            },
        );
        super::arrays::bounds::register(
            return_id,
            scope.result,
            false,
            Vec::new(),
            &mut ensures_scope,
        );
    }
    for contract in contracts.iter().filter(|c| c.kind == ContractKind::Ensures) {
        validate_ast(&contract.condition, true, &ensures_scope)?;
        let (checked, true_facts, _) = eval_condition(&contract.condition, &ensures_scope)?;
        if checked.ty != Type::Bool {
            return Err(expr_error(
                contract.location,
                "contract condition must have type bool",
            ));
        }
        validate_checked(&checked, contract.location, &ensures_scope)?;
        *ensures_scope.array_bounds.facts.borrow_mut() = true_facts;
    }
    Ok(())
}

/// 契約式内の仮引数名・`return` 名を、呼び出し実引数や返却式へ対応づける環境。
struct SubstEnv<'a> {
    bindings: BTreeMap<&'a str, &'a CheckedExpression>,
}

impl<'a> SubstEnv<'a> {
    fn from_call(signature: &'a Signature, arguments: &'a [CheckedExpression]) -> Self {
        let mut bindings = BTreeMap::new();
        if let Some(names) = &signature.parameter_names {
            for (name, arg) in names.iter().zip(arguments) {
                bindings.insert(name.as_str(), arg);
            }
        }
        Self { bindings }
    }

    fn from_return(returned: &'a CheckedExpression) -> Self {
        let mut bindings = BTreeMap::new();
        bindings.insert("return", returned);
        Self { bindings }
    }
}

/// 式から `Affine`（`term + offset`）を抽出する（仮引数・`return` を環境で置換）。
fn subst_affine(expr: &Expression, env: &SubstEnv<'_>, scope: &Scope) -> Option<Affine> {
    match &expr.kind {
        ExpressionKind::Group(inner) => subst_affine(inner, env, scope),
        ExpressionKind::Integer { digits, negative } => {
            let v: i128 = digits.parse().ok()?;
            Some(Affine::constant(if *negative {
                v.checked_neg()?
            } else {
                v
            }))
        }
        ExpressionKind::Name(name) => {
            if let Some(bound) = env.bindings.get(name.as_str()) {
                return affine(bound, scope);
            }
            if let Some(binding) = scope.get(name) {
                if let Some(constant) = &binding.constant
                    && let Ok(Some(n)) = constant_integer(constant)
                {
                    return Some(Affine::constant(n));
                }
                if let Some(&dim) = scope.array_bounds.names.get(&binding.id) {
                    return match dim {
                        super::arrays::bounds::Dimension::Constant(n)
                        | super::arrays::bounds::Dimension::Static(_, _, n) => {
                            Some(Affine::constant(n))
                        }
                        super::arrays::bounds::Dimension::Source(id, axis, _) => {
                            Some(Affine::term(Term::Dim(id, axis)))
                        }
                    };
                }
                if binding.ty.is_integer() {
                    return Some(Affine::term(Term::Local(binding.id)));
                }
            }
            None
        }
        ExpressionKind::Len(inner) => {
            let target = subst_checked_target(inner, env, scope)?;
            let checked = CheckedExpression {
                ty: Type::Integer(crate::ast::IntegerType::Len),
                value: Value::Len(Box::new(target)),
            };
            affine(&checked, scope)
        }
        ExpressionKind::Negate(inner) => {
            let a = subst_affine(inner, env, scope)?;
            if a.term == Term::Zero {
                Some(Affine::constant(a.offset.checked_neg()?))
            } else {
                None
            }
        }
        ExpressionKind::Binary {
            operator: op @ (BinaryOperator::Add | BinaryOperator::Subtract),
            left,
            right,
        } => {
            let l = subst_affine(left, env, scope)?;
            let r = subst_affine(right, env, scope)?;
            match (l.term == Term::Zero, r.term == Term::Zero) {
                (_, true) => {
                    let delta = if *op == BinaryOperator::Subtract {
                        r.offset.checked_neg()?
                    } else {
                        r.offset
                    };
                    l.shifted(delta)
                }
                (true, false) if *op == BinaryOperator::Add => r.shifted(l.offset),
                _ => None,
            }
        }
        ExpressionKind::Field { .. } => {
            let target = subst_checked_target(expr, env, scope)?;
            affine(&target, scope)
        }
        _ => None,
    }
}

/// 配列対象など、`Value::Local` や置換後の `CheckedExpression` を取得する補助。
fn subst_checked_target(
    expr: &Expression,
    env: &SubstEnv<'_>,
    scope: &Scope,
) -> Option<CheckedExpression> {
    match &expr.kind {
        ExpressionKind::Group(inner) => subst_checked_target(inner, env, scope),
        ExpressionKind::Name(name) => {
            if let Some(bound) = env.bindings.get(name.as_str()) {
                return Some((*bound).clone());
            }
            let binding = scope.get(name)?;
            if let Some(c) = &binding.constant {
                return Some(c.clone());
            }
            Some(CheckedExpression {
                ty: binding.ty,
                value: Value::Local(binding.id),
            })
        }
        ExpressionKind::Field { target, name } => {
            let base = subst_checked_target(target, env, scope)?;
            let struct_id = match base.ty.pointee().unwrap_or(base.ty) {
                Type::Struct(id) => id,
                _ => return None,
            };
            let s = scope.structs.get(struct_id);
            let idx = s.fields.iter().position(|f| f.name == *name)?;
            if base.ty.pointee().is_none()
                && let Value::Struct { fields } = &base.value
            {
                return fields
                    .iter()
                    .find(|(i, _)| *i == idx)
                    .map(|(_, e)| e.clone());
            }
            let ty = s.fields[idx].ty;
            let base = if base.ty.pointee().is_some() {
                CheckedExpression {
                    ty: base.ty.pointee().unwrap(),
                    value: Value::Deref(Box::new(base)),
                }
            } else {
                base
            };
            Some(CheckedExpression {
                ty,
                value: Value::Field {
                    target: Box::new(base),
                    index: idx,
                },
            })
        }
        ExpressionKind::Deref(inner) => {
            let base = subst_checked_target(inner, env, scope)?;
            let pointee = base.ty.pointee()?;
            Some(CheckedExpression {
                ty: pointee,
                value: Value::Deref(Box::new(base)),
            })
        }
        _ => None,
    }
}

/// 算術・キャストの葉ノードの型範囲 `[min, max]` を、`Facts` で証明済みの定数上下限で狭める。
/// 定数畳み込み（例：`0 as u8`）は行わず、ローカル束縛・配列寸法の記憶済み条件のみを使う。
pub(in crate::check) fn narrow_leaf_range(
    expr: &CheckedExpression,
    min: i128,
    max: i128,
    scope: &Scope,
) -> Range {
    let mut range = Range::bounds(min, max);
    if scope.array_bounds.facts.borrow().is_empty() {
        return range;
    }
    if let Some(a) = affine(expr, scope)
        && a.term != Term::Zero
    {
        let (lo, hi) = scope.array_bounds.facts.borrow().proven_bounds(&a.term);
        if let Some(l) = lo.and_then(|v| v.checked_add(a.offset)) {
            let l_int = Int::from_i128(l);
            if l_int > range.min {
                range.min = l_int;
            }
        }
        if let Some(h) = hi.and_then(|v| v.checked_add(a.offset)) {
            let h_int = Int::from_i128(h);
            if h_int < range.max {
                range.max = h_int;
            }
        }
    }
    if range.min <= range.max {
        range
    } else {
        Range::bounds(min, max)
    }
}

/// 検査済み整数式の証明済み値域 `(min, max)` を求める（`Exact`・`Facts`・型境界を統合）。
pub(in crate::check) fn checked_int_range(
    expr: &CheckedExpression,
    scope: &Scope,
) -> Option<Range> {
    if let Ok(Some(n)) = constant_integer(expr) {
        return Some(Range::point(Int::from_i128(n)));
    }
    let mut range = match &expr.value {
        Value::Exact(e) => e.range,
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => e.range,
        Value::Cast(inner)
            if expr.ty.is_integer() && (inner.ty.is_integer() || inner.ty == Type::Exact) =>
        {
            let inner_r = checked_int_range(inner, scope);
            let ty_r = expr
                .ty
                .integer_bounds()
                .map(|(lo, hi)| Range::bounds(lo, hi));
            match (inner_r, ty_r) {
                (Some(a), Some(b)) => Some(Range {
                    min: a.min.max(b.min),
                    max: a.max.min(b.max),
                }),
                (a, b) => a.or(b),
            }
        }
        _ => expr
            .ty
            .integer_bounds()
            .map(|(lo, hi)| Range::bounds(lo, hi)),
    }?;
    if let Some(a) = affine(expr, scope) {
        let (lo, hi) = scope.array_bounds.facts.borrow().proven_bounds(&a.term);
        if let Some(l) = lo.and_then(|v| v.checked_add(a.offset)) {
            let l_int = Int::from_i128(l);
            if l_int > range.min {
                range.min = l_int;
            }
        }
        if let Some(h) = hi.and_then(|v| v.checked_add(a.offset)) {
            let h_int = Int::from_i128(h);
            if h_int < range.max {
                range.max = h_int;
            }
        }
    }
    (range.min <= range.max).then_some(range)
}

/// 契約式内の整数式の証明済み値域を求める（仮引数・`return` を環境で置換）。
fn subst_range(expr: &Expression, env: &SubstEnv<'_>, scope: &Scope) -> Option<Range> {
    match &expr.kind {
        ExpressionKind::Group(inner) => subst_range(inner, env, scope),
        ExpressionKind::Integer { digits, negative } => {
            let v = Int::new(*negative, digits.parse().ok()?);
            Some(Range::point(v))
        }
        ExpressionKind::Name(name) => {
            if let Some(bound) = env.bindings.get(name.as_str()) {
                return checked_int_range(bound, scope);
            }
            let binding = scope.get(name)?;
            if let Some(c) = &binding.constant {
                return checked_int_range(c, scope);
            }
            let local = CheckedExpression {
                ty: binding.ty,
                value: Value::Local(binding.id),
            };
            checked_int_range(&local, scope)
        }
        ExpressionKind::Negate(inner) => {
            let r = subst_range(inner, env, scope)?;
            Some(Range {
                min: r.max.neg(),
                max: r.min.neg(),
            })
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let l = subst_range(left, env, scope)?;
            let r = subst_range(right, env, scope)?;
            l.binary(*operator, r).ok().flatten()
        }
        ExpressionKind::Len(inner) => {
            let target = subst_checked_target(inner, env, scope)?;
            let checked = CheckedExpression {
                ty: Type::Integer(crate::ast::IntegerType::Len),
                value: Value::Len(Box::new(target)),
            };
            checked_int_range(&checked, scope)
        }
        ExpressionKind::Field { .. } => {
            let target = subst_checked_target(expr, env, scope)?;
            checked_int_range(&target, scope)
        }
        _ => None,
    }
}

fn expr_is_integer(expr: &Expression, env: &SubstEnv<'_>, scope: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Group(inner) | ExpressionKind::Negate(inner) => {
            expr_is_integer(inner, env, scope)
        }
        ExpressionKind::Integer { .. } | ExpressionKind::Len(_) => true,
        ExpressionKind::Float(_) => false,
        ExpressionKind::Name(name) => {
            if let Some(bound) = env.bindings.get(name.as_str()) {
                return bound.ty.is_integer() || bound.ty == Type::Exact;
            }
            scope
                .get(name)
                .is_some_and(|b| b.ty.is_integer() || b.ty == Type::Exact)
        }
        ExpressionKind::Binary { left, right, .. } => {
            expr_is_integer(left, env, scope) && expr_is_integer(right, env, scope)
        }
        ExpressionKind::Cast { target, .. } => scope
            .resolve_type_at(*target, expr.location)
            .is_ok_and(|t| t.is_integer()),
        ExpressionKind::Field { .. } | ExpressionKind::Deref(_) => {
            subst_checked_target(expr, env, scope)
                .is_some_and(|c| c.ty.is_integer() || c.ty == Type::Exact)
        }
        _ => false,
    }
}

/// 契約式内の式を、仮引数・`return` 置換済みの正規化 `PredExpr` へ変換する。
fn subst_pred(expr: &Expression, env: &SubstEnv<'_>, scope: &Scope) -> Option<PredExpr> {
    match &expr.kind {
        ExpressionKind::Group(inner) => subst_pred(inner, env, scope),
        ExpressionKind::Bool(b) => Some(PredExpr::Bool(*b)),
        ExpressionKind::Integer { digits, negative } => {
            let v: i128 = digits.parse().ok()?;
            Some(PredExpr::Int(if *negative { v.checked_neg()? } else { v }))
        }
        ExpressionKind::Float(text) => {
            Some(PredExpr::FloatBits(text.parse::<f64>().ok()?.to_bits()))
        }
        ExpressionKind::Char(c) => Some(PredExpr::Char(*c)),
        ExpressionKind::Nil => Some(PredExpr::Nil),
        ExpressionKind::Name(name) => {
            if let Some(bound) = env.bindings.get(name.as_str()) {
                return pred_expr_from_checked(bound);
            }
            let binding = scope.get(name)?;
            if let Some(c) = &binding.constant {
                return pred_expr_from_checked(c);
            }
            Some(PredExpr::Local(binding.id))
        }
        ExpressionKind::Field { target, name } => {
            let checked_target = subst_checked_target(target, env, scope)?;
            let struct_id = match checked_target.ty.pointee().unwrap_or(checked_target.ty) {
                Type::Struct(id) => id,
                _ => return None,
            };
            let idx = scope
                .structs
                .get(struct_id)
                .fields
                .iter()
                .position(|f| f.name == *name)?;
            if checked_target.ty.pointee().is_none()
                && let Value::Struct { fields } = &checked_target.value
            {
                return fields
                    .iter()
                    .find(|(i, _)| *i == idx)
                    .and_then(|(_, e)| pred_expr_from_checked(e));
            }
            let base_pred = pred_expr_from_checked(&checked_target)?;
            let base_pred = if checked_target.ty.pointee().is_some() {
                PredExpr::Deref(Box::new(base_pred))
            } else {
                base_pred
            };
            Some(PredExpr::Field(Box::new(base_pred), idx))
        }
        ExpressionKind::Deref(inner) => {
            Some(PredExpr::Deref(Box::new(subst_pred(inner, env, scope)?)))
        }
        ExpressionKind::Len(inner) => {
            Some(PredExpr::Len(Box::new(subst_pred(inner, env, scope)?), 0))
        }
        ExpressionKind::Not(inner) => Some(subst_pred(inner, env, scope)?.negate()),
        ExpressionKind::Negate(inner) => {
            Some(PredExpr::Neg(Box::new(subst_pred(inner, env, scope)?)))
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => Some(PredExpr::Binary(
            *operator,
            Box::new(subst_pred(left, env, scope)?),
            Box::new(subst_pred(right, env, scope)?),
        )),
        ExpressionKind::Comparison {
            operators,
            operands,
            ..
        } if operators.len() == 1 => {
            let invertible = expr_is_integer(&operands[0], env, scope)
                && expr_is_integer(&operands[1], env, scope);
            Some(PredExpr::compare(
                operators[0],
                subst_pred(&operands[0], env, scope)?,
                subst_pred(&operands[1], env, scope)?,
                invertible,
            ))
        }
        ExpressionKind::Logical {
            operator: LogicalOperator::Or,
            left,
            right,
        } => Some(PredExpr::Or(
            Box::new(subst_pred(left, env, scope)?),
            Box::new(subst_pred(right, env, scope)?),
        )),
        ExpressionKind::Cast {
            operand, target, ..
        } => {
            let inner = subst_pred(operand, env, scope)?;
            let ty = scope.resolve_type_at(*target, expr.location).ok()?;
            if ty.is_integer() {
                Some(inner)
            } else {
                Some(PredExpr::Cast(Box::new(inner), ty))
            }
        }
        ExpressionKind::Slice {
            target,
            start: Some(start),
            index: true,
            ..
        } => Some(PredExpr::Index(
            Box::new(subst_pred(target, env, scope)?),
            vec![subst_pred(start, env, scope)?],
        )),
        _ => None,
    }
}

fn prove_comparison_pair(
    left: &Expression,
    op: ComparisonOperator,
    right: &Expression,
    env: &SubstEnv<'_>,
    scope: &Scope,
) -> bool {
    let facts = scope.array_bounds.facts.borrow();
    // 1. 差分制約グラフ（[array.bounds-proof]）
    if let (Some(l), Some(r)) = (
        subst_affine(left, env, scope),
        subst_affine(right, env, scope),
    ) {
        let ok = match op {
            ComparisonOperator::Less => facts.proves_lt(&l, &r).is_some(),
            ComparisonOperator::LessEqual => facts.proves_le(&l, &r).is_some(),
            ComparisonOperator::Greater => facts.proves_lt(&r, &l).is_some(),
            ComparisonOperator::GreaterEqual => facts.proves_le(&r, &l).is_some(),
            ComparisonOperator::Equal => facts.proves_eq(&l, &r).is_some(),
            ComparisonOperator::NotEqual => {
                facts.proves_lt(&l, &r).is_some()
                    || facts.proves_lt(&r, &l).is_some()
                    || (r.term == Term::Zero && r.offset == 0 && facts.proves_nonzero(&l))
                    || (l.term == Term::Zero && l.offset == 0 && facts.proves_nonzero(&r))
            }
        };
        if ok {
            return true;
        }
    }
    drop(facts);

    // 2. 値域（[arith.width] + 証明済み定数上下限）
    if let (Some(lr), Some(rr)) = (
        subst_range(left, env, scope),
        subst_range(right, env, scope),
    ) {
        let ok = match op {
            ComparisonOperator::Less => lr.max < rr.min,
            ComparisonOperator::LessEqual => lr.max <= rr.min,
            ComparisonOperator::Greater => lr.min > rr.max,
            ComparisonOperator::GreaterEqual => lr.min >= rr.max,
            ComparisonOperator::Equal => lr.min == lr.max && rr.min == rr.max && lr.min == rr.min,
            ComparisonOperator::NotEqual => lr.max < rr.min || lr.min > rr.max,
        };
        if ok {
            return true;
        }
    }

    // 3. 一般条件の構文一致（括弧除去・比較反転・&& 分解済み）
    if let (Some(lp), Some(rp)) = (subst_pred(left, env, scope), subst_pred(right, env, scope)) {
        let invertible = expr_is_integer(left, env, scope) && expr_is_integer(right, env, scope);
        let target = PredExpr::compare(op, lp, rp, invertible);
        if scope
            .array_bounds
            .facts
            .borrow()
            .has_predicate(&target)
            .is_some()
        {
            return true;
        }
    }
    false
}

fn prove_condition(expr: &Expression, env: &SubstEnv<'_>, scope: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Group(inner) => prove_condition(inner, env, scope),
        ExpressionKind::Bool(b) => *b,
        ExpressionKind::Logical {
            operator: LogicalOperator::And,
            left,
            right,
        } => prove_condition(left, env, scope) && prove_condition(right, env, scope),
        ExpressionKind::Logical {
            operator: LogicalOperator::Or,
            left,
            right,
        } => {
            if prove_condition(left, env, scope) || prove_condition(right, env, scope) {
                return true;
            }
            if let Some(pred) = subst_pred(expr, env, scope)
                && scope
                    .array_bounds
                    .facts
                    .borrow()
                    .has_predicate(&pred)
                    .is_some()
            {
                return true;
            }
            false
        }
        ExpressionKind::Comparison {
            operators,
            operands,
            ..
        } => (0..operators.len()).all(|k| {
            prove_comparison_pair(&operands[k], operators[k], &operands[k + 1], env, scope)
        }),
        _ => {
            if let Some(pred) = subst_pred(expr, env, scope) {
                if pred == PredExpr::Bool(true) {
                    return true;
                }
                if scope
                    .array_bounds
                    .facts
                    .borrow()
                    .has_predicate(&pred)
                    .is_some()
                {
                    return true;
                }
            }
            false
        }
    }
}

/// 関数・メソッド呼び出し位置で `@requires` をすべて静的に証明する。
pub(super) fn check_call(
    signature: &Signature,
    arguments: &[CheckedExpression],
    call_location: Location,
    scope: &Scope,
) -> Result<(), Diagnostic> {
    if signature.contracts.is_empty() {
        return Ok(());
    }
    // 汎用関数の定義時検査（記号的型）では具体型が未確定のため、具体化時に判定する。
    if scope.generic_check() {
        return Ok(());
    }
    let env = SubstEnv::from_call(signature, arguments);
    for contract in signature
        .contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Requires)
    {
        if !prove_condition(&contract.condition, &env, scope) {
            let mut diag = call_location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "contract.requires",
                    "cannot statically prove @requires precondition for this call"
                ),
            );
            diag.related.push(contract.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("contract.requires", "precondition declared here"),
            ));
            return Err(diag);
        }
    }
    Ok(())
}

/// `return` 位置で現在の関数の `@ensures` をすべて静的に証明する。
pub(super) fn check_return(
    returned: &CheckedExpression,
    return_location: Location,
    scope: &Scope,
) -> Result<(), Diagnostic> {
    if scope.current_ensures.is_empty() || scope.generic_check() {
        return Ok(());
    }
    let env = SubstEnv::from_return(returned);
    for contract in scope.current_ensures.iter() {
        if !prove_condition(&contract.condition, &env, scope) {
            let mut diag = return_location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "contract.ensures",
                    "cannot statically prove @ensures postcondition at return"
                ),
            );
            diag.related.push(contract.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("contract.ensures", "postcondition declared here"),
            ));
            return Err(diag);
        }
    }
    Ok(())
}

/// 初期化式が `@ensures` を持つ関数の直接呼び出し（または脱出ハンドラ付き呼び出し）なら `(callee_id, args)` を抽出する。
pub(super) fn extract_ensures_call(
    initializer: &CheckedExpression,
    scope: &Scope,
) -> Option<(usize, Vec<CheckedExpression>)> {
    let (callee_id, call_args) = match &initializer.value {
        Value::Call { id, arguments, .. } => (*id, arguments.as_slice()),
        Value::HandledCall { call, handler } => {
            let exits = matches!(
                handler.as_ref(),
                super::ir::CallHandler::Return { .. } | super::ir::CallHandler::Fail(_)
            );
            if !exits {
                return None;
            }
            match &call.value {
                Value::Call { id, arguments, .. } => (*id, arguments.as_slice()),
                _ => return None,
            }
        }
        _ => return None,
    };
    let signature = scope.functions.values().find(|s| s.id == callee_id)?;
    if !signature
        .contracts
        .iter()
        .any(|c| c.kind == ContractKind::Ensures)
    {
        return None;
    }
    Some((callee_id, call_args.to_vec()))
}

/// `fix x = f(...)` のような束縛で、呼び出し先関数の `@ensures` を呼び出し元の `Facts` へ引き継ぐ。
/// 別名更新で壊れうる参照先・配列要素に依存する条件は持ち越さない。
pub(super) fn record_ensures_for_call(
    binding_id: usize,
    binding_ty: Type,
    ensures_call: Option<(usize, Vec<CheckedExpression>)>,
    location: Location,
    scope: &Scope,
) {
    let Some((callee_id, call_args)) = ensures_call else {
        return;
    };
    let Some(signature) = scope.functions.values().find(|s| s.id == callee_id) else {
        return;
    };
    let result_expr = CheckedExpression {
        ty: binding_ty,
        value: Value::Local(binding_id),
    };
    let mut env = SubstEnv::from_call(signature, &call_args);
    env.bindings.insert("return", &result_expr);
    for contract in signature
        .contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Ensures)
    {
        record_ensures_expr(&contract.condition, &env, location, scope);
    }
}

fn record_ensures_expr(expr: &Expression, env: &SubstEnv<'_>, location: Location, scope: &Scope) {
    match &expr.kind {
        ExpressionKind::Group(inner) => record_ensures_expr(inner, env, location, scope),
        ExpressionKind::Logical {
            operator: LogicalOperator::And,
            left,
            right,
        } => {
            record_ensures_expr(left, env, location, scope);
            record_ensures_expr(right, env, location, scope);
        }
        ExpressionKind::Comparison {
            operators,
            operands,
            ..
        } => {
            for k in 0..operators.len() {
                if let (Some(l), Some(r)) = (
                    subst_affine(&operands[k], env, scope),
                    subst_affine(&operands[k + 1], env, scope),
                ) {
                    scope.array_bounds.facts.borrow_mut().add_comparison(
                        &l,
                        operators[k],
                        &r,
                        false,
                        location,
                    );
                }
                if let (Some(lp), Some(rp)) = (
                    subst_pred(&operands[k], env, scope),
                    subst_pred(&operands[k + 1], env, scope),
                ) {
                    let invertible = expr_is_integer(&operands[k], env, scope)
                        && expr_is_integer(&operands[k + 1], env, scope);
                    let pred = PredExpr::compare(operators[k], lp, rp, invertible);
                    let mut locals = Vec::new();
                    let mut shared = false;
                    pred.collect_deps(&mut locals, &mut shared);
                    if !shared {
                        scope
                            .array_bounds
                            .facts
                            .borrow_mut()
                            .add_predicate(pred, location);
                    }
                }
            }
        }
        _ => {
            if let Some(pred) = subst_pred(expr, env, scope) {
                let mut locals = Vec::new();
                let mut shared = false;
                pred.collect_deps(&mut locals, &mut shared);
                if !shared {
                    scope
                        .array_bounds
                        .facts
                        .borrow_mut()
                        .add_predicate(pred, location);
                }
            }
        }
    }
}
