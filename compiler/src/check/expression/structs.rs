use super::expression;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
    check::{
        ir::{CheckedExpression, Value},
        scope::Scope,
    },
};
use std::collections::BTreeSet;

pub(super) fn check(expr: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Struct { id, fields } => {
            let declaration = &scope.structs[*id];
            let mut seen = BTreeSet::new();
            let mut values = Vec::new();
            for field in fields {
                let index = declaration
                    .fields
                    .iter()
                    .position(|f| f.name == field.name)
                    .ok_or_else(|| {
                        field.location.error(
                            ErrorKind::InvalidSource,
                            "unknown field in struct literal [struct.literal]",
                        )
                    })?;
                if !seen.insert(index) {
                    return Err(field.location.error(
                        ErrorKind::InvalidSource,
                        "duplicate field initializer [struct.literal]",
                    ));
                }
                values.push((
                    index,
                    super::coerced(&field.value, Some(declaration.fields[index].ty), scope)?,
                ));
            }
            if seen.len() != declaration.fields.len() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "struct literal must initialize every field [struct.literal]",
                ));
            }
            Ok(CheckedExpression {
                ty: Type::Struct(*id),
                value: Value::Struct { fields: values },
            })
        }
        ExpressionKind::Field { target, name } => {
            let target = expression(target, None, scope)?;
            let (index, ty) = field(target.ty, name, expr.location, scope)?;
            Ok(CheckedExpression {
                ty,
                value: Value::Field {
                    target: Box::new(target),
                    index,
                },
            })
        }
        _ => unreachable!(),
    }
}

pub(crate) fn field(
    ty: Type,
    name: &str,
    location: crate::ast::Location,
    scope: &Scope,
) -> Result<(usize, Type), Diagnostic> {
    let Type::Struct(id) = ty else {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "field access requires a struct value [struct.access]",
        ));
    };
    scope.structs[id]
        .fields
        .iter()
        .enumerate()
        .find(|(_, f)| f.name == name)
        .map(|(index, field)| (index, field.ty))
        .ok_or_else(|| {
            location.error(
                ErrorKind::InvalidSource,
                "unknown struct field [struct.access]",
            )
        })
}
