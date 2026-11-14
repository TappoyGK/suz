use super::{ExpressionContext, expressions::c_string};
use crate::{
    ast::{Location, TemplatePart},
    check::CheckedExpression,
};
use std::fmt::Write;

pub(super) fn emit(
    c: &mut String,
    parts: &[TemplatePart<CheckedExpression>],
    location: Location,
    depth: usize,
    id: usize,
    context: &mut ExpressionContext,
) {
    let indent = "    ".repeat(depth);
    let value = super::text::template(parts, location, true, context);
    let filename = std::path::Path::new(&context.source_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&context.source_name);
    writeln!(c, "{indent}{{\n{indent}    suz_text_buffer suz_log_{id} = {value};\n{indent}    suz_log_begin({}, {});\n{indent}    suz_log_text(suz_log_{id}.bytes, (size_t)suz_log_{id}.length);\n{indent}    suz_log_end();\n{indent}    suz_text_buffer_dispose(&suz_log_{id});\n{indent}}}", c_string(filename), location.line).unwrap();
}
