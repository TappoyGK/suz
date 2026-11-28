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
                    Type::Range(ty) => writeln!(body, "    suz_text_buffer_{}(&buffer, {name}, location);", if ty.base.bounds().0 < 0 { "i64" } else { "u64" }).unwrap(),
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

/// インターン表へ登録しない、プログラム終了まで有効な文字列。
pub(super) fn static_view(text: &str) -> String {
    format!(
        "(suz_text_view){{{}, UINT64_C({}), UINT64_C({}), NULL}}",
        c_string(text),
        text.len(),
        text.chars().count()
    )
}

// 容量と埋め込みを呼び出し元で順に評価する。補助関数内では数値を固定長の
// スタック領域へ整形し、全断片の長さを求めてから作業領域へ直接組み立てる。
pub(super) fn buffer(
    capacity: Option<&CheckedExpression>,
    value: &CheckedExpression,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    compose(capacity, None, value, location, context)
}

pub(super) fn append(
    left: &CheckedExpression,
    right: &CheckedExpression,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    compose(None, Some(left), right, location, context)
}

// 追記では旧値を右辺より先に保存する。整形と断片の直接コピーはtextbufと共通。
fn compose(
    capacity: Option<&CheckedExpression>,
    left: Option<&CheckedExpression>,
    value: &CheckedExpression,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    use crate::check::Value;
    context.uses_arena = true;
    context.uses_text = true;
    let id = context.next_block;
    context.next_block += 1;
    let mut setup = Vec::new();
    let mut arguments = Vec::new();
    let mut parameters = vec![
        if left.is_some() {
            "suz_text_view left"
        } else {
            "uint64_t capacity"
        }
        .to_string(),
    ];
    if let Some(capacity) = left.or(capacity) {
        let temp = context.temporaries.len();
        context.temporaries.push(capacity.ty);
        setup.push(format!(
            "suz_cmp_{temp} = {}",
            expression(capacity, context)
        ));
        arguments.push(format!("suz_cmp_{temp}"));
    } else {
        arguments.push("0".into());
    }
    let parts = match &value.value {
        Value::Template { parts, .. } => parts.clone(),
        _ => vec![TemplatePart::Value(value.clone())],
    };
    let mut body = String::new();
    let mut fragments = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let value = match part {
            TemplatePart::Text(text) => {
                if !text.is_empty() {
                    fragments.push(static_view(text));
                }
                continue;
            }
            TemplatePart::Value(value) => value,
        };
        let temp = context.temporaries.len();
        context.temporaries.push(value.ty);
        setup.push(format!("suz_cmp_{temp} = {}", expression(value, context)));
        arguments.push(format!("suz_cmp_{temp}"));
        let name = format!("value_{index}");
        parameters.push(format!("{} {name}", value.ty.c_name()));
        let view = match value.ty {
            Type::Text => name,
            Type::Symbol => format!("suz_text_as_view({name})"),
            Type::Bool => format!(
                "(suz_text_view){{{name} ? \"true\" : \"false\", {name} ? 4 : 5, {name} ? 4 : 5, NULL}}"
            ),
            Type::Enum(ty) => {
                context.enum_logs.insert(ty.id);
                // 列挙名はASCII識別子なのでバイト長とスカラー値の個数は同じ。
                format!(
                    "(suz_text_view){{suz_enum_{}_names[{name}], suz_enum_{}_lengths[{name}], suz_enum_{}_lengths[{name}], NULL}}",
                    ty.id, ty.id, ty.id
                )
            }
            ty => {
                let function = match ty {
                    Type::Char => "suz_char_encode".into(),
                    Type::Exact => "suz_format_exact".into(),
                    Type::Integer(ty) => format!(
                        "suz_text_format_{}",
                        if ty.bounds().0 < 0 { "i64" } else { "u64" }
                    ),
                    Type::Range(ty) => format!(
                        "suz_text_format_{}",
                        if ty.base.bounds().0 < 0 { "i64" } else { "u64" }
                    ),
                    Type::Float(ty) => {
                        context.float_types.insert(ty);
                        format!("suz_format_{}", ty.name())
                    }
                    _ => unreachable!("checked interpolation"),
                };
                writeln!(body, "    char bytes_{index}[40];\n    size_t length_{index} = {function}({name}, bytes_{index});").unwrap();
                let scalars = if ty == Type::Char {
                    "1".into()
                } else {
                    format!("length_{index}")
                };
                format!("(suz_text_view){{bytes_{index}, length_{index}, {scalars}, NULL}}")
            }
        };
        fragments.push(view);
    }
    let count = fragments.len();
    // Cには長さ0の配列がないため、空テンプレートにはダミーを置く。
    // countは0のままにし、構築時にはこの要素を走査しない。
    if fragments.is_empty() {
        fragments.push(static_view(""));
    }
    let build = if left.is_some() {
        "suz_textbuf_append(left"
    } else {
        "suz_textbuf_build(capacity"
    };
    let helper = format!(
        "static inline suz_text_view suz_textbuf_{id}({}) {{\n{body}    const suz_text_view parts[] = {{{}}};\n    return {build}, parts, {count}, {});\n}}\n\n",
        parameters.join(", "),
        fragments.join(", "),
        position(location, context)
    );
    context.functions.push_str(&helper);
    setup.push(format!("suz_textbuf_{id}({})", arguments.join(", ")));
    format!("({})", setup.join(", "))
}
