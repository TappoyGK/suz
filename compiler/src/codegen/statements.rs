use super::{
    ExpressionContext, arithmetic::integer_parts, casts::cast_parts, expressions::expression,
};
use crate::check::CheckedStatement;
use std::fmt::Write;

pub(super) fn emit_block(
    c: &mut String,
    statements: &[CheckedStatement],
    depth: usize,
    next_assignment: &mut usize,
    context: &mut ExpressionContext,
) {
    let indent = "    ".repeat(depth);
    for statement in statements {
        match statement {
            CheckedStatement::Log {
                parts,
                location,
                level,
            } => {
                let id = *next_assignment;
                *next_assignment += 1;
                super::logs::emit(c, parts, (*location, *level), depth, id, context);
            }
            CheckedStatement::Expression(value) => {
                writeln!(c, "{indent}(void)({});", expression(value, context)).unwrap();
            }
            CheckedStatement::Match {
                id, target, arms, ..
            } => {
                writeln!(c, "{indent}{{").unwrap();
                writeln!(
                    c,
                    "{indent}    const {} suz_match_{id} = {};",
                    target.ty.c_name(),
                    expression(target, context)
                )
                .unwrap();
                writeln!(c, "{indent}    (void)suz_match_{id};").unwrap();
                for (index, (pattern, body)) in arms.iter().enumerate() {
                    // Exhaustiveness is checked before lowering, so the last
                    // arm is unconditional. Avoid a C switch: break belongs
                    // to the enclosing Suz loop, not to this match.
                    if index + 1 == arms.len() {
                        writeln!(c, "{indent}    {}{{", if index == 0 { "" } else { "else " })
                            .unwrap();
                    } else {
                        let pattern = expression(pattern.as_ref().unwrap(), context);
                        let condition = super::expressions::equality(
                            target.ty,
                            &format!("suz_match_{id}"),
                            &pattern,
                        );
                        writeln!(
                            c,
                            "{indent}    {}if ({condition}) {{",
                            if index == 0 { "" } else { "else " }
                        )
                        .unwrap();
                    }
                    emit_block(c, body, depth + 2, next_assignment, context);
                    writeln!(c, "{indent}    }}").unwrap();
                }
                writeln!(c, "{indent}}}").unwrap();
            }
            CheckedStatement::For {
                id,
                start,
                end,
                inclusive,
                reverse,
                body,
            } => {
                let Some(end) = end else {
                    super::characters::emit_text_loop(
                        c,
                        *id,
                        start,
                        *reverse,
                        body,
                        depth,
                        next_assignment,
                        context,
                    );
                    continue;
                };
                let character = start.ty == crate::ast::Type::Char;
                let ty = start.ty.c_name();
                let start = expression(start, context);
                let end = expression(end, context);
                let comparison = if *inclusive { "<=" } else { "<" };
                writeln!(c, "{indent}{{").unwrap();
                writeln!(c, "{indent}    const {ty} suz_start_{id} = {start};").unwrap();
                writeln!(c, "{indent}    const {ty} suz_end_{id} = {end};").unwrap();
                if *reverse {
                    writeln!(c, "{indent}    _Bool suz_more_{id} = suz_start_{id} {comparison} suz_end_{id};").unwrap();
                    writeln!(c, "{indent}    {ty} suz_cursor_{id} = suz_end_{id};").unwrap();
                    if !inclusive && character {
                        writeln!(c, "{indent}    if (suz_more_{id}) suz_cursor_{id} = suz_cursor_{id} == 0xe000 ? 0xd7ff : suz_cursor_{id} - 1;").unwrap();
                    } else if !inclusive {
                        // Empty ranges must not subtract from the type's minimum.
                        writeln!(c, "{indent}    if (suz_more_{id}) --suz_cursor_{id};").unwrap();
                    }
                    // Updating in the for clause also handles continue at the minimum.
                    let step = if character {
                        format!(
                            "suz_cursor_{id} -= suz_more_{id} ? (suz_cursor_{id} == 0xe000 ? 0x801 : 1) : 0"
                        )
                    } else {
                        format!("suz_cursor_{id} -= suz_more_{id}")
                    };
                    writeln!(c, "{indent}    for (; suz_more_{id}; suz_more_{id} = suz_cursor_{id} > suz_start_{id}, {step}) {{").unwrap();
                } else {
                    writeln!(c, "{indent}    {ty} suz_cursor_{id} = suz_start_{id};").unwrap();
                    writeln!(c, "{indent}    _Bool suz_more_{id} = 1;").unwrap();
                    // Test before incrementing, including after continue at the maximum.
                    let step = if character {
                        format!(
                            "suz_cursor_{id} += suz_more_{id} ? (suz_cursor_{id} == 0xd7ff ? 0x801 : 1) : 0"
                        )
                    } else {
                        format!("suz_cursor_{id} += suz_more_{id}")
                    };
                    writeln!(c, "{indent}    for (; suz_more_{id} && suz_cursor_{id} {comparison} suz_end_{id}; suz_more_{id} = suz_cursor_{id} < suz_end_{id}, {step}) {{").unwrap();
                }
                writeln!(
                    c,
                    "{indent}        const {ty} suz_local_{id} = suz_cursor_{id};"
                )
                .unwrap();
                writeln!(c, "{indent}        (void)suz_local_{id};").unwrap();
                emit_block(c, body, depth + 2, next_assignment, context);
                writeln!(c, "{indent}    }}\n{indent}}}").unwrap();
            }
            CheckedStatement::SliceReturn {
                id, return_value, ..
            }
            | CheckedStatement::ArithmeticReturn {
                id, return_value, ..
            }
            | CheckedStatement::CastReturn {
                id, return_value, ..
            } => {
                let (ty, (setup, condition, converted)) = match statement {
                    CheckedStatement::SliceReturn { operation, .. } => {
                        (operation.ty, super::slices::parts(operation, context))
                    }
                    CheckedStatement::ArithmeticReturn { operation, .. } => {
                        (operation.ty, integer_parts(operation, context))
                    }
                    CheckedStatement::CastReturn {
                        ty,
                        operand,
                        wrap,
                        location,
                        ..
                    } => (*ty, cast_parts(operand, *ty, *wrap, *location, context)),
                    _ => unreachable!(),
                };
                writeln!(c, "{indent}{setup};").unwrap();
                writeln!(c, "{indent}if (!({condition})) {{").unwrap();
                writeln!(
                    c,
                    "{indent}    return {};",
                    expression(return_value, context)
                )
                .unwrap();
                writeln!(c, "{indent}}}").unwrap();
                writeln!(
                    c,
                    "{indent}const {} suz_local_{id} = {converted};",
                    ty.c_name()
                )
                .unwrap();
                writeln!(c, "{indent}(void)suz_local_{id};").unwrap();
            }
            CheckedStatement::Loop {
                condition, body, ..
            } => {
                let condition = condition
                    .as_ref()
                    .map(|expr| expression(expr, context))
                    .unwrap_or_else(|| "1".into());
                writeln!(c, "{indent}while ({condition}) {{").unwrap();
                emit_block(c, body, depth + 1, next_assignment, context);
                writeln!(c, "{indent}}}").unwrap();
            }
            CheckedStatement::Break => writeln!(c, "{indent}break;").unwrap(),
            CheckedStatement::Continue => writeln!(c, "{indent}continue;").unwrap(),
            CheckedStatement::If {
                branches,
                otherwise,
            } => {
                for (index, (condition, body)) in branches.iter().enumerate() {
                    writeln!(
                        c,
                        "{indent}{}if ({}) {{",
                        if index == 0 { "" } else { "else " },
                        expression(condition, context)
                    )
                    .unwrap();
                    emit_block(c, body, depth + 1, next_assignment, context);
                    writeln!(c, "{indent}}}").unwrap();
                }
                if let Some(body) = otherwise {
                    writeln!(c, "{indent}else {{").unwrap();
                    emit_block(c, body, depth + 1, next_assignment, context);
                    writeln!(c, "{indent}}}").unwrap();
                }
            }
            CheckedStatement::Block(inner) => {
                writeln!(c, "{indent}{{").unwrap();
                emit_block(c, inner, depth + 1, next_assignment, context);
                writeln!(c, "{indent}}}").unwrap();
            }
            CheckedStatement::Binding {
                mutable,
                id,
                ty,
                initializer,
            } => {
                writeln!(
                    c,
                    "{indent}{}{} suz_local_{id} = {};",
                    if *mutable { "" } else { "const " },
                    ty.c_name(),
                    expression(initializer, context)
                )
                .unwrap();
                // Suz does not require reading every binding; suppress C unused warnings.
                writeln!(c, "{indent}(void)suz_local_{id};").unwrap();
            }
            CheckedStatement::Assign {
                id,
                fields,
                ty,
                value,
            } => {
                let index = *next_assignment;
                *next_assignment += 1;
                writeln!(
                    c,
                    "{indent}const {} suz_assign_{index} = {};",
                    ty.c_name(),
                    expression(value, context)
                )
                .unwrap();
                let mut local = context.local(*id);
                for index in fields {
                    write!(local, ".suz_field_{index}").unwrap();
                }
                writeln!(c, "{indent}{local} = suz_assign_{index};").unwrap();
                writeln!(c, "{indent}(void){local};").unwrap();
            }
            CheckedStatement::Return(value) => {
                writeln!(c, "{indent}return {};", expression(value, context)).unwrap();
            }
        }
    }
}
