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
        .map(|(id, ty)| {
            if id == 0 && function.mutable_receiver {
                format!("{} *const suz_ref_0", ty.c_name())
            } else {
                format!("const {} suz_local_{id}", ty.c_name())
            }
        })
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
    context.immutable_locals = function
        .parameters
        .iter()
        .enumerate()
        .filter(|(id, _)| !(*id == 0 && function.mutable_receiver))
        .map(|(id, _)| id)
        .collect();
    context.references.clear();
    if function.mutable_receiver {
        context.references.insert(0);
    }
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
    for (id, &ty) in function.parameters.iter().enumerate() {
        // Suzでは未使用の引数を認めるため、Cコンパイラの警告を抑える。
        writeln!(c, "    (void){};", context.local(id)).unwrap();
        if ty.array_id().is_some() && context.immutable_locals.contains(&id) {
            super::arrays::cache(&mut c, id, ty, "    ", context);
        }
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
        let by_reference = matches!(
            argument.value,
            crate::check::ir::Value::Receiver { mutable: true, .. }
        );
        context.temporaries.push(if by_reference {
            argument.ty.pointer()
        } else {
            argument.ty
        });
        let name = format!("suz_cmp_{index}");
        let value = expression(argument, context);
        setup.push(format!(
            "{name} = {}",
            if by_reference {
                format!("&({value})")
            } else {
                value
            }
        ));
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
/// 再帰する構造体のために予約した番号もあるので、関数型の依存先から出力する。
pub(super) fn type_declarations(c: &mut String, context: &ExpressionContext) {
    let mut dependencies = vec![0; context.function_types.len()];
    let mut parents = vec![Vec::new(); context.function_types.len()];
    let mut symbolic = vec![false; context.function_types.len()];
    for (id, ty) in context.function_types.iter().enumerate() {
        for ty in ty.parameters.iter().chain([&ty.result]) {
            match ty.leaf() {
                Type::StaticFunction(dependency) | Type::Function(dependency) => {
                    dependencies[id] += 1;
                    parents[dependency].push(id);
                }
                Type::Parameter(_) => symbolic[id] = true,
                Type::Struct(id_struct) => symbolic[id] |= context.structs[id_struct].symbolic,
                ty if ty.symbolic() => symbolic[id] = true,
                _ => {}
            }
        }
    }
    let mut ready: std::collections::VecDeque<_> = (0..dependencies.len())
        .filter(|id| dependencies[*id] == 0)
        .collect();
    while let Some(id) = ready.pop_front() {
        for &parent in &parents[id] {
            symbolic[parent] |= symbolic[id];
            dependencies[parent] -= 1;
            if dependencies[parent] == 0 {
                ready.push_back(parent);
            }
        }
        // 定義検査用の記号的な署名には、C側の型を作らない。
        if symbolic[id] {
            continue;
        }
        let ty = &context.function_types[id];
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
