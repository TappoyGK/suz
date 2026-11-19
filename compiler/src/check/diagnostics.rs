//! 検査済みの式と文から、整数除算・比較・未処理の整数演算の診断候補を集める。
//!
//! 利用元: [`crate::check::functions`]。
//! 主な対象規則: `[diagnostic.integer-division-to-float]`、`[diagnostic.constant-comparison]`、`[diagnostic.arithmetic]`。

use super::ir::{CheckedExpression, CheckedHandler, CheckedStatement, Value};
use crate::ast::Location;

pub(super) fn collect(expr: &CheckedExpression, locations: &mut Vec<Location>) {
    match &expr.value {
        Value::HandledCall { call, handler } => {
            collect(call, locations);
            collect_handler(handler, locations);
        }
        Value::Closure(closure) => collect_statements(&closure.function.statements, locations),
        Value::FunctionBorrow(inner) => collect(inner, locations),
        Value::Exact(e) => {
            if e.arithmetic {
                locations.push(e.location);
            }
            e.leaves(&mut |v| collect(v, locations));
        }
        Value::TextView(inner) | Value::Intern { operand: inner, .. } => collect(inner, locations),
        Value::Slice {
            target,
            start,
            end,
            handler,
            ..
        } => {
            collect(target, locations);
            for bound in [start, end].into_iter().flatten() {
                collect(bound, locations);
            }
            if let Some(CheckedHandler::Value(value)) = handler {
                collect(value, locations);
            }
        }
        Value::Template { parts, .. } | Value::Panic { parts, .. } => {
            for part in parts {
                if let crate::ast::TemplatePart::Value(value) = part {
                    collect(value, locations);
                }
            }
        }
        Value::With { target, fields } => {
            collect(target, locations);
            for (_, field) in fields {
                collect(field, locations);
            }
        }
        Value::Struct { fields } => {
            for (_, field) in fields {
                collect(field, locations);
            }
        }
        Value::Field { target, .. } => collect(target, locations),
        Value::Invoke {
            target, arguments, ..
        } => {
            collect(target, locations);
            for argument in arguments {
                collect(argument, locations);
            }
        }
        Value::Call { arguments, .. } => {
            for argument in arguments {
                collect(argument, locations);
            }
        }
        Value::Block {
            statements, value, ..
        } => {
            collect_statements(statements, locations);
            collect(value, locations);
        }
        Value::Match {
            target,
            arms,
            pattern_warnings,
        } => {
            collect(target, locations);
            locations.extend(pattern_warnings);
            for (_, value) in arms {
                collect(value, locations);
            }
        }
        Value::IntegerArithmetic {
            operation,
            location,
            handler,
            ..
        } => {
            if handler.is_none() {
                locations.push(*location);
            }
            if let Some(handler) = handler {
                match handler {
                    CheckedHandler::Value(value) => collect(value, locations),
                    CheckedHandler::Return { .. } => {
                        unreachable!("failure returns must be lowered to statements")
                    }
                }
            }
            if let super::ir::IntegerOperation::Exact(e) = operation {
                e.leaves(&mut |v| collect(v, locations));
            }
            for operand in operation.operands() {
                collect(operand, locations);
            }
        }
        Value::If {
            branches,
            otherwise,
        } => {
            for (condition, value) in branches {
                collect(condition, locations);
                collect(value, locations);
            }
            collect(otherwise, locations);
        }
        Value::Integer {
            minus: Some(location),
            ..
        } => locations.push(*location),
        Value::CastReturn { .. } => unreachable!("cast returns must be lowered to statements"),
        Value::CheckedCast {
            operand, fallback, ..
        } => {
            collect(operand, locations);
            collect(fallback, locations);
        }
        Value::Cast(inner)
        | Value::Truncate(inner)
        | Value::Not(inner)
        | Value::BitNot(inner)
        | Value::Negate(inner) => collect(inner, locations),
        Value::Logical { left, right, .. } | Value::Binary { left, right, .. } => {
            collect(left, locations);
            collect(right, locations);
        }
        Value::Comparison { operands, .. } => {
            for operand in operands {
                collect(operand, locations);
            }
        }
        _ => {}
    }
}
fn collect_statements(statements: &[CheckedStatement], locations: &mut Vec<Location>) {
    for statement in statements {
        match statement {
            CheckedStatement::Ignore(_) => {}
            CheckedStatement::HandleCall { call, handler, .. } => {
                collect(call, locations);
                collect_handler(handler, locations);
            }
            CheckedStatement::Fail(value) | CheckedStatement::Recover(value) => {
                collect(value, locations)
            }
            CheckedStatement::SliceReturn {
                operation,
                return_value,
                ..
            } => {
                collect(operation, locations);
                collect(return_value, locations);
            }
            CheckedStatement::Log { parts, .. } => {
                for part in parts {
                    if let crate::ast::TemplatePart::Value(value) = part {
                        collect(value, locations);
                    }
                }
            }
            CheckedStatement::Expression(value) => collect(value, locations),
            CheckedStatement::Match {
                target,
                arms,
                pattern_warnings,
                ..
            } => {
                collect(target, locations);
                locations.extend(pattern_warnings);
                for (_, body) in arms {
                    collect_statements(body, locations);
                }
            }
            CheckedStatement::For {
                start, end, body, ..
            } => {
                collect(start, locations);
                if let Some(end) = end {
                    collect(end, locations);
                }
                collect_statements(body, locations);
            }
            CheckedStatement::ArithmeticReturn {
                operation,
                return_value,
                ..
            } => {
                let Value::IntegerArithmetic { operation, .. } = &operation.value else {
                    unreachable!()
                };
                if let super::ir::IntegerOperation::Exact(e) = operation {
                    e.leaves(&mut |v| collect(v, locations));
                }
                for operand in operation.operands() {
                    collect(operand, locations);
                }
                collect(return_value, locations);
            }
            CheckedStatement::CastReturn {
                operand,
                return_value,
                ..
            } => {
                collect(operand, locations);
                collect(return_value, locations);
            }
            CheckedStatement::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    collect(condition, locations);
                    collect_statements(body, locations);
                }
                if let Some(body) = otherwise {
                    collect_statements(body, locations);
                }
            }
            CheckedStatement::Loop {
                condition, body, ..
            } => {
                if let Some(condition) = condition {
                    collect(condition, locations);
                }
                collect_statements(body, locations);
            }
            CheckedStatement::Break | CheckedStatement::Continue => {}
            CheckedStatement::Arena { body: inner, .. } | CheckedStatement::Block(inner) => {
                collect_statements(inner, locations)
            }
            CheckedStatement::Store {
                target,
                value,
                body,
                ..
            } => {
                collect(target, locations);
                collect_statements(body, locations);
                collect(value, locations);
            }
            CheckedStatement::Binding { initializer, .. } => collect(initializer, locations),
            CheckedStatement::Assign { value, .. } => collect(value, locations),
            CheckedStatement::Return { value, .. } => collect(value, locations),
        }
    }
}

pub(super) fn unhandled_arithmetic(statements: &[CheckedStatement]) -> Vec<Location> {
    let mut locations = Vec::new();
    collect_statements(statements, &mut locations);
    locations
}

fn collect_handler(handler: &super::ir::CallHandler, locations: &mut Vec<Location>) {
    match handler {
        super::ir::CallHandler::Branches(arms) => {
            for (_, body) in arms {
                collect_statements(body, locations);
            }
        }
        super::ir::CallHandler::Value(v)
        | super::ir::CallHandler::Return { value: v, .. }
        | super::ir::CallHandler::Fail(Some(v)) => collect(v, locations),
        _ => {}
    }
}
