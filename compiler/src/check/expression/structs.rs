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
            let indices = initializer_indices(declaration, fields, expr.location)?;
            let values = fields
                .iter()
                .zip(indices)
                .map(|(field, index)| {
                    Ok((
                        index,
                        super::coerced(&field.value, Some(declaration.fields[index].ty), scope)?,
                    ))
                })
                .collect::<Result<_, Diagnostic>>()?;
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

// Field membership and completeness do not depend on numeric substitutions.
pub(crate) fn initializer_indices(
    declaration: &crate::ast::StructDeclaration,
    fields: &[crate::ast::FieldInitializer],
    location: crate::ast::Location,
) -> Result<Vec<usize>, Diagnostic> {
    let mut seen = BTreeSet::new();
    let mut indices = Vec::new();
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
        indices.push(index);
    }
    if seen.len() != declaration.fields.len() {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "struct literal must initialize every field [struct.literal]",
        ));
    }
    Ok(indices)
}
