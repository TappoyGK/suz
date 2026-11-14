use super::{ExpressionContext, expressions::expression, statements::emit_block};
use crate::{
    ast::Type,
    check::{CheckedExpression, CheckedStatement},
};
use std::fmt::Write;

pub(super) fn emit(
    statements: &[CheckedStatement],
    value: &CheckedExpression,
    captures: &[(usize, Type, bool)],
    context: &mut ExpressionContext,
) -> String {
    if statements.is_empty() {
        return expression(value, context);
    }
    let id = context.next_block;
    context.next_block += 1;
    let arguments = captures
        .iter()
        .map(|(id, _, _)| format!("&({})", context.local(*id)))
        .collect::<Vec<_>>()
        .join(", ");
    let saved_references = std::mem::replace(
        &mut context.references,
        captures.iter().map(|(id, _, _)| *id).collect(),
    );
    let saved_temporaries = std::mem::take(&mut context.temporaries);
    let mut body = String::new();
    emit_block(&mut body, statements, 1, &mut 0, context);
    writeln!(body, "    return {};", expression(value, context)).unwrap();
    let parameters = captures
        .iter()
        .map(|(id, ty, mutable)| {
            format!(
                "{}{} *suz_ref_{id}",
                if *mutable { "" } else { "const " },
                ty.c_name()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut function = format!(
        "static inline {} suz_block_{id}({}) {{\n",
        value.ty.c_name(),
        if parameters.is_empty() {
            "void"
        } else {
            &parameters
        }
    );
    for (index, ty) in context.temporaries.iter().enumerate() {
        writeln!(function, "    {} suz_cmp_{index};", ty.c_name()).unwrap();
    }
    function.push_str(&body);
    function.push_str("}\n\n");
    // Nested helpers have already been emitted, before their callers.
    context.functions.push_str(&function);
    context.temporaries = saved_temporaries;
    context.references = saved_references;
    format!("suz_block_{id}({arguments})")
}
