//! ASTの式と文の子を列挙し、構文に共通する走査を提供する。
//!
//! 利用元: [`crate::check`] の制御・汎用型検査。

use super::{Expression, ExpressionKind, FailureHandler, Statement, TemplatePart};
use crate::Diagnostic;

pub(crate) trait Visitor {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        walk_statement(self, statement)
    }
    fn scope(
        &mut self,
        statements: &[Statement],
        value: Option<&Expression>,
    ) -> Result<(), Diagnostic> {
        for statement in statements {
            self.statement(statement)?;
        }
        if let Some(value) = value {
            self.expression(value)?;
        }
        Ok(())
    }
}

pub(crate) fn walk_expression<V: Visitor + ?Sized>(
    visitor: &mut V,
    expr: &Expression,
) -> Result<(), Diagnostic> {
    match &expr.kind {
        ExpressionKind::Slice {
            target,
            start,
            end,
            fallback,
            ..
        } => {
            visitor.expression(target)?;
            for bound in [start, end].into_iter().flatten() {
                visitor.expression(bound)?;
            }
            if let Some(FailureHandler::Value(value) | FailureHandler::Return { value, .. }) =
                fallback
            {
                visitor.expression(value)?;
            }
        }
        ExpressionKind::Template(parts) => {
            for part in parts {
                if let TemplatePart::Value(value) = part {
                    visitor.expression(value)?;
                }
            }
        }
        ExpressionKind::With { target, fields } => {
            visitor.expression(target)?;
            for field in fields {
                visitor.expression(&field.value)?;
            }
        }
        ExpressionKind::Struct { fields, .. } => {
            for field in fields {
                visitor.expression(&field.value)?;
            }
        }
        ExpressionKind::Field { target, .. } => visitor.expression(target)?,
        ExpressionKind::Call { arguments, .. } => {
            for argument in arguments {
                visitor.expression(argument)?;
            }
        }
        ExpressionKind::Void | ExpressionKind::Variant { .. } => {}
        ExpressionKind::Block { statements, value } => visitor.scope(statements, Some(value))?,
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            for (condition, value) in branches {
                visitor.expression(condition)?;
                visitor.expression(value)?;
            }
            visitor.expression(otherwise)?;
        }
        ExpressionKind::Match { target, arms } => {
            visitor.expression(target)?;
            for arm in arms {
                if let Some(pattern) = &arm.pattern {
                    visitor.expression(pattern)?;
                }
                visitor.expression(&arm.body)?;
            }
        }
        ExpressionKind::ArithmeticHandler { operand, fallback }
        | ExpressionKind::Cast {
            operand, fallback, ..
        } => {
            visitor.expression(operand)?;
            if let Some(FailureHandler::Value(value) | FailureHandler::Return { value, .. }) =
                fallback
            {
                visitor.expression(value)?;
            }
        }
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::Len(inner)
        | ExpressionKind::Not(inner)
        | ExpressionKind::BitNot(inner) => visitor.expression(inner)?,
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Logical { left, right, .. } => {
            visitor.expression(left)?;
            visitor.expression(right)?;
        }
        ExpressionKind::Comparison { operands, .. } => {
            for operand in operands {
                visitor.expression(operand)?;
            }
        }
        ExpressionKind::Name(_)
        | ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Bool(_) => {}
    }
    Ok(())
}

pub(crate) fn walk_statement<V: Visitor + ?Sized>(
    visitor: &mut V,
    statement: &Statement,
) -> Result<(), Diagnostic> {
    match statement {
        Statement::Binding { initializer, .. } => visitor.expression(initializer)?,
        Statement::Assign { value, .. }
        | Statement::Return(value)
        | Statement::Expression(value) => visitor.expression(value)?,
        Statement::Log { parts, .. } => {
            for part in parts {
                if let TemplatePart::Value(value) = part {
                    visitor.expression(value)?;
                }
            }
        }
        Statement::Block(body) => visitor.scope(body, None)?,
        Statement::Loop { condition, body } => {
            if let Some(condition) = condition {
                visitor.expression(condition)?;
            }
            visitor.scope(body, None)?;
        }
        Statement::For {
            start, end, body, ..
        } => {
            visitor.expression(start)?;
            if let Some(end) = end {
                visitor.expression(end)?;
            }
            visitor.scope(body, None)?;
        }
        Statement::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                visitor.expression(condition)?;
                visitor.scope(body, None)?;
            }
            if let Some(body) = otherwise {
                visitor.scope(body, None)?;
            }
        }
        Statement::Match { target, arms, .. } => {
            visitor.expression(target)?;
            for arm in arms {
                if let Some(pattern) = &arm.pattern {
                    visitor.expression(pattern)?;
                }
                visitor.scope(&arm.body, None)?;
            }
        }
        Statement::Break(_) | Statement::Continue(_) => {}
    }
    Ok(())
}
