//! 文字列定数・テンプレート・文字列の参照範囲をCへ変換する。補間値の評価と文字列の構築を分ける。
//!
//! 利用元: [`crate::codegen::expressions`]、[`crate::codegen::casts`]。

use super::{
    ExpressionContext,
    expressions::{c_string, expression},
};
use crate::{
    ast::{Location, TemplatePart, Type},
    check::CheckedExpression,
};
use std::fmt::Write;

pub(super) fn position(location: Location, context: &ExpressionContext) -> String {
    format!(
        "(suz_text_location){{{}, {}, {}}}",
        c_string(&context.source_name),
        location.line,
        location.column
    )
}

pub(super) fn constant(text: &str, context: &mut ExpressionContext) -> String {
    context.uses_text = true;
    let next = context.text_literals.len();
    let id = *context.text_literals.entry(text.into()).or_insert(next);
    format!("suz_text_constant_{id}")
}

// 補助関数は評価済みの値を受け取る。すべての副作用は呼び出し元で順に実行し、
// その後に出力バイト列の構築やインターンを行う。
pub(super) fn template(
    parts: &[TemplatePart<CheckedExpression>],
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    context.uses_text = true;
    let id = context.next_block;
    context.next_block += 1;
    let mut setup = Vec::new();
    let mut arguments = Vec::new();
    let mut parameters = Vec::new();
    let mut body = String::new();
    for (index, part) in parts.iter().enumerate() {
        match part {
            TemplatePart::Text(text) => {
                if !text.is_empty() {
                    writeln!(
                        body,
                        "    suz_text_buffer_append(&buffer, {}, UINT64_C({}), location);",
                        c_string(text),
                        text.len()
                    )
                    .unwrap();
                }
            }
            TemplatePart::Value(value) => {
                let temp = context.temporaries.len();
                context.temporaries.push(value.ty);
                setup.push(format!("suz_cmp_{temp} = {}", expression(value, context)));
                arguments.push(format!("suz_cmp_{temp}"));
                let name = format!("value_{index}");
                parameters.push(format!("{} {name}", value.ty.c_name()));
                match value.ty {
                    Type::Symbol | Type::Text => {
                        let (bytes, length) = contents(value.ty, &name);
                        writeln!(body, "    suz_text_buffer_append(&buffer, {bytes}, {length}, location);").unwrap();
                    },
                    Type::Exact => writeln!(body, "    {{ char digits[40]; size_t length = suz_format_exact({name}, digits); suz_text_buffer_append(&buffer, digits, length, location); }}").unwrap(),
                    Type::Char => writeln!(body, "    {{ char bytes[4]; size_t length = suz_char_encode({name}, bytes); suz_text_buffer_append(&buffer, bytes, length, location); }}").unwrap(),
                    Type::Bool => writeln!(body, "    suz_text_buffer_append(&buffer, {name} ? \"true\" : \"false\", {name} ? 4 : 5, location);").unwrap(),
                    Type::Enum(ty) => {
                        context.enum_logs.insert(ty.id);
                        writeln!(body, "    suz_text_buffer_append(&buffer, suz_enum_{}_names[{name}], suz_enum_{}_lengths[{name}], location);", ty.id, ty.id).unwrap();
                    }
                    Type::Integer(ty) => writeln!(body, "    suz_text_buffer_{}(&buffer, {name}, location);", if ty.bounds().0 < 0 { "i64" } else { "u64" }).unwrap(),
                    Type::Float(ty) => {
                        context.float_types.insert(ty);
                        writeln!(body, "    {{\n        char digits[32];\n        size_t length = suz_format_{}({name}, digits);\n        suz_text_buffer_append(&buffer, digits, length, location);\n    }}", ty.name()).unwrap();
                    }
                    _ => unreachable!("interpolation types are checked"),
                }
            }
        }
    }
    let parameters = if parameters.is_empty() {
        "void".into()
    } else {
        parameters.join(", ")
    };
    let mut helper = format!(
        "static inline suz_text suz_template_{id}({parameters}) {{\n    suz_text_buffer buffer = {{0}};\n"
    );
    writeln!(
        helper,
        "    const suz_text_location location = {};",
        position(location, context)
    )
    .unwrap();
    helper.push_str(&body);
    helper.push_str("    suz_text value = suz_text_intern(buffer.bytes, buffer.length, false, location);\n    suz_text_buffer_dispose(&buffer);\n    return value;\n}\n\n");
    context.functions.push_str(&helper);
    setup.push(format!("suz_template_{id}({})", arguments.join(", ")));
    format!("({})", setup.join(", "))
}

// 呼び出す側が評価済みのローカル値を渡すため、バイト範囲を選ぶ際に式を重複評価しない。
// 生成Cに汎用型の動的な振り分けは不要。
pub(super) fn contents(ty: Type, name: &str) -> (String, String) {
    let prefix = match ty {
        Type::Symbol => "suz_text",
        Type::Text => "suz_view",
        _ => unreachable!(),
    };
    (
        format!("{prefix}_bytes({name})"),
        format!("{prefix}_length({name})"),
    )
}
