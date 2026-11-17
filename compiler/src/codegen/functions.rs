//! 検査済み関数の宣言・定義・呼び出しを生成する。関数番号と引数型をCの名前と型へ写す。
//!
//! 利用元: [`crate::codegen`]。

use super::{ExpressionContext, expressions::expression, statements};
use crate::{
    ast::Type,
    check::{CheckedExpression, ir::CheckedFunction},
};
use std::fmt::Write;

pub(super) fn declaration(function: &CheckedFunction) -> String {
    let parameters = function
        .parameters
        .iter()
        .enumerate()
        .map(|(id, ty)| format!("const {} suz_local_{id}", ty.c_name()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "static inline {} suz_fn_{}({})",
        function.result.c_name(),
        function.id,
        if parameters.is_empty() {
            "void"
        } else {
            &parameters
        }
    )
}

pub(super) fn emit(function: &CheckedFunction, context: &mut ExpressionContext) -> String {
    context.temporaries.clear();
    for ty in function.parameters.iter().chain([&function.result]) {
        if let Type::Float(ty) = ty {
            context.float_types.insert(*ty);
        }
    }
    let mut body = String::new();
    statements::emit_block(&mut body, &function.statements, 1, &mut 0, context);
    let mut c = format!("{} {{\n", declaration(function));
    for id in 0..function.parameters.len() {
        // Suzでは未使用の引数を認めるため、Cコンパイラの警告を抑える。
        writeln!(c, "    (void)suz_local_{id};").unwrap();
    }
    for (index, ty) in context.temporaries.iter().enumerate() {
        writeln!(c, "    {} suz_cmp_{index};", ty.c_name()).unwrap();
    }
    c.push_str(&body);
    c.push_str("}\n\n");
    c
}

pub(super) fn call(
    id: usize,
    arguments: &[CheckedExpression],
    context: &mut ExpressionContext,
) -> String {
    let mut setup = Vec::new();
    let mut names = Vec::new();
    for argument in arguments {
        let index = context.temporaries.len();
        context.temporaries.push(argument.ty);
        let name = format!("suz_cmp_{index}");
        setup.push(format!("{name} = {}", expression(argument, context)));
        names.push(name);
    }
    let call = format!("suz_fn_{id}({})", names.join(", "));
    if setup.is_empty() {
        call
    } else {
        setup.push(call);
        format!("({})", setup.join(", "))
    }
}

/// 呼び出す関数を実引数より先に保存し、引数による変数の書き換えの影響を避ける。
pub(super) fn invoke(
    target: &CheckedExpression,
    arguments: &[CheckedExpression],
    context: &mut ExpressionContext,
) -> String {
    let id = context.temporaries.len();
    context.temporaries.push(target.ty);
    let mut setup = vec![format!("suz_cmp_{id} = {}", expression(target, context))];
    let mut names = Vec::new();
    for argument in arguments {
        let index = context.temporaries.len();
        context.temporaries.push(argument.ty);
        setup.push(format!(
            "suz_cmp_{index} = {}",
            expression(argument, context)
        ));
        names.push(format!("suz_cmp_{index}"));
    }
    setup.push(format!("suz_cmp_{id}({})", names.join(", ")));
    format!("({})", setup.join(", "))
}

/// 構造体の前方宣言の後、本体の前に関数ポインタ型を宣言する。
/// 関数値の型の番号は依存先から付くため、ここでは番号順に出力できる。
pub(super) fn type_declarations(c: &mut String, types: &[crate::ast::FunctionType]) {
    for (id, ty) in types.iter().enumerate() {
        let args = if ty.parameters.is_empty() {
            "void".into()
        } else {
            ty.parameters
                .iter()
                .map(|t| t.c_name())
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(
            c,
            "typedef {} (*suz_fntype_{id})({args});",
            ty.result.c_name()
        )
        .unwrap();
    }
}
