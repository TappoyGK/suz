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
    (location, level): (Location, crate::ast::LogLevel),
    depth: usize,
    id: usize,
    context: &mut ExpressionContext,
) {
    let indent = "    ".repeat(depth);
    writeln!(c, "{indent}{{").unwrap();
    // Evaluate all values before acquiring the output lock. Nested logs and
    // panics in these expressions must complete before this log begins.
    for (index, part) in parts.iter().enumerate() {
        if let TemplatePart::Value(value) = part {
            let evaluated = expression(value, context);
            writeln!(
                c,
                "{indent}    const {} suz_log_{id}_{index} = {evaluated};",
                value.ty.c_name()
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
        "{indent}    suz_log_begin({}, {}, {});",
        c_string(level.name()),
        c_string(filename),
        location.line
    )
    .unwrap();
    for (index, part) in parts.iter().enumerate() {
        let output = match part {
            TemplatePart::Text(text) => {
                format!("suz_log_text({}, {});", c_string(text), text.len())
            }
            TemplatePart::Value(value) => {
                let name = format!("suz_log_{id}_{index}");
                match value.ty {
                    Type::Symbol | Type::Text => {
                        let (bytes, length) = super::text::contents(value.ty, &name);
                        format!("suz_log_text({bytes}, (size_t){length});")
                    }
                    Type::Char => format!("suz_log_char({name});"),
                    Type::Bool => format!("suz_log_bool({name});"),
                    Type::Enum(ty) => {
                        context.enum_logs.insert(ty.id);
                        format!(
                            "suz_log_text(suz_enum_{}_names[{name}], suz_enum_{}_lengths[{name}]);",
                            ty.id, ty.id
                        )
                    }
                    Type::Integer(ty) => format!(
                        "suz_log_{}({name});",
                        if ty.bounds().0 < 0 { "i64" } else { "u64" }
                    ),
                    Type::Float(ty) => {
                        context.float_types.insert(ty);
                        format!("suz_log_{}({name});", ty.name())
                    }
                    _ => unreachable!("interpolation types are checked"),
                }
            }
        };
        writeln!(c, "{indent}    {output}").unwrap();
    }
    writeln!(c, "{indent}    suz_log_end();\n{indent}}}").unwrap();
}
