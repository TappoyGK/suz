use super::{
    ExpressionContext,
    expressions::{c_string, expression},
};
use crate::{
    ast::{Location, TemplatePart, Type},
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
    writeln!(c, "{indent}{{").unwrap();
    for (index, part) in parts.iter().enumerate() {
        if let TemplatePart::Value(value) = part {
            writeln!(
                c,
                "{indent}    const {} suz_log_{id}_{index} = {};",
                value.ty.c_name(),
                expression(value, context)
            )
            .unwrap();
        }
    }
    let filename = std::path::Path::new(&context.source_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&context.source_name);
    writeln!(
        c,
        "{indent}    suz_log_begin({}, {});",
        c_string(filename),
        location.line
    )
    .unwrap();
    for (index, part) in parts.iter().enumerate() {
        match part {
            TemplatePart::Text(text) => {
                writeln!(
                    c,
                    "{indent}    suz_log_text({}, {});",
                    c_string(text),
                    text.len()
                )
                .unwrap();
            }
            TemplatePart::Value(value) => {
                let kind = match value.ty {
                    Type::Enum(ty) => {
                        context.enum_logs.insert(ty.id);
                        writeln!(c, "{indent}    suz_log_text(suz_enum_{}_names[suz_log_{id}_{index}], suz_enum_{}_lengths[suz_log_{id}_{index}]);", ty.id, ty.id).unwrap();
                        continue;
                    }
                    Type::Bool => "bool",
                    Type::Integer(ty) if ty.bounds().0 < 0 => "i64",
                    Type::Integer(_) => "u64",
                    Type::Float(ty) => ty.name(),
                    Type::Void | Type::Struct(_) => {
                        unreachable!("invalid interpolation type is rejected")
                    }
                };
                writeln!(c, "{indent}    suz_log_{kind}(suz_log_{id}_{index});").unwrap();
            }
        }
    }
    writeln!(c, "{indent}    suz_log_end();\n{indent}}}").unwrap();
}
