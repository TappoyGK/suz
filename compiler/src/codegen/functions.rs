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
    let mut parameters = function
        .parameters
        .iter()
        .enumerate()
        .map(|(id, ty)| format!("const {} suz_local_{id}", ty.c_name()))
        .collect::<Vec<_>>()
        .join(", ");
    if function.fallible {
        if !parameters.is_empty() {
            parameters.push_str(", ");
        }
        parameters.push_str("uint64_t *suz_error");
    }
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
    context.function_result = Some(function.result);
    context.temporaries.clear();
    context.closure_environments.clear();
    for ty in function.parameters.iter().chain([&function.result]) {
        if let Type::Function(id) = ty {
            context.call_only_types.insert(*id);
        }
        if let Type::Float(ty) = ty {
            context.float_types.insert(*ty);
        }
    }
    let mut body = if function.fallible {
        "    *suz_error = 0;\n".into()
    } else {
        String::new()
    };
    statements::emit_block(&mut body, &function.statements, 1, &mut 0, context);
    let mut c = format!("{} {{\n", declaration(function));
    for id in 0..function.parameters.len() {
        // Suzでは未使用の引数を認めるため、Cコンパイラの警告を抑える。
        writeln!(c, "    (void)suz_local_{id};").unwrap();
    }
    for (index, ty) in context.temporaries.iter().enumerate() {
        writeln!(c, "    {} suz_cmp_{index};", ty.c_name()).unwrap();
    }
    for declaration in &context.closure_environments {
        writeln!(c, "    {declaration}").unwrap();
    }
    c.push_str(&body);
    c.push_str("}\n\n");
    c
}

pub(super) fn call(
    id: usize,
    arguments: &[CheckedExpression],
    context: &mut ExpressionContext,
    error: Option<&str>,
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
    if let Some(error) = error {
        names.push(format!("&{error}"));
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
    error: Option<&str>,
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
    if let Some(error) = error {
        names.push(format!("&{error}"));
    }
    if matches!(target.ty, Type::Function(_)) {
        names.insert(0, format!("suz_cmp_{id}.environment"));
        setup.push(format!("suz_cmp_{id}.call({})", names.join(", ")));
    } else {
        setup.push(format!("suz_cmp_{id}({})", names.join(", ")));
    }
    format!("({})", setup.join(", "))
}

/// 構造体の前方宣言の後、本体の前に関数ポインタ型を宣言する。
/// 関数値の型の番号は依存先から付くため、ここでは番号順に出力できる。
pub(super) fn type_declarations(c: &mut String, context: &ExpressionContext) {
    for (id, ty) in context.function_types.iter().enumerate() {
        let mut args = if ty.parameters.is_empty() {
            "void".into()
        } else {
            ty.parameters
                .iter()
                .map(|t| t.c_name())
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !ty.errors.is_empty() {
            if args == "void" {
                args.clear();
            } else {
                args.push_str(", ");
            }
            args.push_str("uint64_t *");
        }
        writeln!(
            c,
            "typedef {} (*suz_fntype_{id})({args});",
            ty.result.c_name()
        )
        .unwrap();
        if context.call_only_types.contains(&id) {
            let mut arguments = vec!["void *".to_owned()];
            arguments.extend(ty.parameters.iter().map(|ty| ty.c_name()));
            if !ty.errors.is_empty() {
                arguments.push("uint64_t *".into());
            }
            writeln!(
                c,
                "typedef struct {{ {} (*call)({}); void *environment; }} suz_callback_{id};",
                ty.result.c_name(),
                arguments.join(", ")
            )
            .unwrap();
        }
    }
}

/// 評価済みの関数ポインタをスタックの一時変数に保持する。
/// 仮引数は保存・返却できないため、その呼び出しと転送先の実行中はこの環境が生きている。
pub(super) fn borrow(value: &CheckedExpression, context: &mut ExpressionContext) -> String {
    let Type::StaticFunction(id) = value.ty else {
        unreachable!()
    };
    context.call_only_types.insert(id);
    context.function_adapters.insert(id);
    let temporary = context.temporaries.len();
    context.temporaries.push(value.ty);
    let value = expression(value, context);
    format!(
        "(suz_cmp_{temporary} = {value}, (suz_callback_{id}){{ suz_adapt_{id}, &suz_cmp_{temporary} }})"
    )
}

/// 関数ポインタ型と構造体の定義後に、必要な包み関数を出力する。
/// この型を使わないプログラムには宣言を追加しない。
pub(super) fn call_only_declarations(c: &mut String, context: &ExpressionContext) {
    for &id in &context.function_adapters {
        let ty = &context.function_types[id];
        let result = ty.result.c_name();
        let parameters: Vec<_> = ty
            .parameters
            .iter()
            .enumerate()
            .map(|(i, ty)| format!("{} argument_{i}", ty.c_name()))
            .collect();
        let mut with_environment = vec!["void *environment".to_owned()];
        with_environment.extend(parameters);
        if !ty.errors.is_empty() {
            with_environment.push("uint64_t *suz_error".into());
        }
        let parameters = with_environment.join(", ");
        let mut arguments = (0..ty.parameters.len())
            .map(|i| format!("argument_{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        if !ty.errors.is_empty() {
            if !arguments.is_empty() {
                arguments.push_str(", ");
            }
            arguments.push_str("suz_error");
        }
        let return_value = if ty.result == Type::Void {
            ""
        } else {
            "return "
        };
        writeln!(c, "static inline {result} suz_adapt_{id}({parameters}) {{\n    {return_value}(*(suz_fntype_{id} *)environment)({arguments});\n}}").unwrap();
    }
}
