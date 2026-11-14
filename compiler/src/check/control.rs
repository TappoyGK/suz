use super::ir::{CheckedExpression, CheckedHandler, CheckedStatement, IntegerOperation, Value};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Statement},
};

// Validate positions before type checking, including unsupported arithmetic
// whose operands may contain an otherwise hidden control transfer.
pub(super) fn validate_return_positions(statements: &[Statement]) -> Result<(), Diagnostic> {
    validate_statements(statements, false)
}

fn validate_statements(statements: &[Statement], in_value: bool) -> Result<(), Diagnostic> {
    fn visit(expr: &Expression, allowed: bool, in_value: bool) -> Result<(), Diagnostic> {
        match &expr.kind {
            ExpressionKind::Struct { fields, .. } => {
                for field in fields {
                    visit(&field.value, false, in_value)?;
                }
            }
            ExpressionKind::Field { target, .. } => visit(target, false, in_value)?,
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
                match fallback {
                    Some(FailureHandler::Return { value, location }) => {
                        if in_value || !allowed {
                            return Err(location.error(ErrorKind::InvalidSource, if in_value {
                                "control transfers are forbidden inside value expressions [expr.jump-prohibited]"
                            } else { "or return must apply to the entire initializer or assignment RHS [expr.or-return-position]" }));
                        }
                        visit(value, false, in_value)?;
                    }
                    Some(FailureHandler::Value(value)) => visit(value, false, in_value)?,
                    None => {}
                }
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
            | ExpressionKind::BitNot(inner)
            | ExpressionKind::Not(inner)
            | ExpressionKind::Truncate { operand: inner, .. } => visit(inner, false, in_value)?,
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
            Statement::Expression(value) => visit(value, false, in_value)?,
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
                visit(end, false, in_value)?;
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
            Statement::Return(value) => visit(value, false, in_value)?,
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
            Statement::Block(body) => validate_statements(body, in_value)?,
            _ => {}
        }
    }
    Ok(())
}

// Hoist permitted failure returns into statements. Checked value expressions
// passed to the backend then contain no function exits.
pub(super) fn lower_return_handler(
    expr: &mut CheckedExpression,
    statements: &mut Vec<CheckedStatement>,
    next_id: &mut usize,
) -> bool {
    if let Value::IntegerArithmetic {
        handler: Some(CheckedHandler::Return(_)),
        ..
    } = &expr.value
    {
        let id = *next_id;
        *next_id += 1;
        let value = std::mem::replace(&mut expr.value, Value::Local(id));
        let mut operation = CheckedExpression { ty: expr.ty, value };
        let Value::IntegerArithmetic { handler, .. } = &mut operation.value else {
            unreachable!()
        };
        let Some(CheckedHandler::Return(return_value)) = handler.take() else {
            unreachable!()
        };
        statements.push(CheckedStatement::ArithmeticReturn {
            id,
            operation,
            return_value: *return_value,
        });
        return true;
    }
    if let Value::Binary { left, right, .. }
    | Value::IntegerArithmetic {
        operation: IntegerOperation::Binary { left, right, .. },
        ..
    } = &mut expr.value
    {
        if !matches!(
            right.value,
            Value::CastReturn { .. }
                | Value::IntegerArithmetic {
                    handler: Some(CheckedHandler::Return(_)),
                    ..
                }
        ) {
            return false;
        }
        // Compound assignment reads the old value before evaluating its RHS.
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
            mutable: false,
            id,
            ty,
            initializer: *old,
        });
        return lower_return_handler(right, statements, next_id);
    }
    if !matches!(expr.value, Value::CastReturn { .. }) {
        return false;
    }
    let id = *next_id;
    *next_id += 1;
    let Value::CastReturn {
        operand,
        return_value,
    } = std::mem::replace(&mut expr.value, Value::Local(id))
    else {
        unreachable!()
    };
    statements.push(CheckedStatement::CastReturn {
        id,
        ty: expr.ty,
        operand: *operand,
        return_value: *return_value,
    });
    true
}

fn contains_break(statements: &[CheckedStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        CheckedStatement::Match { arms, .. } => arms.iter().any(|(_, body)| contains_break(body)),
        CheckedStatement::Break => true,
        CheckedStatement::Block(inner) => contains_break(inner),
        CheckedStatement::If {
            branches,
            otherwise,
        } => {
            branches.iter().any(|(_, body)| contains_break(body))
                || otherwise.as_ref().is_some_and(|body| contains_break(body))
        }
        // Breaks inside a nested loop belong to that loop.
        _ => false,
    })
}
pub(super) fn stops(statements: &[CheckedStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        CheckedStatement::Match { arms, .. } => arms.iter().all(|(_, body)| stops(body)),
        CheckedStatement::Return(_) | CheckedStatement::Break | CheckedStatement::Continue => true,
        CheckedStatement::Loop {
            unconditional,
            body,
            ..
        } => *unconditional && !contains_break(body),
        CheckedStatement::Block(inner) => stops(inner),
        CheckedStatement::If {
            branches,
            otherwise,
        } => {
            otherwise.as_ref().is_some_and(|body| stops(body))
                && branches.iter().all(|(_, body)| stops(body))
        }
        _ => false,
    })
}
