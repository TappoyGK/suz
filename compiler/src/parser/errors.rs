//! エラー分岐のパターンと腕を解析する。腕は値ブロックではなく、独自のrecover先を持つ。
use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{ErrorArmBody, ExpressionKind, FailureHandler, Location},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn ignore_reason(&mut self) -> Result<Location, Diagnostic> {
        let location = self.location();
        self.cursor += 1;
        if !matches!(self.token().kind, Kind::IgnoreReason(_)) {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("err.or-ignore", "ignore requires ': reason'"),
            ));
        }
        self.cursor += 1;
        Ok(location)
    }

    pub(super) fn error_handler(&mut self) -> Result<FailureHandler, Diagnostic> {
        let location = self.location();
        if self.constant_expression {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.expression",
                    "error handlers are not constant expressions"
                ),
            ));
        }
        self.cursor += 1;
        let arms = self.match_arms(Self::error_arm)?;
        for arm in &arms {
            if let Some(pattern) = &arm.pattern
                && !matches!(pattern.kind, ExpressionKind::Variant { .. })
            {
                return Err(pattern.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("err.or-error", "error patterns must be variants or _"),
                ));
            }
        }
        Ok(FailureHandler::Errors(Box::new(crate::ast::ErrorHandler {
            arms,
            location,
        })))
    }

    fn error_arm(&mut self) -> Result<ErrorArmBody, Diagnostic> {
        let saved = self.in_value;
        // 外側の値式から脱出できるかは制御検査で判定する。腕内部の通常の値ブロックは従来どおり制限する。
        self.in_value = false;
        let result = if self.token().kind == Kind::Symbol("{") {
            self.block()
                .map(|(body, _)| ErrorArmBody::Statements { body, block: true })
        } else if matches!(
            self.token().kind,
            Kind::Keyword("return" | "fail" | "break" | "continue" | "panic" | "ignore")
        ) {
            self.nested(self.location(), Self::statement)
                .map(|statement| ErrorArmBody::Statements {
                    body: vec![statement],
                    block: false,
                })
        } else {
            self.nested(self.location(), Self::expression)
                .map(|value| ErrorArmBody::Value(Box::new(value)))
        };
        self.in_value = saved;
        result
    }
}
