//! 寸法名の束縛と、同じ仮引数リスト内の一致条件を本体の入口へ出力する。
use crate::check::{
    ir::{CheckedExpression, CheckedStatement, Value},
    scope::Scope,
    statements::Binding,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{IntegerType, Type, arrays::Dimension},
};
use std::collections::BTreeMap;

#[allow(clippy::too_many_arguments)]
pub(in crate::check) fn bind(
    names: &[Dimension],
    array: usize,
    ty: Type,
    scope: &mut Scope,
    next_id: &mut usize,
    declared: &mut Vec<String>,
    output: &mut Vec<CheckedStatement>,
    mut shared: Option<&mut BTreeMap<String, usize>>,
) -> Result<(), Diagnostic> {
    for dimension in names {
        let length = CheckedExpression {
            ty: Type::Integer(IntegerType::Len),
            value: Value::ArrayLength {
                target: Box::new(CheckedExpression {
                    ty,
                    value: Value::Local(array),
                }),
                dimension: dimension.index,
            },
        };
        if let Some(id) = shared
            .as_ref()
            .and_then(|m| m.get(&dimension.name))
            .copied()
        {
            let canonical = scope.array_bounds.names[&id];
            scope.array_bounds.arrays.get_mut(&array).unwrap()[dimension.index] = Some(canonical);
            output.push(CheckedStatement::Expression(CheckedExpression {
                ty: Type::Void,
                value: Value::ArrayDimensionGuard {
                    name: dimension.name.clone(),
                    left: Box::new(CheckedExpression {
                        ty: length.ty,
                        value: Value::Local(id),
                    }),
                    right: Box::new(length),
                    location: dimension.location,
                },
            }));
            continue;
        }
        if scope.contains_key(&dimension.name)
            || scope.functions.contains_key(&dimension.name)
            || scope.is_type_name(&dimension.name)
        {
            return Err(dimension.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "decl.duplicate",
                    "dimension name conflicts with an existing name"
                ),
            ));
        }
        let fact = scope.array_bounds.arrays[&array][dimension.index].unwrap();
        let id = *next_id;
        scope.array_bounds.names.insert(id, fact);
        *next_id += 1;
        scope.locals.insert(
            dimension.name.clone(),
            Binding::local(id, length.ty, false, false, dimension.location),
        );
        declared.push(dimension.name.clone());
        if let Some(shared) = &mut shared {
            shared.insert(dimension.name.clone(), id);
        }
        output.push(CheckedStatement::Binding {
            location: dimension.location,
            mutable: false,
            id,
            ty: length.ty,
            initializer: Box::new(length),
        });
    }
    Ok(())
}
