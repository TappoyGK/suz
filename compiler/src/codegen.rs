mod arithmetic;
mod blocks;
mod casts;
mod expressions;
mod functions;
mod helpers;
mod logs;
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
        ..ExpressionContext::default()
    };
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
    text_structure::prepare(&mut context);
    helpers::emit(&mut c, &context);
    structs::emit(&mut c, &context);
    text_structure::emit(&mut c, &context);
    for function in &program.functions {
        writeln!(c, "{};", functions::declaration(function)).unwrap();
    }
    c.push_str(&context.functions);
    for body in bodies {
        c.push_str(&body);
    }
    // Mention function addresses without calling them, so valid unused Suz
    // functions do not trigger C's unused-function warning.
    let references: String = program
        .functions
        .iter()
        .filter(|f| f.id != program.main_id)
        .map(|f| format!("    (void)suz_fn_{};\n", f.id))
        .collect();
    let text_init = if context.uses_text {
        format!(
            "    suz_text_pool_init(UINT64_C({}));\n",
            text_pool_limit.runtime_value()
        )
    } else {
        String::new()
    };
    writeln!(
        c,
        "int main(void) {{\n    suz_runtime_init();\n{references}{text_init}    return (int)suz_fn_{}();\n}}",
        program.main_id
    )
    .unwrap();
    c
}
