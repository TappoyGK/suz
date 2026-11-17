//! 文字列をUnicodeスカラー単位でたどるforループを生成する。正順・逆順とcontinue時の更新を扱う。
//!
//! 利用元: [`crate::codegen::statements`]。

use super::{ExpressionContext, expressions::expression, statements::emit_block};
use crate::check::{CheckedExpression, CheckedStatement};
use std::fmt::Write;

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_text_loop(
    c: &mut String,
    id: usize,
    source: &CheckedExpression,
    reverse: bool,
    body: &[CheckedStatement],
    depth: usize,
    next_assignment: &mut usize,
    context: &mut ExpressionContext,
) {
    context.uses_text = true;
    let indent = "    ".repeat(depth);
    let ty = source.ty.c_name();
    let (bytes, length) = super::text::contents(source.ty, &format!("suz_iter_{id}"));
    let source = expression(source, context);
    writeln!(
        c,
        "{indent}{{\n{indent}    const {ty} suz_iter_{id} = {source};"
    )
    .unwrap();
    writeln!(c, "{indent}    const char *suz_bytes_{id} = {bytes};").unwrap();
    writeln!(c, "{indent}    const uint64_t suz_length_{id} = {length};").unwrap();
    let initial = if reverse {
        format!("suz_length_{id}")
    } else {
        "0".into()
    };
    let condition = if reverse {
        format!("suz_pos_{id} > 0")
    } else {
        format!("suz_pos_{id} < suz_length_{id}")
    };
    writeln!(
        c,
        "{indent}    uint64_t suz_pos_{id} = {initial};\n{indent}    while ({condition}) {{"
    )
    .unwrap();
    if reverse {
        writeln!(c, "{indent}        do {{ --suz_pos_{id}; }} while (suz_pos_{id} > 0 && ((unsigned char)suz_bytes_{id}[suz_pos_{id}] & 0xc0) == 0x80);").unwrap();
    }
    writeln!(c, "{indent}        uint32_t suz_scalar_{id} = 0;\n{indent}        uint64_t suz_width_{id} = 0;").unwrap();
    // 文字列の不変条件によりUTF-8は有効。本体より前に移動し、continueでも次の文字へ進む。
    writeln!(c, "{indent}        (void)suz_char_decode(suz_bytes_{id} + suz_pos_{id}, suz_length_{id} - suz_pos_{id}, &suz_scalar_{id}, &suz_width_{id});").unwrap();
    if !reverse {
        writeln!(c, "{indent}        suz_pos_{id} += suz_width_{id};").unwrap();
    }
    writeln!(c, "{indent}        const uint32_t suz_local_{id} = suz_scalar_{id};\n{indent}        (void)suz_local_{id};").unwrap();
    emit_block(c, body, depth + 2, next_assignment, context);
    writeln!(c, "{indent}    }}\n{indent}}}").unwrap();
}
