//! 構文解析中に深さを制限し、左結合で既存の部分木が深くなる場合も検出する。

use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Location, Statement, TemplatePart},
};

pub(crate) const LIMIT: usize = 256;

pub(crate) fn error(location: Location) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!(
            "src.nesting-limit",
            "syntax nesting exceeds the limit of {LIMIT}"
        ),
    )
}

impl Parser<'_, '_> {
    pub(super) fn primary_node(
        &self,
        kind: ExpressionKind,
        location: Location,
    ) -> Result<Expression, Diagnostic> {
        let height = kind_height(&kind);
        if self.depth - 1 + height > LIMIT {
            return Err(error(location));
        }
        Ok(Expression {
            kind,
            location,
            height,
        })
    }
    pub(super) fn nested<T>(
        &mut self,
        location: Location,
        parse: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        if self.depth == LIMIT {
            return Err(error(self.limit_location.unwrap_or(location)));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    pub(super) fn check_height(&self, height: usize, location: Location) -> Result<(), Diagnostic> {
        if self.depth + height > LIMIT {
            Err(error(location))
        } else {
            Ok(())
        }
    }

    pub(super) fn node(
        &self,
        kind: ExpressionKind,
        location: Location,
    ) -> Result<Expression, Diagnostic> {
        let height = kind_height(&kind);
        self.check_height(height, location)?;
        Ok(Expression {
            kind,
            location,
            height,
        })
    }

    // 右辺・初期値の解析中に上限へ達したときも、それを1段深くする演算子を示す。
    pub(super) fn operator_child<T>(
        &mut self,
        location: Location,
        parse: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        let saved = self.limit_location.replace(location);
        let result = self.nested(location, parse);
        self.limit_location = saved;
        result
    }
}

fn handler_height(handler: &Option<FailureHandler>) -> usize {
    match handler {
        Some(
            FailureHandler::Value(value)
            | FailureHandler::Return { value, .. }
            | FailureHandler::Fail {
                value: Some(value), ..
            },
        ) => value.height,
        None | Some(FailureHandler::Fail { value: None, .. } | FailureHandler::Ignore(_)) => 0,
    }
}

pub(super) fn block_height(body: &[Statement]) -> usize {
    1 + body.iter().map(statement_height).max().unwrap_or(0)
}

fn statement_height(statement: &Statement) -> usize {
    match statement {
        Statement::Binding { initializer, .. } => initializer.height,
        Statement::Assign {
            value, compound, ..
        } => value.height - usize::from(*compound),
        Statement::Return(value) | Statement::Fail(value) | Statement::Expression(value) => {
            value.height
        }
        Statement::Log { parts, .. } => {
            1 + parts
                .iter()
                .filter_map(|p| match p {
                    TemplatePart::Value(v) => Some(v.height),
                    _ => None,
                })
                .max()
                .unwrap_or(0)
        }
        Statement::Block(body) => block_height(body),
        Statement::Loop { condition, body } => condition
            .as_ref()
            .map_or(0, |v| v.height)
            .max(block_height(body)),
        Statement::For {
            start, end, body, ..
        } => start
            .height
            .max(end.as_ref().map_or(0, |v| v.height))
            .max(block_height(body)),
        Statement::If {
            branches,
            otherwise,
        } => branches
            .iter()
            .map(|(c, b)| c.height.max(block_height(b)))
            .chain(otherwise.iter().map(|b| block_height(b)))
            .max()
            .unwrap_or(0),
        Statement::Match { target, arms, .. } => target.height.max(
            arms.iter()
                .map(|a| {
                    a.pattern
                        .as_ref()
                        .map_or(0, |v| v.height)
                        .max(block_height(&a.body))
                })
                .max()
                .unwrap_or(0),
        ),
        Statement::Break(_) | Statement::Continue(_) => 0,
    }
}

fn kind_height(kind: &ExpressionKind) -> usize {
    use ExpressionKind::*;
    1 + match kind {
        Slice {
            target,
            start,
            end,
            fallback,
            ..
        } => (target
            .height
            .max(start.as_ref().map_or(0, |v| v.height))
            .max(end.as_ref().map_or(0, |v| v.height))
            + usize::from(fallback.is_some()))
        .max(handler_height(fallback)),
        Template(parts) | Panic(parts) => parts
            .iter()
            .filter_map(|p| match p {
                TemplatePart::Value(v) => Some(v.height),
                _ => None,
            })
            .max()
            .unwrap_or(0),
        Struct { fields, .. } => fields.iter().map(|f| f.value.height).max().unwrap_or(0),
        With { target, fields } => target
            .height
            .max(fields.iter().map(|f| f.value.height).max().unwrap_or(0)),
        Field { target, .. } => target.height,
        Invoke { target, arguments } => target
            .height
            .max(arguments.iter().map(|v| v.height).max().unwrap_or(0)),
        Closure(closure) => block_height(&closure.function.statements),
        FunctionValue(_) => 0,
        Call { arguments, .. } => arguments.iter().map(|v| v.height).max().unwrap_or(0),
        Block { statements, value } => block_height(statements).max(1 + value.height),
        Match { target, arms } => target.height.max(
            arms.iter()
                .map(|a| {
                    a.body
                        .height
                        .max(a.pattern.as_ref().map_or(0, |v| v.height))
                })
                .max()
                .unwrap_or(0),
        ),
        ArithmeticHandler { operand, fallback } => operand.height.max(handler_height(fallback)),
        Cast {
            operand, fallback, ..
        } => (operand.height + usize::from(fallback.is_some())).max(handler_height(fallback)),
        Len(v) | Negate(v) | BitNot(v) | Not(v) | Group(v) => v.height,
        If {
            branches,
            otherwise,
        } => otherwise.height.max(
            branches
                .iter()
                .map(|(c, v)| c.height.max(v.height))
                .max()
                .unwrap_or(0),
        ),
        Binary { left, right, .. } | Logical { left, right, .. } => left.height.max(right.height),
        Comparison { operands, .. } => operands.iter().map(|v| v.height).max().unwrap_or(0),
        Integer { negative: true, .. } => 1,
        Float(value) if value.starts_with('-') => 1,
        Integer { .. } | Float(_) | Bool(_) | Char(_) | Name(_) | Variant { .. } | Void => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depths_follow_source_constructs_including_implicit_value_blocks() {
        for (expression, depth) in [
            ("(1 + 2) * 3", 5),
            ("0 < a < 10", 3),
            ("a + 1 + 1", 4),
            ("`n=${a + 1}`", 4),
            ("ifexpr a == 0 { a } else { 0 }", 4),
            ("expr { 1 }", 4),
            ("run(fn []() -> i32 { return 1 })", 5),
            ("matchexpr a { _ => 1 }", 4),
            ("x as u8 or 0", 4),
            ("s[0] or 'a'", 4),
            ("-1", 3),
        ] {
            let source = format!("fn main() -> u8 {{ fix value = {expression}\nreturn 0 }}");
            let tokens = crate::lexer::lex(&source).unwrap();
            let parsed = crate::parser::parse(&tokens).unwrap();
            assert_eq!(
                block_height(&parsed.functions[0].statements),
                depth,
                "{expression}"
            );
        }
    }
}
