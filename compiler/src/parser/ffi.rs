//! Cヘッダーを指定した、ファイル内の外部関数宣言を解析する。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{ExternFunction, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn extern_block(&mut self) -> Result<Vec<ExternFunction>, Diagnostic> {
        self.cursor += 1;
        let Kind::Header(header) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("ffi.header", "extern requires a quoted C header name"),
            ));
        };
        let header = header.to_owned();
        self.cursor += 1;
        self.expect(FixedToken::Symbol("{"))?;
        self.newlines();
        let mut functions = Vec::new();
        while self.token().kind != Kind::Symbol("}") {
            self.expect(FixedToken::Keyword("fn"))?;
            let location = self.location();
            let c_name = match self.token().kind {
                Kind::Ident(n) | Kind::Keyword(n) | Kind::TypeName(n) => n.to_owned(),
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("ffi.decl", "expected a C function name"),
                    ));
                }
            };
            if c_name.starts_with("suz_") {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.name", "C names starting with suz_ are reserved"),
                ));
            }
            self.cursor += 1;
            let name = if self.token().kind == Kind::Keyword("as") {
                self.cursor += 1;
                let Kind::Ident(n) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("ffi.name", "expected a Suz alias"),
                    ));
                };
                self.cursor += 1;
                n.to_owned()
            } else {
                c_name.clone()
            };
            super::names::check(super::names::Shape::Value, &name, location)?;
            // 予約語と型名をSuz側の呼び出し名にしない。
            if name == "main" || crate::lexer::reserved(&name) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.name", "C function requires a non-reserved Suz alias"),
                ));
            }
            let parameters = self.list(Self::parameter)?;
            self.expect(FixedToken::Symbol("->"))?;
            let result = self.parse_type()?;
            let supported = |ty| {
                matches!(
                    ty,
                    Type::CInteger(_)
                        | Type::Integer(
                            crate::ast::IntegerType::I8
                                | crate::ast::IntegerType::I16
                                | crate::ast::IntegerType::I32
                                | crate::ast::IntegerType::I64
                                | crate::ast::IntegerType::U8
                                | crate::ast::IntegerType::U16
                                | crate::ast::IntegerType::U32
                                | crate::ast::IntegerType::U64
                        )
                        | Type::Float(_)
                        | Type::Bool
                )
            };
            if !parameters
                .iter()
                .all(|p| p.ty == Type::CPointer || supported(p.ty))
                || !(result == Type::Void || supported(result))
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.type", "unsupported C interface type"),
                ));
            }
            let mut names = std::collections::BTreeSet::new();
            if !parameters.iter().all(|p| names.insert(&p.name)) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("fn.parameter-scope", "duplicate parameter name"),
                ));
            }
            if !matches!(self.token().kind, Kind::Newline | Kind::Symbol("}")) {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "ffi.decl",
                        "extern signatures have no body or error specification"
                    ),
                ));
            }
            functions.push(ExternFunction {
                header: header.clone(),
                c_name,
                name,
                parameters,
                result,
                location,
            });
            self.newlines();
        }
        self.cursor += 1;
        Ok(functions)
    }
}
