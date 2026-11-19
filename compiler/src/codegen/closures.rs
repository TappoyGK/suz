//! クロージャを環境引数付きのC関数にし、評価位置で値・変数の位置をスタック上の環境へ保持する。
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
            if capture.by_reference {
                format!("&({})", expression(&capture.value, context))
            } else {
                expression(&capture.value, context)
            }
        ));
    }
    let saved_recovery = context.recovery.take();
    let saved_temporaries = std::mem::take(&mut context.temporaries);
    let saved_environments = std::mem::take(&mut context.closure_environments);
    let saved_references = std::mem::take(&mut context.references);
    // 値ブロックと同じ参照表現を使い、読み書きと再キャプチャーが外側の変数へ届くようにする。
    context.references = closure
        .captures
        .iter()
        .enumerate()
        .filter(|(_, c)| c.by_reference)
        .map(|(i, _)| parameter_count + i)
        .collect();
    let saved_result = context.function_result.replace(function.result);
    let mut body = if function.fallible {
        "    *suz_error = 0;\n".into()
    } else {
        String::new()
    };
    emit_block(&mut body, &function.statements, 1, &mut 0, context);
    let mut definition = String::new();
    if !closure.captures.is_empty() {
        writeln!(definition, "typedef struct {{").unwrap();
        for (index, capture) in closure.captures.iter().enumerate() {
            writeln!(
                definition,
                "    {} {}capture_{index};",
                capture.value.ty.c_name(),
                if capture.by_reference { "*" } else { "" }
            )
            .unwrap();
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
    if function.fallible {
        parameters.push("uint64_t *suz_error".into());
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
        let local = parameter_count + index;
        if capture.by_reference {
            writeln!(definition, "    {} *const suz_ref_{local} = ((const suz_closure_env_{id} *)environment)->capture_{index};", capture.value.ty.c_name()).unwrap();
        } else {
            writeln!(definition, "    const {} suz_local_{local} = ((const suz_closure_env_{id} *)environment)->capture_{index};", capture.value.ty.c_name()).unwrap();
        }
    }
    for index in 0..function.parameters.len() {
        writeln!(definition, "    (void){};", context.local(index)).unwrap();
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
    context.function_result = saved_result;
    context.recovery = saved_recovery;
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
