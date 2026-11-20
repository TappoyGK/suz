//! externブロックの関数宣言を解析し、先に収集したC構造体宣言を読み進める。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{ExternFunction, ExternParameter, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
    fn extern_parameter(&mut self) -> Result<ExternParameter, Diagnostic> {
        let modifier_location = self.location();
        // constと同じく文脈上の修飾語。keep: 型という従来の仮引数名は予約しない。
        let keep = self.token().kind == Kind::Ident("keep")
            && self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|t| matches!(t.kind, Kind::Ident(_)));
        if keep {
            self.cursor += 1;
        }
        let (name, location) = self.parameter_name()?;
        let mut read_only = false;
        let ty = if self.token().kind == Kind::Symbol("*") {
            self.cursor += 1;
            if self.token().kind == Kind::Ident("const") {
                read_only = true;
                self.cursor += 1;
            }
            let target = self.parse_type()?;
            if read_only && target == Type::Void {
                Type::CPointer // 文字列のバイト列を渡す既存の契約を維持する。
            } else if numeric_type(target)
                || matches!(target, Type::Struct(id) if self.structs[id].external.is_some())
            {
                target.pointer()
            } else {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "ffi.pointer-type",
                        "C pointer parameters require a supported numeric type or an extern struct"
                    ),
                ));
            }
        } else {
            let ty = self.function_parameter_type()?;
            if ty.pointee().is_some() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "ffi.pointer-type",
                        "use *const T or *T for an extern pointer parameter, not ptr T"
                    ),
                ));
            }
            if ty == Type::Void {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("type.void", "parameters cannot have type void"),
                ));
            }
            ty
        };
        if keep && ty.pointee().is_none() {
            return Err(modifier_location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "ffi.keep",
                    "keep requires a typed C pointer parameter (*const T or *T)"
                ),
            ));
        }
        // keepはCが保持できる期間の契約だけを宣言する。型・ABI・寿命検査は
        // 通常のポインタ引数と同じなので、後段へ実行用の情報を追加しない。
        Ok(ExternParameter {
            name,
            ty,
            location,
            read_only,
        })
    }

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
            if self.token().kind == Kind::Keyword("struct") {
                self.cursor = self
                    .structs
                    .iter()
                    .find(|s| s.start == self.cursor)
                    .expect("collected extern struct")
                    .end;
                if !matches!(
                    self.token().kind,
                    Kind::Newline | Kind::BlankLine | Kind::Symbol("}")
                ) {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "ffi.decl",
                            "extern declarations must be separated by a newline"
                        ),
                    ));
                }
                self.newlines();
                continue;
            }
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
            let parameters = self.list(Self::extern_parameter)?;
            self.expect(FixedToken::Symbol("->"))?;
            let result = self.parse_type()?;
            let supported = |ty| numeric_type(ty) || ty == Type::Bool;
            if !parameters
                .iter()
                .all(|p| p.ty == Type::CPointer || p.ty.pointee().is_some() || supported(p.ty))
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
            if !matches!(
                self.token().kind,
                Kind::Newline | Kind::BlankLine | Kind::Symbol("}")
            ) {
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

/// extern関数とC構造体で共通の数値型。len_tやchar、汎用型はC ABIの型ではない。
pub(super) fn numeric_type(ty: Type) -> bool {
    matches!(
        ty,
        Type::CInteger(_)
            | Type::Float(_)
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
    )
}
