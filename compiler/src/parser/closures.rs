//! クロージャの明示的なキャプチャー一覧と関数本体を解析する。
//! 本体では外側の値ブロックのreturn制限を引き継がず、構文の深さは引き継ぐ。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Capture, Closure, Function, Location},
    lexer::Kind,
};

impl Parser<'_, '_> {
    // クロージャの構築を単項式の再帰フレームから分離し、深い通常式のスタックを増やさない。
    pub(super) fn closure_node(
        &mut self,
        location: Location,
    ) -> Result<crate::ast::Expression, Diagnostic> {
        let closure = self.closure(location)?;
        self.primary_node(
            crate::ast::ExpressionKind::Closure(Box::new(closure)),
            location,
        )
    }

    pub(super) fn closure(&mut self, location: Location) -> Result<Closure, Diagnostic> {
        let captures = self.delimited_list("[", "]", Self::capture)?;
        let parameters = self.list(Self::parameter)?;
        self.expect(FixedToken::Symbol("->"))?;
        let result = self.parse_type()?;
        let errors = self.error_spec()?;
        if result.leaf().generic() || parameters.iter().any(|p| p.ty.leaf().generic()) {
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
                type_parameters: Vec::new(),
                needs: Vec::new(),
                contracts: Vec::new(),
                method: None,
                name: String::new(),
                keyword_location: location,
                location,
                parameters,
                result,
                errors,
                statements,
                end,
            },
        })
    }

    fn capture(&mut self) -> Result<Capture, Diagnostic> {
        let location = self.location();
        let by_reference = self.token().kind == Kind::Keyword("var");
        if by_reference {
            self.cursor += 1;
        }
        let (Kind::Ident(name) | Kind::Keyword(name @ "self")) = self.token().kind else {
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
            by_reference,
            name: name.into(),
            location,
        })
    }
}
