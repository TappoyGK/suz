//! 文を順に検査し、束縛・代入・分岐・反復のIRを作る。スコープ内の再代入や制御移動の条件を確認する。
//!
//! 利用元: [`crate::check::functions`]、[`crate::check::expression`] のブロック検査。
//! 主な対象規則: `[char.for-type]`、`[const.decl]`、`[decl.duplicate]`、`[decl.local-scope]`、`[decl.shadowing]`、`[fn.parameter]`。

use super::{
    control::lower_return_handler,
    expression::expression,
    ir::{CheckedExpression, CheckedStatement},
};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Location, Statement, Type},
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

impl Binding {
    // 型のヒント取得では、IRを出力せず代入状態も調べない。
    pub(super) fn hint(ty: Type, location: Location) -> Self {
        Self::local(0, ty, false, false, location)
    }

    pub(super) fn local(
        id: usize,
        ty: Type,
        mutable: bool,
        loop_variable: bool,
        location: Location,
    ) -> Self {
        Self {
            constant: None,
            id,
            ty,
            mutable,
            parameter: false,
            loop_variable,
            assigned: Default::default(),
            location,
        }
    }
}

pub(super) fn block(
    input: &[Statement],
    bindings: &mut Scope,
    next_id: &mut usize,
    loop_depth: usize,
) -> Result<Vec<CheckedStatement>, Diagnostic> {
    block_with_bindings(input, bindings, next_id, loop_depth, Vec::new())
}

pub(super) fn block_with_bindings(
    input: &[Statement],
    bindings: &mut Scope,
    next_id: &mut usize,
    loop_depth: usize,
    mut declared: Vec<String>,
) -> Result<Vec<CheckedStatement>, Diagnostic> {
    let statements = check_statements(input, bindings, next_id, loop_depth, &mut declared)?;
    finish_scope(bindings, declared)?;
    Ok(statements)
}

pub(super) fn check_statements(
    input: &[Statement],
    bindings: &mut Scope,
    next_id: &mut usize,
    loop_depth: usize,
    declared: &mut Vec<String>,
) -> Result<Vec<CheckedStatement>, Diagnostic> {
    let mut context = StatementContext {
        bindings,
        next_id,
        loop_depth,
        declared,
        output: Vec::new(),
    };
    let saved_loop = context.bindings.loop_depth;
    context.bindings.loop_depth = loop_depth;
    let result = input
        .iter()
        .try_for_each(|statement| check_statement(statement, &mut context));
    context.bindings.loop_depth = saved_loop;
    result?;
    Ok(context.output)
}

pub(super) struct StatementContext<'a> {
    pub(super) bindings: &'a mut Scope,
    pub(super) next_id: &'a mut usize,
    pub(super) loop_depth: usize,
    pub(super) declared: &'a mut Vec<String>,
    pub(super) output: Vec<CheckedStatement>,
}

fn check_statement(
    statement: &Statement,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    #[cfg(test)]
    super::generic::work();
    match statement {
        Statement::Arena { .. } | Statement::Let { .. } | Statement::Store { .. } => {
            super::arena::statement(statement, context)?
        }
        Statement::Recover(value) => {
            let Some(ty) = context.bindings.recovery.filter(|ty| *ty != Type::Void) else {
                return Err(value.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "err.recover",
                        "recover requires a non-void error block arm"
                    ),
                ));
            };
            let value = super::expression::coerced(value, Some(ty), context.bindings)?;
            context.output.push(CheckedStatement::Recover(value));
        }
        Statement::Ignore(ignore) => {
            let location = ignore.location;
            if context.bindings.recovery != Some(Type::Void) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "err.or-ignore",
                        "ignore statements require a void error arm"
                    ),
                ));
            }
            context
                .output
                .push(CheckedStatement::Ignore(ignore.clone()));
        }

        Statement::Fail(value) => context
            .output
            .push(CheckedStatement::Fail(super::errors::fail(
                value,
                context.bindings,
            )?)),
        Statement::Log {
            parts,
            location,
            level,
        } => {
            let checked = super::text::parts(parts, context.bindings)?;
            context.output.push(CheckedStatement::Log {
                level: *level,
                parts: checked,
                location: *location,
            });
        }
        Statement::Expression(value) => {
            let mut checked = expression(value, None, context.bindings)?;
            if let super::ir::Value::HandledCall { handler, .. } = &checked.value
                && matches!(
                    handler.as_ref(),
                    super::ir::CallHandler::Return { .. } | super::ir::CallHandler::Fail(_)
                )
                && checked.ty != Type::Void
            {
                return Err(value.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.or-return-position",
                        "a call statement with or return or or fail must have success type void"
                    ),
                ));
            }
            // void呼び出しを文へ移した場合、評価すべき値の式は残らない。
            if !lower_return_handler(
                &mut checked,
                &mut context.output,
                context.next_id,
                value.location,
            ) {
                context.output.push(CheckedStatement::Expression(checked));
            }
        }
        Statement::Match {
            target,
            arms,
            location,
        } => {
            let target = crate::check::exact::use_value(target, context.bindings)?;
            let patterns = super::matches::patterns(target.ty, arms, *location, context.bindings)?;
            let id = *context.next_id;
            *context.next_id += 1;
            let mut checked = Vec::new();
            for (arm, pattern) in arms.iter().zip(patterns.values) {
                checked.push((
                    pattern,
                    block(
                        &arm.body,
                        context.bindings,
                        context.next_id,
                        context.loop_depth,
                    )?,
                ));
            }
            context.output.push(CheckedStatement::Match {
                id,
                target,
                arms: checked,
                pattern_warnings: patterns.warnings,
            });
        }
        Statement::For { .. } => check_for(statement, context)?,

        Statement::Loop { condition, body } => {
            let condition = condition
                .as_ref()
                .map(|expr| {
                    condition_value(expr, context.bindings, || {
                        crate::diagnostic!("while.loop", "while condition must be bool")
                    })
                })
                .transpose()?;
            let body = block(
                body,
                context.bindings,
                context.next_id,
                context.loop_depth + 1,
            )?;
            context
                .output
                .push(CheckedStatement::Loop { condition, body });
        }
        Statement::Break(location) | Statement::Continue(location) => {
            if context.loop_depth == 0 {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "loop.jump-context",
                        "break and continue require an enclosing loop"
                    ),
                ));
            }
            context
                .output
                .push(if matches!(statement, Statement::Break(_)) {
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
                let value = condition_value(condition, context.bindings, || {
                    crate::diagnostic!("if.condition", "if and elif conditions must be bool")
                })?;
                let body = block(body, context.bindings, context.next_id, context.loop_depth)?;
                checked.push((value, body));
            }
            let otherwise = otherwise
                .as_ref()
                .map(|body| block(body, context.bindings, context.next_id, context.loop_depth))
                .transpose()?;
            context.output.push(CheckedStatement::If {
                branches: checked,
                otherwise,
            });
        }
        Statement::Trusted(inner) => {
            // スコープを複製すると、外側の変数への再代入の記録まで分離される。
            // 通常のブロックと同じ束縛を使い、trustedの状態だけを一時的に切り替える。
            let previous = context.bindings.trusted_block;
            context.bindings.trusted_block = true;
            let checked = block(inner, context.bindings, context.next_id, context.loop_depth);
            context.bindings.trusted_block = previous;
            context.output.push(CheckedStatement::Block(checked?));
        }
        Statement::Objection(inner) => {
            context.output.push(CheckedStatement::Objection(block(
                inner,
                context.bindings,
                context.next_id,
                context.loop_depth,
            )?));
        }
        Statement::Block(inner) => {
            context.output.push(CheckedStatement::Block(block(
                inner,
                context.bindings,
                context.next_id,
                context.loop_depth,
            )?));
        }
        Statement::Binding { .. } => check_binding(statement, context)?,

        Statement::Assign { .. } => check_assign(statement, context)?,

        Statement::Return(value) => {
            context.output.push(CheckedStatement::Return {
                value: super::functions::return_value(value, context.bindings)?,
                location: value.location,
            });
        }
    }
    Ok(())
}

fn check_for(statement: &Statement, context: &mut StatementContext<'_>) -> Result<(), Diagnostic> {
    let Statement::For {
        name,
        location,
        start,
        end,
        inclusive,
        reverse,
        body,
    } = statement
    else {
        unreachable!("matching statement handler")
    };

    let (start, end, variable_type) = if let Some(end) = end {
        let (start, end) = super::ranges::bounds(start, end, context.bindings)?;
        let ty = start.ty;
        (start, Some(end), ty)
    } else {
        super::ranges::check_references(start, context.bindings)?;
        let value = expression(start, None, context.bindings)?;
        if !value.ty.is_text() {
            return Err(start.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("char.for-type", "iteration without a range requires text"),
            ));
        }
        (value, None, Type::Char)
    };
    if context.bindings.functions.contains_key(name)
        || context.bindings.is_type_name(name)
        || context.bindings.contains_key(name)
    {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("decl.shadowing", "loop variable shadows an existing name"),
        ));
    }
    let id = *context.next_id;
    *context.next_id += 1;
    context.bindings.locals.insert(
        name.clone(),
        Binding::local(id, variable_type, false, true, *location),
    );
    let body = block_with_bindings(
        body,
        context.bindings,
        context.next_id,
        context.loop_depth + 1,
        vec![name.clone()],
    )?;
    context.output.push(CheckedStatement::For {
        id,
        start,
        end: end.map(Box::new),
        inclusive: *inclusive,
        reverse: *reverse,
        body,
    });

    Ok(())
}

fn check_binding(
    statement: &Statement,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    let Statement::Binding {
        pointer,
        mutable,
        name,
        location,
        annotation,
        initializer,
    } = statement
    else {
        unreachable!("matching statement handler")
    };

    if context.bindings.functions.contains_key(name) || context.bindings.is_type_name(name) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("decl.shadowing", "binding shadows a top-level name"),
        ));
    }
    if context.bindings.contains_key(name) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            if context.declared.contains(name) && context.bindings[name].parameter {
                crate::diagnostic!(
                    "fn.parameter-scope",
                    "binding duplicates a parameter '{name}'"
                )
            } else if context.declared.contains(name) {
                crate::diagnostic!("decl.duplicate", "duplicate binding '{name}'")
            } else {
                crate::diagnostic!(
                    "decl.shadowing",
                    "binding '{name}' shadows an outer binding"
                )
            },
        ));
    }
    // 登録する前に解決する。束縛は自身の初期化式から参照できない。
    let mut initializer = super::expression::coerced(
        initializer,
        annotation
            .map(|t| context.bindings.resolve_type_at(t, *location))
            .transpose()?,
        context.bindings,
    )?;
    if initializer.ty.pointee().is_some() != *pointer {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.ptr-decl",
                "pointer bindings require fix ptr or var ptr"
            ),
        ));
    }
    if matches!(initializer.ty, Type::Function(_)) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.type-call",
                "call-only function values cannot be stored in bindings"
            ),
        ));
    }
    if initializer.ty == Type::Void {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("type.void", "bindings cannot have type void"),
        ));
    }
    lower_return_handler(
        &mut initializer,
        &mut context.output,
        context.next_id,
        *location,
    );
    let id = *context.next_id;
    *context.next_id += 1;
    context.declared.push(name.clone());
    let ty = initializer.ty;
    context.bindings.locals.insert(
        name.clone(),
        Binding::local(id, ty, *mutable, false, *location),
    );
    context.output.push(CheckedStatement::Binding {
        location: *location,
        mutable: *mutable,
        id,
        ty,
        initializer: Box::new(initializer),
    });

    Ok(())
}

fn check_assign(
    statement: &Statement,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    let Statement::Assign {
        name,
        fields,
        location,
        value,
        compound,
    } = statement
    else {
        unreachable!("matching statement handler")
    };

    let binding = context.bindings.get(name).ok_or_else(|| {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("decl.local-scope", "binding '{name}' is not in scope"),
        )
    })?;
    if super::arena::indirect_path(binding.ty, fields, context.bindings) {
        return super::arena::named_store(statement, context);
    }
    if binding.constant.is_some() {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("const.decl", "cannot assign to a constant"),
        ));
    }
    if !binding.mutable {
        return Err(location.error(
            ErrorKind::InvalidSource,
            if context.bindings.value_captures.contains(name) {
                crate::diagnostic!(
                    "closure.capture-value",
                    "cannot assign to a value capture '{name}'"
                )
            } else if binding.parameter {
                crate::diagnostic!("fn.parameter", "cannot assign to parameter '{name}'")
            } else if binding.loop_variable {
                crate::diagnostic!("for.variable", "cannot assign to loop variable '{name}'")
            } else {
                crate::diagnostic!("var.fix-assign", "cannot assign to fix binding '{name}'")
            },
        ));
    }
    super::writes::check(value, name, fields, context.bindings)?;
    let (id, mut ty) = (binding.id, binding.ty);
    let mut indices = Vec::new();
    for name in fields {
        let (index, field_type) =
            super::expression::structs::field(ty, name, *location, context.bindings)?;
        indices.push(index);
        ty = field_type;
    }
    let mut value = if *compound {
        super::operator_methods::compound(value, ty, context.bindings)?
    } else {
        super::expression::coerced(value, Some(ty), context.bindings)?
    };
    lower_return_handler(&mut value, &mut context.output, context.next_id, *location);
    context.bindings[name].assigned.set(true);
    context.output.push(CheckedStatement::Assign {
        root_type: binding.ty,
        location: *location,
        id,
        fields: indices,
        ty,
        value: Box::new(value),
    });

    Ok(())
}

// 通常の関数と汎用関数で、条件には同じboolの規則を適用する。
fn condition_value(
    expr: &crate::ast::Expression,
    scope: &Scope,
    message: impl FnOnce() -> crate::DiagnosticMessage,
) -> Result<CheckedExpression, Diagnostic> {
    expression(expr, None, scope).and_then(|value| {
        if value.ty == Type::Bool {
            Ok(value)
        } else {
            Err(expr.location.error(ErrorKind::InvalidSource, message()))
        }
    })
}

pub(super) fn finish_scope(bindings: &mut Scope, declared: Vec<String>) -> Result<(), Diagnostic> {
    for name in declared {
        let binding = bindings.locals.remove(&name).unwrap();
        if binding.mutable && !binding.assigned.get() {
            let error = binding.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "var.never-reassigned",
                    "var binding '{name}' is never reassigned; declare it with fix"
                ),
            );
            return Err(error);
        }
    }
    Ok(())
}
