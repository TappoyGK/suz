//! 配列型とnewの初期化を解析する。列挙の形をここで揃え、葉の評価順を保つ。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn array_type(&mut self) -> Result<Type, Diagnostic> {
        self.array_type_dimensions(false).map(|(ty, _)| ty)
    }

    pub(super) fn array_type_dimensions(
        &mut self,
        names: bool,
    ) -> Result<(Type, Vec<crate::ast::arrays::Dimension>), Diagnostic> {
        let location = self.location();
        self.nested(location, |parser| {
            parser.expect(FixedToken::Symbol("["))?;
            let element = parser.parse_type()?;
            if element.leaf().generic() {
                return Err(location.error(ErrorKind::InvalidSource, crate::diagnostic!("num.places", "array element types require concrete types or named type parameters; use T instead of int or float")));
            }
            if element == Type::Void {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.type", "array elements require a stored value type"),
                ));
            }
            let mut dimensions = Vec::new();
            let mut rank = 1;
            if parser.token().kind == Kind::Symbol(";") {
                parser.cursor += 1;
                rank = 0;
                loop {
                    match parser.token().kind {
                        Kind::Wildcard => {},
                        Kind::Ident(name) if names => {
                            let location = parser.location();
                            super::names::check(super::names::Shape::Value, name, location)?;
                            dimensions.push(crate::ast::arrays::Dimension { name: name.into(), index: rank, location });
                        }
                        _ => return Err(parser.error(ErrorKind::InvalidSource, crate::diagnostic!(
                            "array.dimension-bind", "dimension names require a nonnull array parameter or fix ptr annotation; use _ here"
                        ))),
                    }
                    parser.cursor += 1;
                    rank += 1;
                    let comma = parser.token().kind == Kind::Symbol(",");
                    if comma {
                        parser.cursor += 1;
                    }
                    if parser.token().kind == Kind::Symbol("]") {
                        if (location.line == parser.location().line) == comma {
                            return Err(parser.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "src.trailing-comma",
                                    "invalid trailing comma for array dimensions"
                                ),
                            ));
                        }
                        break;
                    }
                    if !comma {
                        return Err(parser.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("src.list-comma", "array dimensions require commas"),
                        ));
                    }
                }
            }
            parser.expect(FixedToken::Symbol("]"))?;
            let ty = parser.applications.borrow_mut().arrays.intern(element, rank);
            Ok((ty, dimensions))
        })
    }

    pub(super) fn array_initializer(&mut self) -> Result<Expression, Diagnostic> {
        let location = self.location();
        self.nested(location, |parser| {
            let open = parser.location();
            parser.expect(FixedToken::Symbol("["))?;
            let mut values = Vec::new();
            let mut lengths = Vec::new();
            let mut repeated = false;
            if parser.token().kind != Kind::Symbol("]") {
                values.push(parser.array_item()?);
                if parser.token().kind == Kind::Symbol(";") {
                    if matches!(values[0].kind, ExpressionKind::ArrayInit { .. }) {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "array.enumeration",
                                "repeated initializer must be a value"
                            ),
                        ));
                    }
                    repeated = true;
                    parser.cursor += 1;
                    lengths.push(parser.expression()?);
                }
                loop {
                    let comma = parser.token().kind == Kind::Symbol(",");
                    if comma {
                        parser.cursor += 1;
                    }
                    if parser.token().kind == Kind::Symbol("]") {
                        if (open.line == parser.location().line) == comma {
                            return Err(parser.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "src.trailing-comma",
                                    "invalid trailing comma for array layout"
                                ),
                            ));
                        }
                        break;
                    }
                    if !comma {
                        return Err(parser.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("src.list-comma", "array items require commas"),
                        ));
                    }
                    if repeated {
                        lengths.push(parser.expression()?);
                    } else {
                        values.push(parser.array_item()?);
                    }
                }
            }
            parser.expect(FixedToken::Symbol("]"))?;
            // 列挙を平坦化しても、元の角括弧の深さは後続の構文検査へ残す。
            let height = 1 + values
                .iter()
                .chain(&lengths)
                .map(|v| v.height)
                .max()
                .unwrap_or(0);
            parser.check_height(height, location)?;
            if !repeated {
                let count = values.len();
                let mut flattened = Vec::new();
                let mut shape: Option<Vec<Expression>> = None;
                let nested = values
                    .first()
                    .is_some_and(|v| matches!(v.kind, ExpressionKind::ArrayInit { .. }));
                for value in values {
                    match value.kind {
                        ExpressionKind::ArrayInit {
                            values,
                            lengths,
                            repeated: false,
                        } if nested && !values.is_empty() => {
                            if let Some(previous) = &shape {
                                if previous
                                    .iter()
                                    .map(dimension)
                                    .ne(lengths.iter().map(dimension))
                                {
                                    return Err(location.error(
                                        ErrorKind::InvalidSource,
                                        crate::diagnostic!(
                                            "array.enumeration",
                                            "array rows must have equal shapes"
                                        ),
                                    ));
                                }
                            } else {
                                shape = Some(lengths);
                            }
                            flattened.extend(values);
                        }
                        ExpressionKind::ArrayInit { .. } => {
                            return Err(location.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "array.enumeration",
                                    "array rows must be nonempty rectangular enumerations"
                                ),
                            ));
                        }
                        _ if nested => {
                            return Err(location.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "array.enumeration",
                                    "cannot mix rows and values"
                                ),
                            ));
                        }
                        _ => flattened.push(value),
                    }
                }
                values = flattened;
                lengths.push(parser.node(
                    ExpressionKind::Integer {
                        digits: count.to_string(),
                        negative: false,
                    },
                    location,
                )?);
                lengths.extend(shape.unwrap_or_default());
            }
            Ok(Expression {
                kind: ExpressionKind::ArrayInit {
                    values,
                    lengths,
                    repeated,
                },
                location,
                height,
            })
        })
    }
    fn array_item(&mut self) -> Result<Expression, Diagnostic> {
        if self.token().kind == Kind::Symbol("[") {
            self.array_initializer()
        } else {
            self.expression()
        }
    }
}
fn dimension(expr: &Expression) -> &str {
    let ExpressionKind::Integer { digits, .. } = &expr.kind else {
        unreachable!()
    };
    digits
}
