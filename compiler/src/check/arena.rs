//! arena内の確保・参照解除・更新を型検査する。寿命の候補伝播はlifetimesが担当する。
mod lifetimes;
use super::{
    expression,
    ir::{CheckedExpression, CheckedStatement, Value},
    scope::Scope,
    statements::{self, Binding, StatementContext},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Location, Statement, Type},
};
pub(crate) use lifetimes::Body as LifetimeBody;
pub(super) use lifetimes::check_program;

pub(super) fn contains(ty: Type, scope: &Scope) -> bool {
    contains_in(ty, &scope.structs)
}

fn contains_in(ty: Type, structs: &super::type_parameters::structs::Structs) -> bool {
    match ty {
        Type::Pointer { .. } => true,
        Type::Struct(id) => structs
            .get(id)
            .fields
            .iter()
            .any(|f| contains_in(f.ty, structs)),
        _ => false,
    }
}

pub(super) fn deref(
    value: CheckedExpression,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let Some(ty) = value.ty.pointee() else {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("arena.ptr-operations", "dereference requires ptr"),
        ));
    };
    if !value.ty.is_nonnull() {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.null-deref",
                "use let before dereferencing a possibly null pointer"
            ),
        ));
    }
    Ok(CheckedExpression {
        ty,
        value: Value::Deref(Box::new(value)),
    })
}

pub(super) fn expression(
    expr: &Expression,
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Null => {
            let Some(ty @ Type::Pointer { .. }) = expected else {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.null", "null requires an expected ptr? type"),
                ));
            };
            if !ty.is_nullable() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.null", "null requires ptr?"),
                ));
            }
            Ok(CheckedExpression {
                ty,
                value: Value::Null,
            })
        }
        ExpressionKind::New(inner) => {
            if scope.arena_depth == 0 {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arena.new",
                        "new requires a lexical arena in this function"
                    ),
                ));
            }
            let value = expression::coerced(inner, expected.and_then(Type::pointee), scope)?;
            if value.ty == Type::Void || matches!(value.ty, Type::Function(_)) {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.new-value", "new requires a stored value"),
                ));
            }
            Ok(CheckedExpression {
                ty: value.ty.pointer(),
                value: Value::New {
                    value: Box::new(value),
                    location: expr.location,
                },
            })
        }
        ExpressionKind::Deref(inner) => {
            deref(expression::expression(inner, None, scope)?, expr.location)
        }
        _ => unreachable!(),
    }
}

pub(super) fn statement(
    statement: &Statement,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    match statement {
        Statement::Arena { body, end, .. } => {
            context.bindings.arena_used.set(true);
            context.bindings.arena_depth += 1;
            let result =
                statements::block(body, context.bindings, context.next_id, context.loop_depth);
            context.bindings.arena_depth -= 1;
            context.output.push(CheckedStatement::Arena {
                body: result?,
                end: *end,
            });
        }
        Statement::Let {
            name,
            location,
            value,
            body,
            otherwise,
        } => {
            if context.bindings.contains_key(name)
                || context.bindings.functions.contains_key(name)
                || context.bindings.is_type_name(name)
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.shadowing", "let binding shadows an existing name"),
                ));
            }
            let value = expression::expression(value, None, context.bindings)?;
            if value.ty.pointee().is_none() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.let", "let requires ptr"),
                ));
            }
            let id = *context.next_id;
            *context.next_id += 1;
            let nonnull = value.ty.is_nonnull();
            let ty = value.ty.with_nullable(false);
            context.bindings.locals.insert(
                name.clone(),
                Binding::local(id, ty, false, false, *location),
            );
            let body = statements::block_with_bindings(
                body,
                context.bindings,
                context.next_id,
                context.loop_depth,
                vec![name.clone()],
            )?;
            let otherwise = statements::block(
                otherwise,
                context.bindings,
                context.next_id,
                context.loop_depth,
            )?;
            context.output.push(CheckedStatement::Block(vec![
                CheckedStatement::Binding {
                    location: *location,
                    mutable: false,
                    id,
                    ty,
                    initializer: Box::new(value),
                },
                if nonnull {
                    CheckedStatement::Block(body)
                } else {
                    CheckedStatement::If {
                        branches: vec![(
                            CheckedExpression {
                                ty: Type::Bool,
                                value: Value::NonNull(Box::new(CheckedExpression {
                                    ty,
                                    value: Value::Local(id),
                                })),
                            },
                            body,
                        )],
                        otherwise: Some(otherwise),
                    }
                },
            ]));
        }
        Statement::Store {
            target,
            value,
            operator,
            location,
        } => store(target, value, *operator, *location, context)?,
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn indirect_path(mut ty: Type, fields: &[String], scope: &Scope) -> bool {
    for name in fields {
        if ty.pointee().is_some() {
            return true;
        }
        let Type::Struct(id) = ty else { return false };
        let declaration = scope.structs.get(id);
        let Some(field) = declaration.fields.iter().find(|f| &f.name == name) else {
            return false;
        };
        ty = field.ty;
    }
    false
}

pub(super) fn named_store(
    statement: &Statement,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    let Statement::Assign {
        compound,
        name,
        fields,
        value,
        location,
    } = statement
    else {
        unreachable!()
    };
    let mut target = Expression {
        kind: ExpressionKind::Name(name.clone()),
        location: *location,
        height: 1,
    };
    for field in fields {
        target = Expression {
            height: target.height + 1,
            kind: ExpressionKind::Field {
                target: Box::new(target),
                name: field.clone(),
            },
            location: *location,
        };
    }
    let (operator, value) = if *compound {
        let ExpressionKind::Binary {
            operator, right, ..
        } = &value.kind
        else {
            unreachable!()
        };
        (Some(*operator), right.as_ref())
    } else {
        (None, value)
    };
    store(&target, value, operator, *location, context)
}

fn store(
    target: &Expression,
    value: &Expression,
    operator: Option<crate::ast::BinaryOperator>,
    location: Location,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    let target = expression::expression(target, None, context.bindings)?;
    fn indirect(value: &Value) -> bool {
        match value {
            Value::Deref(_) => true,
            Value::Field { target, .. } => indirect(&target.value),
            _ => false,
        }
    }
    if !indirect(&target.value) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.ptr-operations",
                "pointer assignment requires a dereferenced destination"
            ),
        ));
    }
    let id = *context.next_id;
    *context.next_id += 1;
    // 複合代入は保存済みの代入先から一度だけ読み、通常の算術検査を使う。
    let name = format!("$arena{id}");
    context.bindings.locals.insert(
        name.clone(),
        Binding::local(id, target.ty.pointer(), false, false, location),
    );
    let compound = operator.map(|operator| Expression {
        height: value.height + 2,
        location,
        kind: ExpressionKind::Binary {
            operator,
            left: Box::new(Expression {
                location,
                height: 2,
                kind: ExpressionKind::Deref(Box::new(Expression {
                    location,
                    height: 1,
                    kind: ExpressionKind::Name(name.clone()),
                })),
            }),
            right: Box::new(value.clone()),
        },
    });
    let result = if let Some(compound) = &compound {
        super::operator_methods::compound(compound, target.ty, context.bindings)
    } else {
        expression::coerced(value, Some(target.ty), context.bindings)
    };
    context.bindings.locals.remove(&name);
    let mut value = result?;
    let mut body = Vec::new();
    super::control::lower_return_handler(&mut value, &mut body, context.next_id, location);
    context.output.push(CheckedStatement::Store {
        target: Box::new(target),
        id,
        value: Box::new(value),
        body,
        location,
    });
    Ok(())
}

pub(super) fn check_lifetimes(
    statements: &[CheckedStatement],
    scope: &Scope,
    function: &crate::ast::Function,
    references: &std::collections::BTreeMap<String, Binding>,
    id: usize,
) -> Result<Option<LifetimeBody>, Diagnostic> {
    if !scope.arena_used.get()
        && !function.arena
        && !function.parameters.iter().any(|p| {
            contains(
                scope
                    .resolve_type_at(p.ty, p.location)
                    .expect("parameter type was checked before lifetime analysis"),
                scope,
            )
        })
    {
        return Ok(None);
    }
    lifetimes::check(statements, scope, function, references, id).map(Some)
}
