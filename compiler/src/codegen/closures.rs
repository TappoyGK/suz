//! クロージャを環境引数付きのC関数にし、評価位置で値をスタック上の環境へ写す。
//! 環境を呼び出しの外へ保存できないため、ループでも同じスタック領域を再利用できる。

use super::{ExpressionContext, expressions::expression, statements::emit_block};
use crate::{ast::Type, check::ir::CheckedClosure};
use std::fmt::Write;

pub(super) fn emit(closure: &CheckedClosure, ty: Type, context: &mut ExpressionContext) -> String {
    let Type::Function(shape) = ty else {
        unreachable!()
    };
    context.call_only_types.insert(shape);
    let id = context.next_closure;
    context.next_closure += 1;
    let function = &closure.function;
    for ty in function.parameters.iter().chain([&function.result]) {
        if let Type::Float(ty) = ty {
            context.float_types.insert(*ty);
        }
    }
    let parameter_count = function.parameters.len() - closure.captures.len();
    let mut setup = Vec::new();
    for (index, capture) in closure.captures.iter().enumerate() {
        setup.push(format!(
            "suz_env_{id}.capture_{index} = {}",
            expression(capture, context)
        ));
    }
    let saved_temporaries = std::mem::take(&mut context.temporaries);
    let saved_environments = std::mem::take(&mut context.closure_environments);
    let saved_references = std::mem::take(&mut context.references);
    let mut body = String::new();
    emit_block(&mut body, &function.statements, 1, &mut 0, context);
    let mut definition = String::new();
    if !closure.captures.is_empty() {
        writeln!(definition, "typedef struct {{").unwrap();
        for (index, capture) in closure.captures.iter().enumerate() {
            writeln!(definition, "    {} capture_{index};", capture.ty.c_name()).unwrap();
        }
        writeln!(definition, "}} suz_closure_env_{id};").unwrap();
    }
    let mut parameters = vec!["void *environment".to_owned()];
    for (index, ty) in function.parameters[..parameter_count].iter().enumerate() {
        if let Type::Function(shape) = ty {
            context.call_only_types.insert(*shape);
        }
        parameters.push(format!("const {} suz_local_{index}", ty.c_name()));
    }
    writeln!(
        definition,
        "static {} suz_closure_{id}({}) {{",
        function.result.c_name(),
        parameters.join(", ")
    )
    .unwrap();
    definition.push_str("    (void)environment;\n");
    for (index, capture) in closure.captures.iter().enumerate() {
        writeln!(definition, "    const {} suz_local_{} = ((const suz_closure_env_{id} *)environment)->capture_{index};", capture.ty.c_name(), parameter_count + index).unwrap();
    }
    for index in 0..function.parameters.len() {
        writeln!(definition, "    (void)suz_local_{index};").unwrap();
    }
    for (index, ty) in context.temporaries.iter().enumerate() {
        writeln!(definition, "    {} suz_cmp_{index};", ty.c_name()).unwrap();
    }
    for declaration in &context.closure_environments {
        writeln!(definition, "    {declaration}").unwrap();
    }
    definition.push_str(&body);
    definition.push_str("}\n\n");
    // 入れ子のクロージャと値ブロックは、外側の本体より先に出力済み。
    context.functions.push_str(&definition);
    context.temporaries = saved_temporaries;
    context.closure_environments = saved_environments;
    context.references = saved_references;
    let environment = if closure.captures.is_empty() {
        "NULL".to_owned()
    } else {
        context
            .closure_environments
            .push(format!("suz_closure_env_{id} suz_env_{id};"));
        format!("&suz_env_{id}")
    };
    setup.push(format!(
        "(suz_callback_{shape}){{ suz_closure_{id}, {environment} }}"
    ));
    format!("({})", setup.join(", "))
}
