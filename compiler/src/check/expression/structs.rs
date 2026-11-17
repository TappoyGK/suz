//! 構造体リテラルのフィールドの対応と完全性を検査し、フィールド名を番号へ解決する。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[struct.access]`、`[struct.literal]`。

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
            crate::diagnostic!("struct.access", "field access requires a struct value"),
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
                crate::diagnostic!("struct.access", "unknown struct field"),
            )
        })
}

// フィールドの所属と完全性は、数値型の代入に依存しない。
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
                    crate::diagnostic!("struct.literal", "unknown field in struct literal"),
                )
            })?;
        if !seen.insert(index) {
            return Err(field.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.literal", "duplicate field initializer"),
            ));
        }
        indices.push(index);
    }
    if seen.len() != declaration.fields.len() {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "struct.literal",
                "struct literal must initialize every field"
            ),
        ));
    }
    Ok(indices)
}
