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

use crate::check::CheckedProgram;
use std::{collections::BTreeSet, fmt::Write};

#[derive(Default)]
struct ExpressionContext {
    structs: Vec<crate::ast::StructDeclaration>,
    struct_equals: BTreeSet<usize>,
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

pub(crate) fn generate(program: &CheckedProgram, source_name: &str) -> String {
    let mut c = String::from(
        r#"#include <limits.h>
#include <stdint.h>
#include "suz_runtime.h"

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
    helpers::emit(&mut c, &context);
    structs::emit(&mut c, &context);
    for function in &program.functions {
        writeln!(c, "{};", functions::declaration(function)).unwrap();
    }
    c.push_str(&context.functions);
    for body in bodies {
        c.push_str(&body);
    }
    writeln!(
        c,
        "int main(void) {{\n    suz_runtime_init();\n    return (int)suz_fn_{}();\n}}",
        program.main_id
    )
    .unwrap();
    c
}
