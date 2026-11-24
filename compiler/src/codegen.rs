//! 検査済みIRからC23のソースを生成する。補助関数・定数・関数本体を出力順に組み立てる。
//!
//! 利用元: [`crate::compile_with_options`]。

mod arena;
mod arithmetic;
mod arrays;
mod blocks;
mod casts;
mod characters;
mod closures;
mod errors;
mod exact;
mod expressions;
mod ffi;
mod functions;
mod helpers;
mod logs;
mod slices;
mod statements;
mod structs;
mod text;
mod text_structure;

use crate::check::CheckedProgram;
use std::{collections::BTreeSet, fmt::Write};

struct RecoveryTarget {
    result: Option<String>,
    label: String,
    used: bool,
}

#[derive(Default)]
struct ExpressionContext {
    uses_arena: bool,
    report_function: String,
    report_sites: std::collections::BTreeMap<(usize, usize), (usize, String)>,
    arrays: crate::ast::arrays::Types,
    constant_arrays: std::collections::BTreeMap<String, usize>,
    constant_array_data: String,
    constant_array_init: String,
    uses_c_byte_len: bool,
    uses_c_integer: bool,
    uses_c_pointer: bool,
    c_literal_checks: BTreeSet<String>,
    log_level: crate::LogLevel,
    log_ignore: bool,
    arena_report: Option<crate::ArenaReportOptions>,
    function_result: Option<crate::ast::Type>,
    text_literals: std::collections::BTreeMap<String, usize>,
    uses_text: bool,
    structs: Vec<crate::ast::StructDeclaration>,
    external_ids: std::ops::Range<usize>,
    function_types: Vec<crate::ast::FunctionType>,
    call_only_types: BTreeSet<usize>,
    function_adapters: BTreeSet<usize>,
    struct_equals: BTreeSet<usize>,
    struct_parsers: BTreeSet<usize>,
    enums: Vec<crate::ast::EnumDeclaration>,
    enum_logs: BTreeSet<usize>,
    functions: String,
    next_block: usize,
    next_recovery: usize,
    recovery: Option<RecoveryTarget>,
    next_closure: usize,
    closure_environments: Vec<String>,
    references: BTreeSet<usize>,
    immutable_locals: BTreeSet<usize>,
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
    symbol_pool_limit: crate::SymbolPoolLimit,
    arena: crate::ArenaOptions,
    log_level: crate::LogLevel,
    log_ignore: bool,
    arena_report: Option<crate::ArenaReportOptions>,
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
    for (name, value) in [
        ("INITIAL", arena.initial),
        ("GROW", arena.grow),
        ("LIMIT", arena.limit.map_or(0, |n| n.get())),
    ] {
        writeln!(c, "#define SUZ_ARENA_{name} UINT64_C({value})").unwrap();
    }
    let mut context = ExpressionContext {
        log_level,
        log_ignore,
        arena_report,
        arrays: program.arrays.clone(),
        source_name: source_name.into(),
        enums: program.enums.clone(),
        structs: program.structs.clone(),
        external_ids: program.external_base..program.external_base + program.externs.len(),
        function_types: program.function_types.clone(),
        // 未使用のfnshapeもCへ宣言するため、その仮引数が参照する組の型も用意する。
        call_only_types: program
            .function_types
            .iter()
            .flat_map(|ty| &ty.parameters)
            .filter_map(|ty| match ty {
                crate::ast::Type::Function(id) => Some(*id),
                _ => None,
            })
            .collect(),
        ..ExpressionContext::default()
    };
    // logの埋め込みは名前とフィールド参照だけ。ここには独立した宣言・式が必要とする
    // symbol定数が入るため維持する。ログ本文と表示用の名前は、文の生成時だけ登録する。
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
    let bodies: Vec<_> = program
        .functions
        .iter()
        .map(|function| functions::emit(function, &mut context))
        .collect();
    if context.uses_arena {
        if let Some(report) = arena_report {
            c.push_str("#define SUZ_LOG_ARENA 1\n");
            if report.format != crate::ArenaReportFormat::Summary {
                let format = match report.format {
                    crate::ArenaReportFormat::Tree => "SUZ_ARENA_REPORT_TREE",
                    crate::ArenaReportFormat::TreeBytes => "SUZ_ARENA_REPORT_TREE_BYTES",
                    crate::ArenaReportFormat::Csv => "SUZ_ARENA_REPORT_CSV",
                    crate::ArenaReportFormat::Summary => unreachable!(),
                };
                writeln!(c, "#define SUZ_ARENA_REPORT_FORMAT {format}\n#define SUZ_ARENA_REPORT_CAPACITY {}", report.capacity).unwrap();
            }
        }
        c.push_str("#include \"suz_arena.h\"\n#include \"suz_textbuf.h\"\n");
    }
    for (&(line, column), (id, function)) in &context.report_sites {
        writeln!(c, "static suz_arena_report_site suz_arena_report_site_{id} = {{ .id = {id}, .source = {}, .line = {line}, .column = {column}, .function = {}, .cached_parent = SIZE_MAX }};",
            expressions::c_string(source_name), expressions::c_string(function)).unwrap();
    }
    arrays::declarations(&mut c, &context);
    ffi::checks(&mut c, program, &context);
    ffi::declarations(&mut c, program);
    text_structure::prepare(&mut context);
    helpers::emit(&mut c, &context);
    structs::emit(&mut c, &context);
    functions::call_only_declarations(&mut c, &context);
    text_structure::emit(&mut c, &context);
    for function in &program.functions {
        writeln!(c, "{};", functions::declaration(function)).unwrap();
    }
    c.push_str(&context.constant_array_data);
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
            "    suz_symbol_pool_init(UINT64_C({}));\n",
            symbol_pool_limit.runtime_value()
        )
    } else {
        String::new()
    };
    if context.uses_arena {
        text_init.insert_str(
            0,
            &format!(
                "    suz_arena_init(SUZ_ARENA_INITIAL, SUZ_ARENA_GROW, SUZ_ARENA_LIMIT, {}, {}, {});\n",
                expressions::c_string(source_name),
                program.main_location.line,
                program.main_location.column
            ),
        );
    }
    for (value, id) in &context.text_literals {
        writeln!(text_init, "    suz_text_constant_{id} = suz_text_intern(suz_text_literal_{id}, UINT64_C({}), true, {});", value.len(), text::position(program.main_location, &context)).unwrap();
    }
    text_init.push_str(&context.constant_array_init);
    let report = if arena_report.is_some() {
        "    suz_arena_report();\n"
    } else {
        ""
    };
    let exit = if context.uses_arena {
        format!(
            "    int result = (int)suz_fn_{}();\n{report}    suz_arena_destroy();\n    return result;",
            program.main_id
        )
    } else {
        format!("    return (int)suz_fn_{}();", program.main_id)
    };
    writeln!(
        c,
        "int main(void) {{\n    suz_runtime_init();\n{references}{text_init}{exit}\n}}"
    )
    .unwrap();
    c
}

#[cfg(test)]
mod tests;
