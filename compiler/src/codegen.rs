//! 検査済みIRからC23のソースを生成する。補助関数・定数・関数本体を出力順に組み立てる。
//!
//! 利用元: [`crate::compile_with_options`]。

mod arithmetic;
mod blocks;
mod casts;
mod characters;
mod exact;
mod expressions;
mod functions;
mod helpers;
mod logs;
mod slices;
mod statements;
mod structs;
#[cfg(test)]
mod tests;
mod text;
mod text_structure;

use crate::check::CheckedProgram;
use std::{collections::BTreeSet, fmt::Write};

#[derive(Default)]
struct ExpressionContext {
    text_literals: std::collections::BTreeMap<String, usize>,
    uses_text: bool,
    structs: Vec<crate::ast::StructDeclaration>,
    function_types: Vec<crate::ast::FunctionType>,
    call_only_types: BTreeSet<usize>,
    function_adapters: BTreeSet<usize>,
    struct_equals: BTreeSet<usize>,
    struct_parsers: BTreeSet<usize>,
    enums: Vec<crate::ast::EnumDeclaration>,
    enum_logs: BTreeSet<usize>,
    functions: String,
    next_block: usize,
    references: BTreeSet<usize>,
    helpers: BTreeSet<bool>,
    source_name: String,
    narrow_float: bool,
    narrow_float_check: bool,
    bit_helpers: BTreeSet<crate::ast::IntegerType>,
    float_helpers: BTreeSet<crate::ast::FloatType>,
    float_types: BTreeSet<crate::ast::FloatType>,
    temporaries: Vec<crate::ast::Type>,
}

impl ExpressionContext {
    fn local(&self, id: usize) -> String {
        if self.references.contains(&id) {
            format!("(*suz_ref_{id})")
        } else {
            format!("suz_local_{id}")
        }
    }
}

pub(crate) fn generate(
    program: &CheckedProgram,
    source_name: &str,
    text_pool_limit: crate::TextPoolLimit,
) -> String {
    let mut c = String::from(
        r#"#include <limits.h>
#include <stdint.h>
#include "suz_runtime.h"
#include "suz_text.h"
#include "suz_parse.h"
#include "suz_parse_struct.h"

_Static_assert(CHAR_BIT == 8, "Suz requires 8-bit bytes");
_Static_assert(UINT8_MAX == 255, "Suz requires uint8_t");

"#,
    );
    let mut context = ExpressionContext {
        source_name: source_name.into(),
        enums: program.enums.clone(),
        structs: program.structs.clone(),
        function_types: program.function_types.clone(),
        ..ExpressionContext::default()
    };
    for value in &program.constant_texts {
        text::constant(value, &mut context);
    }
    for declaration in &program.structs {
        for field in &declaration.fields {
            if let crate::ast::Type::Float(ty) = field.ty {
                context.float_types.insert(ty);
            }
        }
    }
    // 初期値を先に生成し、必要な文字列と補助関数を出力対象へ登録する。
    let global_initializers: Vec<_> = program
        .globals
        .iter()
        .map(|value| expressions::expression(value, &mut context))
        .collect();
    let global_temporaries = std::mem::take(&mut context.temporaries);
    let bodies: Vec<_> = program
        .functions
        .iter()
        .map(|function| functions::emit(function, &mut context))
        .collect();
    text_structure::prepare(&mut context);
    helpers::emit(&mut c, &context);
    structs::emit(&mut c, &context);
    functions::call_only_declarations(&mut c, &context);
    text_structure::emit(&mut c, &context);
    for function in &program.functions {
        writeln!(c, "{};", functions::declaration(function)).unwrap();
    }
    for (id, value) in program.globals.iter().enumerate() {
        writeln!(c, "static {} suz_global_{id};", value.ty.c_name()).unwrap();
    }
    c.push_str(&context.functions);
    for body in bodies {
        c.push_str(&body);
    }
    // 関数を呼ばずにアドレスだけを参照し、Suzで有効な未使用関数にCの未使用警告が出ないようにする。
    let references: String = program
        .functions
        .iter()
        .filter(|f| f.id != program.main_id)
        .map(|f| format!("    (void)suz_fn_{};\n", f.id))
        .collect();
    let mut text_init = if context.uses_text {
        format!(
            "    suz_text_pool_init(UINT64_C({}));\n",
            text_pool_limit.runtime_value()
        )
    } else {
        String::new()
    };
    for (value, id) in &context.text_literals {
        writeln!(text_init, "    suz_text_constant_{id} = suz_text_intern(suz_text_literal_{id}, UINT64_C({}), true, {});", value.len(), text::position(program.main_location, &context)).unwrap();
    }
    // symbol/text（構造体のフィールドも含む）の参照先を登録してから初期値を設定する。
    for (id, ty) in global_temporaries.iter().enumerate() {
        writeln!(text_init, "    {} suz_cmp_{id};", ty.c_name()).unwrap();
    }
    for (id, initializer) in global_initializers.iter().enumerate() {
        writeln!(text_init, "    suz_global_{id} = {initializer};").unwrap();
        writeln!(text_init, "    (void)suz_global_{id};").unwrap();
    }
    writeln!(
        c,
        "int main(void) {{\n    suz_runtime_init();\n{references}{text_init}    return (int)suz_fn_{}();\n}}",
        program.main_id
    )
    .unwrap();
    c
}
