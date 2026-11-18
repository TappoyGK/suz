//! 型検査前に制御移動の位置を検査し、許可された失敗時returnをIRの文へ変換する。戻り値と関数末尾の制御も確認する。
//!
//! 利用元: [`crate::check::functions`]、[`crate::check::statements`]。
//! 主な対象規則: `[expr.jump-prohibited]`、`[expr.or-return-position]`、`[fn.fallthrough]`、`[fn.value-return-required]`。

use super::ir::{CheckedExpression, CheckedHandler, CheckedStatement, IntegerOperation, Value};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Statement},
};

// 型検査の前に制御移動の位置を検査する。未対応の演算でも、そのオペランド内に
// 隠れた制御移動がないか調べる。
pub(super) fn validate_return_positions(statements: &[Statement]) -> Result<(), Diagnostic> {
    validate_statements(statements, false)
}

fn validate_statements(statements: &[Statement], in_value: bool) -> Result<(), Diagnostic> {
    fn visit_handler(
        fallback: &Option<FailureHandler>,
        allowed: bool,
        in_value: bool,
    ) -> Result<(), Diagnostic> {
        match fallback {
            Some(FailureHandler::Errors(handler)) => {
                let crate::ast::ErrorHandler { arms, location } = handler.as_ref();
                if error_arms_escape(arms) && (in_value || !allowed) {
                    let message = if in_value {
                        crate::diagnostic!(
                            "expr.jump-prohibited",
                            "control transfers are forbidden inside value expressions"
                        )
                    } else {
                        crate::diagnostic!(
                            "expr.or-return-position",
                            "error handlers with control transfers require an entire initializer, assignment RHS, or call statement"
                        )
                    };
                    return Err(location.error(ErrorKind::InvalidSource, message));
                }
                for arm in arms {
                    match &arm.body {
                        crate::ast::ErrorArmBody::Value(value) => visit(value, false, in_value)?,
                        crate::ast::ErrorArmBody::Statements { body, .. } => {
                            validate_statements(body, in_value)?
                        }
                    }
                }
            }

            Some(FailureHandler::Return { value, location }) => {
                if in_value || !allowed {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        if in_value {
                            crate::diagnostic!(
                                "expr.jump-prohibited",
                                "control transfers are forbidden inside value expressions"
                            )
                        } else {
                            crate::diagnostic!(
                                "expr.or-return-position",
                                "or return requires an entire initializer, assignment RHS, or void call statement"
                            )
                        },
                    ));
                }
                visit(value, false, in_value)?;
            }
            Some(FailureHandler::Fail { value, location }) => {
                if in_value || !allowed {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        if in_value {
                            crate::diagnostic!(
                                "expr.jump-prohibited",
                                "control transfers are forbidden inside value expressions"
                            )
                        } else {
                            crate::diagnostic!(
                                "expr.or-return-position",
                                "or fail requires an entire initializer, assignment RHS, or void call statement"
                            )
                        },
                    ));
                }
                if let Some(value) = value {
                    visit(value, false, in_value)?;
                }
            }
            Some(FailureHandler::Ignore(_)) => {}
            Some(FailureHandler::Value(value)) => visit(value, false, in_value)?,
            None => {}
        }
        Ok(())
    }
    fn visit(expr: &Expression, allowed: bool, in_value: bool) -> Result<(), Diagnostic> {
        match &expr.kind {
            ExpressionKind::Slice {
                target,
                start,
                end,
                fallback,
                ..
            } => {
                visit(target, false, in_value)?;
                for bound in [start, end].into_iter().flatten() {
                    visit(bound, false, in_value)?;
                }
                visit_handler(fallback, allowed, in_value)?;
            }
            ExpressionKind::Template(parts) | ExpressionKind::Panic(parts) => {
                for part in parts {
                    if let crate::ast::TemplatePart::Value(value) = part {
                        visit(value, false, in_value)?;
                    }
                }
            }
            ExpressionKind::With { target, fields } => {
                visit(target, false, in_value)?;
                for field in fields {
                    visit(&field.value, false, in_value)?;
                }
            }
            ExpressionKind::Struct { fields, .. } => {
                for field in fields {
                    visit(&field.value, false, in_value)?;
                }
            }
            ExpressionKind::Field { target, .. } => visit(target, false, in_value)?,
            ExpressionKind::Invoke { target, arguments } => {
                visit(target, false, in_value)?;
                for argument in arguments {
                    visit(argument, false, in_value)?;
                }
            }
            ExpressionKind::Call { arguments, .. } => {
                for argument in arguments {
                    visit(argument, false, in_value)?;
                }
            }
            ExpressionKind::Block { statements, value } => {
                validate_statements(statements, true)?;
                visit(value, false, true)?;
            }
            ExpressionKind::Match { target, arms } => {
                visit(target, false, true)?;
                for arm in arms {
                    visit(&arm.body, false, true)?;
                }
            }
            ExpressionKind::Group(inner) => visit(inner, allowed, in_value)?,
            ExpressionKind::ArithmeticHandler { operand, fallback }
            | ExpressionKind::Cast {
                operand, fallback, ..
            } => {
                visit(operand, false, in_value)?;
                visit_handler(fallback, allowed, in_value)?;
            }
            ExpressionKind::If {
                branches,
                otherwise,
            } => {
                for (condition, value) in branches {
                    visit(condition, false, true)?;
                    visit(value, false, true)?;
                }
                visit(otherwise, false, true)?;
            }
            ExpressionKind::Negate(inner)
            | ExpressionKind::New(inner)
            | ExpressionKind::Deref(inner)
            | ExpressionKind::CBytes(inner)
            | ExpressionKind::CByteLen(inner)
            | ExpressionKind::Len(inner)
            | ExpressionKind::BitNot(inner)
            | ExpressionKind::Not(inner) => visit(inner, false, in_value)?,
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Logical { left, right, .. } => {
                visit(left, false, in_value)?;
                visit(right, false, in_value)?;
            }
            ExpressionKind::Comparison { operands, .. } => {
                for operand in operands {
                    visit(operand, false, in_value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for statement in statements {
        match statement {
            Statement::Log { parts, .. } => {
                for part in parts {
                    if let crate::ast::TemplatePart::Value(value) = part {
                        visit(value, false, in_value)?;
                    }
                }
            }
            // ここでは呼び出し全体という構文だけを許可し、成功型voidは文の型検査で確認する。
            Statement::Expression(value) => visit(value, call_statement(value), in_value)?,
            Statement::Match { target, arms, .. } => {
                visit(target, false, in_value)?;
                for arm in arms {
                    if let Some(pattern) = &arm.pattern {
                        visit(pattern, false, in_value)?;
                    }
                    validate_statements(&arm.body, in_value)?;
                }
            }
            Statement::For {
                start, end, body, ..
            } => {
                visit(start, false, in_value)?;
                if let Some(end) = end {
                    visit(end, false, in_value)?;
                }
                validate_statements(body, in_value)?;
            }
            Statement::Binding { initializer, .. } => visit(initializer, true, in_value)?,
            Statement::Assign {
                value, compound, ..
            } => {
                if *compound {
                    let ExpressionKind::Binary { left, right, .. } = &value.kind else {
                        unreachable!()
                    };
                    visit(left, false, in_value)?;
                    visit(right, true, in_value)?;
                } else {
                    visit(value, true, in_value)?;
                }
            }
            Statement::Return(value) | Statement::Fail(value) | Statement::Recover(value) => {
                visit(value, false, in_value)?
            }
            Statement::Loop { condition, body } => {
                if let Some(condition) = condition {
                    visit(condition, false, in_value)?;
                }
                validate_statements(body, in_value)?;
            }
            Statement::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    visit(condition, false, in_value)?;
                    validate_statements(body, in_value)?;
                }
                if let Some(body) = otherwise {
                    validate_statements(body, in_value)?;
                }
            }
            Statement::IfLet {
                value,
                body,
                otherwise,
                ..
            } => {
                visit(value, false, in_value)?;
                validate_statements(body, in_value)?;
                validate_statements(otherwise, in_value)?;
            }
            Statement::Store { target, value, .. } => {
                visit(target, false, in_value)?;
                visit(value, true, in_value)?;
            }
            Statement::Arena { body, .. } | Statement::Block(body) | Statement::Unsafe(body) => {
                validate_statements(body, in_value)?
            }
            _ => {}
        }
    }
    Ok(())
}

fn call_statement(expr: &Expression) -> bool {
    fn ungroup(mut expr: &Expression) -> &Expression {
        while let ExpressionKind::Group(inner) = &expr.kind {
            expr = inner;
        }
        expr
    }
    matches!(&ungroup(expr).kind, ExpressionKind::ArithmeticHandler { operand, .. }
        if matches!(ungroup(operand).kind, ExpressionKind::Call { .. } | ExpressionKind::Invoke { .. }))
}

fn has_return_handler(expr: &CheckedExpression) -> bool {
    match &expr.value {
        Value::Cast(inner) | Value::TextView(inner) => has_return_handler(inner),
        Value::HandledCall { handler, .. } => matches!(
            handler.as_ref(),
            super::ir::CallHandler::Return(_)
                | super::ir::CallHandler::Fail(_)
                | super::ir::CallHandler::Branches(_)
        ),
        Value::CastReturn { .. }
        | Value::Slice {
            handler: Some(CheckedHandler::Return(_)),
            ..
        }
        | Value::IntegerArithmetic {
            handler: Some(CheckedHandler::Return(_)),
            ..
        } => true,
        _ => false,
    }
}

// 許可された失敗時returnを文へ持ち上げる。これによりC生成へ渡す値の式には関数からの脱出が残らない。
pub(super) fn lower_return_handler(
    expr: &mut CheckedExpression,
    statements: &mut Vec<CheckedStatement>,
    next_id: &mut usize,
    location: crate::ast::Location,
) -> bool {
    if matches!(&expr.value, Value::HandledCall { handler, .. } if matches!(handler.as_ref(), super::ir::CallHandler::Return(_) | super::ir::CallHandler::Fail(_) | super::ir::CallHandler::Branches(_)))
    {
        let id = *next_id;
        *next_id += 1;
        let replacement = if expr.ty == crate::ast::Type::Void {
            Value::Void
        } else {
            Value::Local(id)
        };
        let Value::HandledCall { call, handler } = std::mem::replace(&mut expr.value, replacement)
        else {
            unreachable!()
        };
        statements.push(CheckedStatement::HandleCall { id, call, handler });
        return true;
    }
    if let Value::Cast(inner) | Value::TextView(inner) = &mut expr.value {
        return lower_return_handler(inner, statements, next_id, location);
    }
    if matches!(
        &expr.value,
        Value::Slice {
            handler: Some(CheckedHandler::Return(_)),
            ..
        } | Value::IntegerArithmetic {
            handler: Some(CheckedHandler::Return(_)),
            ..
        }
    ) {
        let id = *next_id;
        *next_id += 1;
        let value = std::mem::replace(&mut expr.value, Value::Local(id));
        let mut operation = CheckedExpression { ty: expr.ty, value };
        let (handler, slice) = match &mut operation.value {
            Value::Slice { handler, .. } => (handler, true),
            Value::IntegerArithmetic { handler, .. } => (handler, false),
            _ => unreachable!(),
        };
        let Some(CheckedHandler::Return(return_value)) = handler.take() else {
            unreachable!()
        };
        statements.push(if slice {
            CheckedStatement::SliceReturn {
                id,
                operation,
                return_value,
            }
        } else {
            CheckedStatement::ArithmeticReturn {
                id,
                operation,
                return_value,
            }
        });
        return true;
    }
    if let Value::IntegerArithmetic {
        operation: IntegerOperation::Exact(exact),
        ..
    } = &mut expr.value
        && let super::exact::ExactKind::Binary(_, left, right) = &mut exact.kind
        && let super::exact::ExactKind::Leaf(rhs) = &mut right.kind
        && has_return_handler(rhs)
    {
        let id = *next_id;
        *next_id += 1;
        let replacement = super::exact::Exact {
            domains: None,
            arithmetic: false,
            kind: super::exact::ExactKind::Leaf(Box::new(CheckedExpression {
                ty: crate::ast::Type::Exact,
                value: Value::Local(id),
            })),
            range: left.range,
            location: left.location,
        };
        let old = std::mem::replace(left, Box::new(replacement));
        statements.push(CheckedStatement::Binding {
            location,
            mutable: false,
            id,
            ty: crate::ast::Type::Exact,
            initializer: Box::new(CheckedExpression {
                ty: crate::ast::Type::Exact,
                value: Value::Exact(old),
            }),
        });
        return lower_return_handler(rhs, statements, next_id, location);
    }
    if let Value::Binary { left, right, .. }
    | Value::IntegerArithmetic {
        operation: IntegerOperation::Binary { left, right, .. },
        ..
    } = &mut expr.value
    {
        if !has_return_handler(right) {
            return false;
        }
        // 複合代入は右辺を評価する前に、代入先の古い値を読む。
        let id = *next_id;
        *next_id += 1;
        let ty = left.ty;
        let old = std::mem::replace(
            left,
            Box::new(CheckedExpression {
                ty,
                value: Value::Local(id),
            }),
        );
        statements.push(CheckedStatement::Binding {
            location,
            mutable: false,
            id,
            ty,
            initializer: old,
        });
        return lower_return_handler(right, statements, next_id, location);
    }
    if !matches!(expr.value, Value::CastReturn { .. }) {
        return false;
    }
    let id = *next_id;
    *next_id += 1;
    let Value::CastReturn {
        location,
        wrap,
        operand,
        return_value,
    } = std::mem::replace(&mut expr.value, Value::Local(id))
    else {
        unreachable!()
    };
    statements.push(CheckedStatement::CastReturn {
        location,
        wrap,
        id,
        ty: expr.ty,
        operand,
        return_value,
    });
    true
}

fn contains_break(statements: &[Statement]) -> bool {
    use crate::ast::visit::{self, Visitor};
    struct Breaks(bool);
    impl Visitor for Breaks {
        fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
            match statement {
                Statement::Break(_) => self.0 = true,
                // 入れ子のループ内のbreakは、その内側のループに属する。
                Statement::For { .. } | Statement::Loop { .. } => return Ok(()),
                _ => {}
            }
            visit::walk_statement(self, statement)
        }
        fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
            if matches!(expr.kind, ExpressionKind::Closure(_)) {
                return Ok(());
            }
            // 式内のor errorにも、現在のループを抜けるbreakが書ける。
            visit::walk_expression(self, expr)
        }
    }
    let mut breaks = Breaks(false);
    breaks.scope(statements, None).unwrap();
    breaks.0
}
pub(super) fn stops(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::IfLet {
            body, otherwise, ..
        } => stops(body) && stops(otherwise),
        Statement::Match { arms, .. } => arms.iter().all(|arm| stops(&arm.body)),
        Statement::Return(_)
        | Statement::Fail(_)
        | Statement::Recover(_)
        | Statement::Break(_)
        | Statement::Continue(_)
        | Statement::Log {
            level: crate::ast::LogLevel::Panic,
            ..
        } => true,
        Statement::Loop { condition, body } => {
            condition
                .as_ref()
                .is_none_or(|e| matches!(e.kind, ExpressionKind::Bool(true)))
                && !contains_break(body)
        }
        Statement::Arena { body: inner, .. }
        | Statement::Block(inner)
        | Statement::Unsafe(inner) => stops(inner),
        Statement::If {
            branches,
            otherwise,
        } => {
            otherwise.as_ref().is_some_and(|body| stops(body))
                && branches.iter().all(|(_, body)| stops(body))
        }
        _ => false,
    })
}
pub(super) fn check_return_shape(function: &crate::ast::Function) -> Result<(), Diagnostic> {
    use crate::ast::visit::{self, Visitor};
    if function.result == crate::ast::Type::Void {
        return Ok(());
    }
    struct Returns(bool);
    impl Visitor for Returns {
        fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
            self.0 |= matches!(
                statement,
                Statement::Return(_)
                    | Statement::Fail(_)
                    | Statement::Log {
                        level: crate::ast::LogLevel::Panic,
                        ..
                    }
            );
            visit::walk_statement(self, statement)
        }
        fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
            if matches!(expr.kind, ExpressionKind::Closure(_)) {
                return Ok(()); // 内側のreturn・panicは外側の関数の終了条件に数えない。
            }
            if let ExpressionKind::Cast { fallback, .. }
            | ExpressionKind::ArithmeticHandler { fallback, .. }
            | ExpressionKind::Slice { fallback, .. } = &expr.kind
            {
                self.0 |= matches!(
                    fallback,
                    Some(FailureHandler::Return { .. } | FailureHandler::Fail { .. })
                );
            }
            visit::walk_expression(self, expr)
        }
    }
    let mut returns = Returns(false);
    returns.scope(&function.statements, None)?;
    if !returns.0 {
        let message = if function.errors.is_empty() {
            "a value-returning function requires a return expression or panic statement"
        } else {
            "a value-returning function with declared errors requires a return expression, panic statement, or fail statement"
        };
        return Err(function.end.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("fn.value-return-required", "{message}"),
        ));
    }
    if !stops(&function.statements) {
        return Err(function.end.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.fallthrough",
                "a value-returning function may reach its end"
            ),
        ));
    }
    Ok(())
}

/// 内側のクロージャの脱出は、そのクロージャ自身に属する。
pub(super) fn error_arms_escape(arms: &[crate::ast::MatchArm<crate::ast::ErrorArmBody>]) -> bool {
    use crate::ast::visit::{self, Visitor};
    struct Jumps(bool);
    impl Visitor for Jumps {
        fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
            self.0 |= matches!(
                statement,
                Statement::Return(_)
                    | Statement::Fail(_)
                    | Statement::Break(_)
                    | Statement::Continue(_)
            );
            visit::walk_statement(self, statement)
        }
        fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
            if matches!(expr.kind, ExpressionKind::Closure(_)) {
                return Ok(());
            }
            if let ExpressionKind::ArithmeticHandler { fallback, .. }
            | ExpressionKind::Cast { fallback, .. }
            | ExpressionKind::Slice { fallback, .. } = &expr.kind
            {
                self.0 |= matches!(
                    fallback,
                    Some(FailureHandler::Return { .. } | FailureHandler::Fail { .. })
                );
            }
            visit::walk_expression(self, expr)
        }
    }
    let mut jumps = Jumps(false);
    for arm in arms {
        match &arm.body {
            crate::ast::ErrorArmBody::Value(value) => jumps.expression(value).unwrap(),
            crate::ast::ErrorArmBody::Statements { body, .. } => jumps.scope(body, None).unwrap(),
        }
    }
    jumps.0
}
