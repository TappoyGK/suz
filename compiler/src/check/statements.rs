use super::{
    control::lower_return_handler,
    expression::expression,
    ir::{CheckedExpression, CheckedStatement},
};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{ExpressionKind, Location, Statement, Type},
};

#[derive(Clone)]
pub(super) struct Binding {
    pub(super) constant: Option<CheckedExpression>,
    pub(super) id: usize,
    pub(super) ty: Type,
    pub(super) mutable: bool,
    pub(super) parameter: bool,
    pub(super) loop_variable: bool,
    pub(super) assigned: std::cell::Cell<bool>,
    pub(super) location: Location,
}

pub(super) fn block(
    input: &[Statement],
    bindings: &mut Scope,
    next_id: &mut usize,
    has_return: &mut bool,
    loop_depth: usize,
) -> Result<Vec<CheckedStatement>, Diagnostic> {
    block_with_bindings(input, bindings, next_id, has_return, loop_depth, Vec::new())
}

pub(super) fn block_with_bindings(
    input: &[Statement],
    bindings: &mut Scope,
    next_id: &mut usize,
    has_return: &mut bool,
    loop_depth: usize,
    mut declared: Vec<String>,
) -> Result<Vec<CheckedStatement>, Diagnostic> {
    let statements = check_statements(
        input,
        bindings,
        next_id,
        has_return,
        loop_depth,
        &mut declared,
    )?;
    finish_scope(bindings, declared)?;
    Ok(statements)
}

pub(super) fn check_statements(
    input: &[Statement],
    bindings: &mut Scope,
    next_id: &mut usize,
    has_return: &mut bool,
    loop_depth: usize,
    declared: &mut Vec<String>,
) -> Result<Vec<CheckedStatement>, Diagnostic> {
    let mut statements = Vec::new();
    for statement in input {
        match statement {
            Statement::Log {
                parts,
                location,
                level,
            } => {
                let checked = super::text::parts(parts, bindings)?;
                statements.push(CheckedStatement::Log {
                    level: *level,
                    parts: checked,
                    location: *location,
                });
            }
            Statement::Expression(value) => {
                statements.push(CheckedStatement::Expression(expression(
                    value, None, bindings,
                )?));
            }
            Statement::Match {
                target,
                arms,
                location,
            } => {
                let target = crate::check::exact::use_value(target, bindings)?;
                let patterns = super::matches::patterns(target.ty, arms, *location, bindings)?;
                let id = *next_id;
                *next_id += 1;
                let mut checked = Vec::new();
                for (arm, pattern) in arms.iter().zip(patterns.values) {
                    checked.push((
                        pattern,
                        block(&arm.body, bindings, next_id, has_return, loop_depth)?,
                    ));
                }
                statements.push(CheckedStatement::Match {
                    id,
                    target,
                    arms: checked,
                    pattern_warnings: patterns.warnings,
                });
            }
            Statement::For {
                name,
                location,
                start,
                end,
                inclusive,
                reverse,
                body,
            } => {
                let (start, end, variable_type) = if let Some(end) = end {
                    let (start, end) = super::ranges::bounds(start, end, bindings)?;
                    let ty = start.ty;
                    (start, Some(end), ty)
                } else {
                    super::ranges::check_references(start, bindings)?;
                    let value = expression(start, None, bindings)?;
                    if !value.ty.is_text() {
                        return Err(start.location.error(
                            ErrorKind::InvalidSource,
                            "iteration without a range requires text [char.for-type]",
                        ));
                    }
                    (value, None, Type::Char)
                };
                if bindings.functions.contains_key(name)
                    || bindings.is_type_name(name)
                    || bindings.contains_key(name)
                {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        "loop variable shadows an existing name [decl.shadowing]",
                    ));
                }
                let id = *next_id;
                *next_id += 1;
                bindings.insert(
                    name.clone(),
                    Binding {
                        constant: None,
                        parameter: false,
                        id,
                        ty: variable_type,
                        mutable: false,
                        loop_variable: true,
                        assigned: Default::default(),
                        location: *location,
                    },
                );
                let body = block_with_bindings(
                    body,
                    bindings,
                    next_id,
                    has_return,
                    loop_depth + 1,
                    vec![name.clone()],
                )?;
                statements.push(CheckedStatement::For {
                    id,
                    start,
                    end,
                    inclusive: *inclusive,
                    reverse: *reverse,
                    body,
                });
            }
            Statement::Loop { condition, body } => {
                let unconditional = condition
                    .as_ref()
                    .is_none_or(|expr| matches!(expr.kind, ExpressionKind::Bool(true)));
                let condition = condition
                    .as_ref()
                    .map(|expr| {
                        let value = expression(expr, None, bindings)?;
                        if value.ty != Type::Bool {
                            return Err(expr.location.error(
                                ErrorKind::InvalidSource,
                                "while condition must be bool [while.loop]",
                            ));
                        }
                        Ok(value)
                    })
                    .transpose()?;
                let body = block(body, bindings, next_id, has_return, loop_depth + 1)?;
                statements.push(CheckedStatement::Loop {
                    condition,
                    body,
                    unconditional,
                });
            }
            Statement::Break(location) | Statement::Continue(location) => {
                if loop_depth == 0 {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        "break and continue require an enclosing loop [loop.jump-context]",
                    ));
                }
                statements.push(if matches!(statement, Statement::Break(_)) {
                    CheckedStatement::Break
                } else {
                    CheckedStatement::Continue
                });
            }
            Statement::If {
                branches,
                otherwise,
            } => {
                let mut checked = Vec::new();
                for (condition, body) in branches {
                    let value = expression(condition, None, bindings)?;
                    if value.ty != Type::Bool {
                        return Err(condition.location.error(
                            ErrorKind::InvalidSource,
                            "if and elif conditions must be bool [if.condition]",
                        ));
                    }
                    checked.push((
                        value,
                        block(body, bindings, next_id, has_return, loop_depth)?,
                    ));
                }
                let otherwise = otherwise
                    .as_ref()
                    .map(|body| block(body, bindings, next_id, has_return, loop_depth))
                    .transpose()?;
                statements.push(CheckedStatement::If {
                    branches: checked,
                    otherwise,
                });
            }
            Statement::Block(inner) => {
                statements.push(CheckedStatement::Block(block(
                    inner, bindings, next_id, has_return, loop_depth,
                )?));
            }
            Statement::Binding {
                mutable,
                name,
                location,
                annotation,
                initializer,
            } => {
                if bindings.functions.contains_key(name) || bindings.is_type_name(name) {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        "binding shadows a top-level name [decl.shadowing]",
                    ));
                }
                if bindings.contains_key(name) {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        if declared.contains(name) && bindings[name].parameter {
                            format!("binding duplicates a parameter '{name}' [fn.parameter-scope]")
                        } else if declared.contains(name) {
                            format!("duplicate binding '{name}' [decl.duplicate]")
                        } else {
                            format!("binding '{name}' shadows an outer binding [decl.shadowing]")
                        },
                    ));
                }
                // Resolve before inserting: a binding is not visible in its own initializer.
                let mut initializer =
                    super::expression::coerced(initializer, *annotation, bindings)?;
                if initializer.ty == Type::Void {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        "bindings cannot have type void [type.void]",
                    ));
                }
                *has_return |= lower_return_handler(&mut initializer, &mut statements, next_id);
                let id = *next_id;
                *next_id += 1;
                declared.push(name.clone());
                let ty = initializer.ty;
                bindings.insert(
                    name.clone(),
                    Binding {
                        constant: None,
                        parameter: false,
                        id,
                        ty,
                        mutable: *mutable,
                        loop_variable: false,
                        assigned: Default::default(),
                        location: *location,
                    },
                );
                statements.push(CheckedStatement::Binding {
                    mutable: *mutable,
                    id,
                    ty,
                    initializer,
                });
            }
            Statement::Assign {
                name,
                fields,
                location,
                value,
                ..
            } => {
                let binding = bindings.get(name).ok_or_else(|| {
                    location.error(
                        ErrorKind::InvalidSource,
                        format!("binding '{name}' is not in scope [decl.local-scope]"),
                    )
                })?;
                if binding.constant.is_some() {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        "cannot assign to a constant [const.decl]",
                    ));
                }
                if !binding.mutable {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        if binding.parameter {
                            format!("cannot assign to parameter '{name}' [fn.parameter]")
                        } else if binding.loop_variable {
                            format!("cannot assign to loop variable '{name}' [for.variable]")
                        } else {
                            format!("cannot assign to fix binding '{name}' [var.fix-assign]")
                        },
                    ));
                }
                super::writes::check(value, name, fields)?;
                let (id, mut ty) = (binding.id, binding.ty);
                let mut indices = Vec::new();
                for name in fields {
                    let (index, field_type) =
                        super::expression::structs::field(ty, name, *location, bindings)?;
                    indices.push(index);
                    ty = field_type;
                }
                let mut value = super::expression::coerced(value, Some(ty), bindings)?;
                *has_return |= lower_return_handler(&mut value, &mut statements, next_id);
                bindings[name].assigned.set(true);
                statements.push(CheckedStatement::Assign {
                    id,
                    fields: indices,
                    ty,
                    value,
                });
            }
            Statement::Return(value) => {
                statements.push(CheckedStatement::Return(super::functions::return_value(
                    value, bindings,
                )?));
                *has_return = true;
            }
        }
    }
    Ok(statements)
}

pub(super) fn finish_scope(bindings: &mut Scope, declared: Vec<String>) -> Result<(), Diagnostic> {
    for name in declared {
        let binding = bindings.remove(&name).unwrap();
        if binding.mutable && !binding.assigned.get() {
            return Err(binding.location.error(
                ErrorKind::InvalidSource,
                format!("var binding '{name}' is never reassigned [var.never-reassigned]"),
            ));
        }
    }
    Ok(())
}
