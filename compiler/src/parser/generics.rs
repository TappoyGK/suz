//! 型引数と操作の要求を、宣言の型スコープで解析する。
use super::{Parser, names};
use crate::{Diagnostic, ErrorKind, ast::generics::TypeParameter, lexer::Kind};

impl Parser<'_, '_> {
    pub(super) fn generic_parameters(&mut self) -> Result<Vec<TypeParameter>, Diagnostic> {
        let parameters = self.delimited_list("[", "]", Self::generic_parameter)?;
        if parameters.is_empty() {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.parameters",
                    "at least one type parameter is required"
                ),
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for p in &parameters {
            if !names.insert(&p.name) {
                return Err(p.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("generic.parameters", "duplicate type parameter {}", p.name),
                ));
            }
        }
        Ok(parameters)
    }

    fn generic_parameter(&mut self) -> Result<TypeParameter, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("generic.parameters", "expected a type parameter name"),
            ));
        };
        names::check(names::Shape::TypeParameter, name, location)?;
        if self.structs.iter().any(|d| d.name == name)
            || self.enums.iter().any(|d| d.name == name)
            || self.fnshape_names.contains_key(name)
            || self.need_groups.names.contains_key(name)
        {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("decl.shadowing", "type parameter shadows a top-level type"),
            ));
        }
        self.cursor += 1;
        Ok(TypeParameter {
            name: name.into(),
            location,
        })
    }
}

impl Parser<'_, '_> {
    pub(super) fn explicit_type_arguments(&self, name: &str) -> bool {
        self.token().kind == Kind::Symbol("[") && self.generic_functions.contains(name)
    }

    pub(super) fn type_argument(&mut self) -> Result<crate::ast::Type, Diagnostic> {
        let location = self.location();
        let ty = self.type_atom()?;
        if ty.leaf().generic() || ty.leaf() == crate::ast::Type::Void {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.type-arguments",
                    "type arguments must be complete value types"
                ),
            ));
        }
        Ok(ty)
    }

    /// 要求・契約は一行に収める。型名は後続の宣言を読んでから、その宣言のスコープで解決する。
    pub(super) fn collect_need(&mut self) -> Result<(), Diagnostic> {
        let start = self.cursor;
        let line = self.token().line;
        let is_contract = matches!(
            self.tokens.get(start + 1).map(|t| t.kind),
            Some(Kind::Ident("expects" | "ensures"))
        );
        self.cursor += 1;
        while !matches!(
            self.token().kind,
            Kind::Newline | Kind::BlankLine | Kind::End
        ) && self.token().line == line
        {
            self.cursor += 1;
        }
        if is_contract {
            self.pending_contracts.push((start, self.cursor));
        } else {
            self.pending_needs.push((start, self.cursor));
        }
        if !self.token().kind.is_newline() {
            return Err(self.error(
                ErrorKind::InvalidSource,
                if is_contract {
                    crate::diagnostic!(
                        "contract.decl",
                        "a contract must occupy one line before its declaration"
                    )
                } else {
                    crate::diagnostic!(
                        "generic.needs",
                        "a requirement must occupy one line before its declaration"
                    )
                },
            ));
        }
        self.cursor += 1;
        while self.token().kind.is_newline() {
            if self.token().kind == Kind::BlankLine {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    if is_contract {
                        crate::diagnostic!(
                            "contract.decl",
                            "a blank line separates the contract from its declaration"
                        )
                    } else {
                        crate::diagnostic!(
                            "generic.needs",
                            "a blank line separates the requirement from its declaration"
                        )
                    },
                ));
            }
            self.cursor += 1;
        }
        if self.token().kind == Kind::End {
            return Err(self.error(
                ErrorKind::InvalidSource,
                if is_contract {
                    crate::diagnostic!("contract.decl", "a contract must precede its declaration")
                } else {
                    crate::diagnostic!(
                        "generic.needs",
                        "a requirement must precede its declaration"
                    )
                },
            ));
        }
        Ok(())
    }

    pub(super) fn declaration_contracts(
        &mut self,
    ) -> Result<Vec<crate::ast::Contract>, Diagnostic> {
        use super::FixedToken;
        use crate::ast::{Contract, ContractKind};
        let saved = self.cursor;
        let mut contracts = Vec::new();
        for (start, end) in std::mem::take(&mut self.pending_contracts) {
            self.cursor = start;
            let location = self.location();
            self.cursor = start + 1;
            let kind = match self.token().kind {
                Kind::Ident("expects") => ContractKind::Expects,
                Kind::Ident("ensures") => ContractKind::Ensures,
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("contract.decl", "expected @expects or @ensures"),
                    ));
                }
            };
            self.cursor += 1;
            self.expect(FixedToken::Symbol("("))?;
            let saved_ensures = self.in_ensures;
            self.in_ensures = true;
            let condition = self.expression();
            self.in_ensures = saved_ensures;
            let condition = condition?;
            self.expect(FixedToken::Symbol(")"))?;
            if self.cursor != end {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("contract.decl", "unexpected tokens in contract"),
                ));
            }
            contracts.push(Contract {
                kind,
                condition,
                location,
            });
        }
        self.cursor = saved;
        Ok(contracts)
    }

    pub(super) fn declaration_needs(
        &mut self,
    ) -> Result<Vec<crate::ast::generics::Need>, Diagnostic> {
        use super::FixedToken;
        use crate::ast::generics::NeedList;
        let saved = self.cursor;
        let mut needs = NeedList::default();
        for (start, end) in std::mem::take(&mut self.pending_needs) {
            self.cursor = start + 1;
            self.expect(FixedToken::Keyword("needs"))?;
            if self.type_parameters.is_empty() {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("generic.needs", "this declaration has no type parameters"),
                ));
            }
            self.need_item(end, &mut needs)?;
        }
        self.cursor = saved;
        Ok(needs.into_vec())
    }
    /// 直接要求は一つの要素として追加し、受け渡し用の一時Vecを作らない。
    pub(super) fn need_item(
        &mut self,
        end: usize,
        needs: &mut crate::ast::generics::NeedList,
    ) -> Result<(), Diagnostic> {
        if self
            .tokens
            .get(self.cursor + 1)
            .is_some_and(|t| t.kind == Kind::Symbol(":"))
        {
            needs.insert(self.direct_need(end)?);
        } else {
            for need in self.named_need(end)? {
                needs.insert(need);
            }
        }
        Ok(())
    }

    fn direct_need(&mut self, end: usize) -> Result<crate::ast::generics::Need, Diagnostic> {
        use super::FixedToken;
        use crate::ast::{
            IntegerType, Type,
            generics::{Need, RequiredOperation},
        };
        let location = self.location();
        let Kind::Ident(target) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("generic.needs", "a requirement targets a type parameter"),
            ));
        };
        let parameter = self
            .type_parameters
            .iter()
            .position(|p| p.name == target)
            .ok_or_else(|| {
                self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.needs",
                        "'{target}' is not a type parameter of this declaration"
                    ),
                )
            })?;
        self.cursor += 1;
        self.expect(FixedToken::Symbol(":"))?;
        let operation = match self.token().kind {
            Kind::Symbol("." | "#") => {
                let mutable = self.token().kind == Kind::Symbol("#");
                self.cursor += 1;
                let Kind::Ident(name) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("generic.needs", "expected a method name"),
                    ));
                };
                names::check(names::Shape::Value, name, self.location())?;
                self.cursor += 1;
                RequiredOperation::Method {
                    name: name.into(),
                    mutable,
                }
            }
            Kind::Symbol(
                op @ ("+" | "-" | "*" | "/" | "%" | "%%" | "+%" | "-%" | "*%" | "+|" | "-|" | "*|"
                | "~" | "&" | "|" | "^" | "<<" | ">>" | ">>>" | "<<%" | ">>%" | ">>>%"
                | "<<|" | "==" | "!=" | "<" | "<=" | ">" | ">="),
            )
            | Kind::Keyword(op @ ("len" | "not")) => {
                self.cursor += 1;
                RequiredOperation::Operator(match op {
                    "len" => "len",
                    "not" => "not",
                    "~" => "~",
                    _ => crate::ast::ComparisonOperator::from_symbol(op)
                        .map(|o| o.symbol())
                        .unwrap_or_else(|| {
                            crate::ast::BinaryOperator::from_symbol(op)
                                .unwrap()
                                .symbol()
                        }),
                })
            }
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("generic.needs", "expected a supported operator or method"),
                ));
            }
        };
        let parameters = self.list(Self::type_atom)?;
        if parameters.contains(&Type::Void) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("type.void", "parameters cannot have type void"),
            ));
        }
        self.expect(FixedToken::Symbol("->"))?;
        let result = self.parse_type()?;
        let errors = self.error_spec()?;
        if self.cursor != end {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("generic.needs", "unexpected tokens in requirement"),
            ));
        }
        if let RequiredOperation::Operator(op) = operation {
            let valid_arity = if matches!(op, "len" | "not" | "~") {
                parameters.is_empty()
            } else if matches!(op, "-" | "-%" | "-|") {
                parameters.len() <= 1
            } else {
                parameters.len() == 1
            };
            let valid_result = if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "not") {
                result == Type::Bool
            } else if op == "len" {
                result == Type::Integer(IntegerType::Len)
            } else {
                result != Type::Void
            };
            if !valid_arity || !valid_result || !errors.is_empty() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("generic.needs", "invalid operator requirement signature"),
                ));
            }
        }
        let need = Need {
            definition: location,
            parameter,
            operation,
            parameters,
            result,
            errors,
            location,
        };
        Ok(need)
    }
}

/// 呼び出しの角括弧と添字を、前方宣言も含めて区別する。宣言自体の検査はfunctionが行う。
pub(super) fn function_names(
    tokens: &[crate::lexer::Token<'_>],
) -> std::collections::BTreeSet<String> {
    tokens
        .windows(3)
        .filter_map(|part| {
            if let [
                crate::lexer::Token {
                    kind: Kind::Keyword("fn"),
                    ..
                },
                crate::lexer::Token {
                    kind: Kind::Ident(name),
                    ..
                },
                crate::lexer::Token {
                    kind: Kind::Symbol("["),
                    ..
                },
            ] = part
            {
                Some((*name).to_owned())
            } else {
                None
            }
        })
        .collect()
}

impl Parser<'_, '_> {
    /// 型適用はASTの表へ保存し、同じ適用には同じ番号を付ける。
    pub(super) fn apply_struct(
        &self,
        declaration: usize,
        arguments: Vec<crate::ast::Type>,
    ) -> crate::ast::Type {
        crate::ast::Type::Application(self.apply_type(declaration, arguments))
    }

    pub(super) fn apply_type(&self, declaration: usize, arguments: Vec<crate::ast::Type>) -> usize {
        let application = crate::ast::generics::Application {
            declaration,
            arguments,
        };
        self.applications.borrow_mut().intern(application)
    }

    pub(super) fn struct_type(
        &mut self,
        declaration: usize,
    ) -> Result<crate::ast::Type, Diagnostic> {
        let count = self.structs[declaration].type_parameters.len();
        if self.token().kind != Kind::Symbol("[") {
            if count == 0 {
                return Ok(crate::ast::Type::Struct(declaration));
            }
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.type-arguments",
                    "generic struct requires explicit type arguments"
                ),
            ));
        }
        let location = self.location();
        let arguments = self.nested(location, |p| {
            p.delimited_list("[", "]", Self::type_argument)
        })?;
        if count == 0 || count != arguments.len() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.type-arguments",
                    "expected {} type arguments, found {}",
                    count,
                    arguments.len()
                ),
            ));
        }
        Ok(self.apply_struct(declaration, arguments))
    }
}
