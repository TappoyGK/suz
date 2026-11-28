//! 配列型とnewの初期化を解析する。列挙の形をここで揃え、葉の評価順を保つ。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn array_type(&mut self, soa: bool) -> Result<Type, Diagnostic> {
        self.array_type_dimensions(false, soa).map(|(ty, _)| ty)
    }

    pub(super) fn array_type_dimensions(
        &mut self,
        names: bool,
        soa: bool,
    ) -> Result<(Type, Vec<crate::ast::arrays::Dimension>), Diagnostic> {
        let location = self.location();
        let mutable = match self.token().kind {
            Kind::Symbol("#") => true,
            Kind::Symbol(".") => false,
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.type", "expected .[T] or #[T] array type"),
                ));
            }
        };
        self.cursor += 1;
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
            if soa
                && !matches!(
                    element,
                    Type::Struct(_) | Type::Application(_) | Type::Parameter(_)
                )
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.soa-type", "soa array elements must be structs"),
                ));
            }
            let mut dimensions = Vec::new();
            let mut dims = vec![None];
            if parser.token().kind == Kind::Symbol(";") {
                parser.cursor += 1;
                dims.clear();
                loop {
                    match parser.token().kind {
                        Kind::Wildcard => {
                            dims.push(None);
                        }
                        Kind::Ident(name) if super::names::is_type_spelling(name) => {
                            let Some(decl) = parser
                                .enums
                                .iter()
                                .find(|d| d.name == name && !d.ty.is_error)
                            else {
                                return Err(parser.error(
                                    ErrorKind::InvalidSource,
                                    crate::diagnostic!(
                                        "array.enum-dimension",
                                        "unknown enum type '{name}' in array dimension"
                                    ),
                                ));
                            };
                            dims.push(Some(decl.ty));
                        }
                        Kind::Ident(name) if names => {
                            let location = parser.location();
                            super::names::check(super::names::Shape::Value, name, location)?;
                            dimensions.push(crate::ast::arrays::Dimension {
                                name: name.into(),
                                index: dims.len(),
                                location,
                            });
                            dims.push(None);
                        }
                        _ => return Err(parser.error(ErrorKind::InvalidSource, crate::diagnostic!(
                            "array.dimension-bind", "dimension names require a non-nil array parameter or fix annotation; use _ here"
                        ))),
                    }
                    parser.cursor += 1;
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
            let nilable = parser.token().kind == Kind::Symbol("?");
            if nilable {
                if !dimensions.is_empty() {
                    return Err(parser.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "array.dimension-bind",
                            "dimension names require a non-nil array type; use _ here"
                        ),
                    ));
                }
                parser.cursor += 1;
            }
            let Type::Array(id) = parser
                .applications
                .borrow_mut()
                .arrays
                .intern_dims(element, dims, soa)
            else {
                unreachable!()
            };
            Ok((
                Type::ArrayRef {
                    id,
                    mutable,
                    nilable,
                },
                dimensions,
            ))
        })
    }

    pub(super) fn array_initializer(
        &mut self,
        readonly: bool,
        soa: bool,
    ) -> Result<Expression, Diagnostic> {
        self.array_initializer_inner(readonly, true, soa)
    }

    fn array_initializer_inner(
        &mut self,
        readonly: bool,
        is_top_level: bool,
        soa: bool,
    ) -> Result<Expression, Diagnostic> {
        let location = self.location();
        self.nested(location, |parser| {
            let open = parser.location();
            parser.expect(FixedToken::Symbol("["))?;
            let mut items = Vec::new();
            let mut lengths = Vec::new();
            let mut keys = Vec::new();
            let mut repeated = false;
            let mut keyed = false;
            if parser.token().kind != Kind::Symbol("]") {
                items.push(parser.array_item()?);
                if parser.token().kind == Kind::Symbol(":") {
                    if items[0].1 {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "array.enum-init",
                                "enum array key must be an enum variant"
                            ),
                        ));
                    }
                    keyed = true;
                    let key = items.pop().unwrap().0;
                    parser.cursor += 1;
                    let value = parser.array_item()?;
                    keys.push(key);
                    items.push(value);
                } else if parser.token().kind == Kind::Symbol(";") {
                    if !is_top_level {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "array.enumeration",
                                "nested array rows cannot use repeated syntax"
                            ),
                        ));
                    }
                    if items[0].1 {
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
                    } else if keyed {
                        let key = parser.expression()?;
                        if parser.token().kind != Kind::Symbol(":") {
                            return Err(parser.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "array.enum-init",
                                    "expected ':' after enum array key"
                                ),
                            ));
                        }
                        parser.cursor += 1;
                        let value = parser.array_item()?;
                        keys.push(key);
                        items.push(value);
                    } else {
                        items.push(parser.array_item()?);
                        if parser.token().kind == Kind::Symbol(":") {
                            return Err(parser.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "array.enum-init",
                                    "cannot mix keyed and unkeyed array items"
                                ),
                            ));
                        }
                    }
                }
            }
            parser.expect(FixedToken::Symbol("]"))?;
            // 列挙を平坦化しても、元の角括弧の深さは後続の構文検査へ残す。
            let height = 1 + items
                .iter()
                .map(|(v, _)| v)
                .chain(&lengths)
                .chain(&keys)
                .map(|v| v.height)
                .max()
                .unwrap_or(0);
            parser.check_height(height, location)?;
            let mut key_groups = Vec::new();
            let values = if !repeated {
                let count = items.len();
                let mut flattened = Vec::new();
                let mut shape: Option<Vec<Expression>> = None;
                let mut child_key_groups: Option<Vec<Option<Vec<Vec<Expression>>>>> = None;
                let nested = items.first().is_some_and(|(_, is_row)| *is_row);
                for (value, is_row) in items {
                    if is_row {
                        if !nested {
                            return Err(location.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "array.enumeration",
                                    "cannot mix rows and values"
                                ),
                            ));
                        }
                        match value.kind {
                            ExpressionKind::ArrayInit {
                                values,
                                lengths,
                                key_groups: sub_keys,
                                repeated: false,
                                ..
                            } if !values.is_empty() => {
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
                                    let prev_keys = child_key_groups.as_mut().unwrap();
                                    for (slot, sub) in prev_keys.iter_mut().zip(sub_keys) {
                                        match (slot, sub) {
                                            (Some(acc), Some(more)) => acc.extend(more),
                                            (None, None) => {}
                                            _ => {
                                                return Err(location.error(
                                                    ErrorKind::InvalidSource,
                                                    crate::diagnostic!(
                                                        "array.enumeration",
                                                        "cannot mix keyed and unkeyed rows at the same depth"
                                                    ),
                                                ));
                                            }
                                        }
                                    }
                                } else {
                                    shape = Some(lengths);
                                    child_key_groups = Some(sub_keys);
                                }
                                flattened.extend(values);
                            }
                            _ => {
                                return Err(location.error(
                                    ErrorKind::InvalidSource,
                                    crate::diagnostic!(
                                        "array.enumeration",
                                        "array rows must be nonempty rectangular enumerations"
                                    ),
                                ));
                            }
                        }
                    } else if nested {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("array.enumeration", "cannot mix rows and values"),
                        ));
                    } else {
                        flattened.push(value);
                    }
                }
                lengths.push(parser.node(
                    ExpressionKind::Integer {
                        digits: count.to_string(),
                        negative: false,
                    },
                    location,
                )?);
                lengths.extend(shape.unwrap_or_default());
                key_groups.push(keyed.then_some(vec![keys]));
                key_groups.extend(child_key_groups.unwrap_or_default());
                flattened
            } else {
                items.into_iter().map(|(v, _)| v).collect()
            };
            Ok(Expression {
                kind: ExpressionKind::ArrayInit {
                    values,
                    lengths,
                    key_groups,
                    repeated,
                    readonly,
                    soa,
                },
                location,
                height,
            })
        })
    }
    fn array_item(&mut self) -> Result<(Expression, bool), Diagnostic> {
        if self.token().kind == Kind::Symbol("[") {
            Ok((self.array_initializer_inner(false, false, false)?, true))
        } else {
            Ok((self.expression()?, false))
        }
    }
}
fn dimension(expr: &Expression) -> &str {
    let ExpressionKind::Integer { digits, .. } = &expr.kind else {
        unreachable!()
    };
    digits
}
