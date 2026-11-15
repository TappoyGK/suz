use super::*;

// A failed statement retains only binding types for subsequent template checks.
// With no annotation or usable hint, i64 is the ordinary integer default. This
// recovery state is never used to emit a function instance.
pub(super) fn recover(
    statement: &Statement,
    bindings: &mut Scope,
    next_id: &mut usize,
    has_return: &mut bool,
    loop_depth: usize,
    declared: &mut Vec<String>,
) -> Result<(), Diagnostic> {
    // A bad condition or pattern must not hide independent errors in
    // its bodies. Use the same block checker, including scope cleanup.
    match statement {
        Statement::For {
            name,
            start,
            end,
            body,
            location,
            ..
        } if !bindings.contains_key(name) => {
            let ty = if end.is_none() {
                Type::Char
            } else {
                crate::check::expression::operand_type(start, bindings)
                    .or_else(|| {
                        end.as_ref()
                            .and_then(|e| crate::check::expression::operand_type(e, bindings))
                    })
                    .unwrap_or(Type::Integer(crate::ast::IntegerType::I64))
            };
            let generic_dependent = crate::check::expression::generic_dependent(start, bindings)
                || end
                    .as_ref()
                    .is_some_and(|e| crate::check::expression::generic_dependent(e, bindings));
            let id = *next_id;
            *next_id += 1;
            bindings.insert(
                name.clone(),
                Binding::local(id, ty, generic_dependent, false, true, *location),
            );
            block_with_bindings(
                body,
                bindings,
                next_id,
                has_return,
                loop_depth + 1,
                vec![name.clone()],
            )?;
        }
        Statement::Match { arms, .. } => {
            for arm in arms {
                block(&arm.body, bindings, next_id, has_return, loop_depth)?;
            }
        }
        _ => {}
    }
    // Continue checking later statements of an unused template, even if
    // this substitution cannot type-check an earlier statement. Retain
    // only its declared/inferred type, never a fabricated value or IR.
    if let Statement::Binding {
        name,
        annotation,
        initializer,
        location,
        mutable,
    } = statement
        && !bindings.contains_key(name)
    {
        let ty = annotation
            .map(|ty| bindings.resolve_type(ty))
            .or_else(|| crate::check::expression::operand_type(initializer, bindings))
            .unwrap_or(Type::Integer(crate::ast::IntegerType::I64));
        let id = *next_id;
        *next_id += 1;
        declared.push(name.clone());
        let generic_dependent = annotation.map_or_else(
            || crate::check::expression::generic_dependent(initializer, bindings),
            Type::generic,
        );
        bindings.insert(
            name.clone(),
            Binding::local(id, ty, generic_dependent, *mutable, false, *location),
        );
    }
    Ok(())
}
