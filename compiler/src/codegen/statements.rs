//! IRの文をCの文へ変換する。分岐・反復・束縛・代入・returnを出力し、Suzの制御の意味を保つ。
//!
//! 利用元: [`crate::codegen::functions`]、[`crate::codegen::blocks`]。

use super::{
    ExpressionContext, arithmetic::integer_parts, casts::cast_parts, expressions::expression,
};
use crate::{ast::Type, check::CheckedStatement};
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
            CheckedStatement::Arena { body, location, .. } => {
                context.uses_arena = true;
                let id = *next_assignment;
                *next_assignment += 1;
                writeln!(c, "{indent}{{\n{indent}    suz_arena_mark suz_arena_mark_{id} __attribute__((cleanup(suz_arena_restore))) = suz_arena_save();").unwrap();
                if let Some(report) = context.arena_report {
                    if report.format == crate::ArenaReportFormat::Summary {
                        writeln!(c, "{indent}    size_t suz_arena_report_mark_{id} __attribute__((cleanup(suz_arena_report_restore))) = suz_arena_report_position;").unwrap();
                    } else {
                        writeln!(c, "{indent}    suz_arena_report_frame suz_arena_report_mark_{id} __attribute__((cleanup(suz_arena_report_leave)));").unwrap();
                        writeln!(c, "{indent}    suz_arena_tree_enter(&suz_arena_report_tree, &suz_arena_report_mark_{id}, suz_arena_report_position, {}, {}, {}, {});",
                            super::expressions::c_string(&context.source_name), location.line, location.column,
                            super::expressions::c_string(&context.report_function)).unwrap();
                    }
                }
                emit_block(c, body, depth + 1, next_assignment, context);
                writeln!(c, "{indent}}}").unwrap();
            }
            CheckedStatement::Store {
                target,
                id,
                value,
                body,
                ..
            } => {
                writeln!(
                    c,
                    "{indent}{{\n{indent}    {} const suz_local_{id} = &({});",
                    target.ty.pointer().c_name(),
                    expression(target, context)
                )
                .unwrap();
                emit_block(c, body, depth + 1, next_assignment, context);
                writeln!(
                    c,
                    "{indent}    *suz_local_{id} = {};\n{indent}}}",
                    expression(value, context)
                )
                .unwrap();
            }

            CheckedStatement::Recover(value) => {
                let target = context
                    .recovery
                    .as_mut()
                    .expect("recover belongs to an error arm");
                target.used = true;
                let result = target.result.clone().expect("recover requires a value");
                let label = target.label.clone();
                writeln!(
                    c,
                    "{indent}{result} = {};\n{indent}goto {label};",
                    expression(value, context)
                )
                .unwrap();
            }

            CheckedStatement::Fail(value) => {
                let failure = super::errors::fail(Some(value), "", context);
                writeln!(c, "{indent}{failure}").unwrap();
            }
            CheckedStatement::HandleCall { id, call, handler } => {
                super::errors::statement(c, *id, call, handler, &indent, context)
            }
            CheckedStatement::Ignore(ignore) => {
                if context.log_ignore {
                    writeln!(c, "{indent}{};", super::logs::ignore(ignore, context)).unwrap();
                }
            }
            CheckedStatement::Log {
                parts,
                location,
                level,
            } => {
                // 検査済みIRを保持したまま生成だけを省く。値の読み取りや表示用データも登録しない。
                if !context.log_level.includes(*level) {
                    continue;
                }
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
                    // 網羅性はIRへ変換する前に検査済みなので、最後の分岐は無条件にする。
                    // breakはmatchではなく外側のSuzループに属するため、Cのswitchを使わない。
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
                variable_type,
                start,
                end,
                inclusive,
                reverse,
                body,
            } => {
                let Some(end) = end else {
                    if start.ty.array_id().is_some() {
                        super::arrays::emit_loop(
                            c,
                            *id,
                            *variable_type,
                            start,
                            *reverse,
                            body,
                            depth,
                            next_assignment,
                            context,
                        );
                        continue;
                    }
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
                        // 空の範囲では、型の最小値から減算してはならない。
                        writeln!(c, "{indent}    if (suz_more_{id}) --suz_cursor_{id};").unwrap();
                    }
                    // forの更新節で進めることで、最小値でのcontinueも扱う。
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
                    // 最大値でのcontinue後も含め、加算する前に終了条件を検査する。
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
                context.immutable_locals.insert(*id);
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
            CheckedStatement::Block(inner) | CheckedStatement::Objection(inner) => {
                writeln!(c, "{indent}{{").unwrap();
                emit_block(c, inner, depth + 1, next_assignment, context);
                writeln!(c, "{indent}}}").unwrap();
            }
            CheckedStatement::Binding {
                mutable,
                id,
                ty,
                initializer,
                ..
            } => {
                writeln!(
                    c,
                    "{indent}{}{} suz_local_{id} = {};",
                    if *mutable { "" } else { "const " },
                    ty.c_name(),
                    expression(initializer, context)
                )
                .unwrap();
                if !mutable {
                    context.immutable_locals.insert(*id);
                    if ty.array_id().is_some() {
                        super::arrays::cache(c, *id, *ty, &indent, context);
                    }
                }
                // Suzは全束縛の読み取りを要求しないため、Cの未使用警告を抑える。
                writeln!(c, "{indent}(void)suz_local_{id};").unwrap();
            }
            CheckedStatement::Assign {
                root_type,
                id,
                fields,
                ty,
                value,
                ..
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
                let mut owner = *root_type;
                for index in fields {
                    write!(
                        local,
                        ".{}",
                        super::structs::field_name(owner, *index, context)
                    )
                    .unwrap();
                    let Type::Struct(id) = owner else {
                        unreachable!("checked assignment field")
                    };
                    owner = context.structs[id].fields[*index].ty;
                }
                writeln!(c, "{indent}{local} = suz_assign_{index};").unwrap();
                writeln!(c, "{indent}(void){local};").unwrap();
            }
            CheckedStatement::Return { value, .. } => {
                writeln!(c, "{indent}return {};", expression(value, context)).unwrap();
            }
        }
    }
}
