//! クロージャの明示的なキャプチャー一覧と関数本体を解析する。
//! 本体では外側の値ブロックのreturn制限を引き継がず、構文の深さは引き継ぐ。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Capture, Closure, Function, Location},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn closure(&mut self, location: Location) -> Result<Closure, Diagnostic> {
        let captures = self.delimited_list("[", "]", Self::capture)?;
        let parameters = self.list(Self::parameter)?;
        self.expect(FixedToken::Symbol("->"))?;
        let result = self.parse_type()?;
        if result.generic() || parameters.iter().any(|p| p.ty.generic()) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "closure.generic",
                    "closure parameters and result must use concrete types"
                ),
            ));
        }
        let saved_value = self.in_value;
        let saved_constant = self.constant_expression;
        let saved_condition = self.restricted_condition;
        self.in_value = false;
        self.constant_expression = false;
        self.restricted_condition = false;
        let body = self.block();
        self.in_value = saved_value;
        self.constant_expression = saved_constant;
        self.restricted_condition = saved_condition;
        let (statements, end) = body?;
        Ok(Closure {
            captures,
            function: Function {
                name: String::new(),
                keyword_location: location,
                location,
                parameters,
                result,
                statements,
                end,
            },
        })
    }

    fn capture(&mut self) -> Result<Capture, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "closure.capture-list",
                    "expected a local binding name in the capture list"
                ),
            ));
        };
        self.cursor += 1;
        Ok(Capture {
            name: name.into(),
            location,
        })
    }
}
