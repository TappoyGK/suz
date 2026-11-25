//! 検査済み関数の宣言・定義・呼び出しを生成する。関数番号と引数型をCの名前と型へ写す。
//!
//! 利用元: [`crate::codegen`]。

use super::{ExpressionContext, expressions::expression, statements};
use crate::{
    ast::Type,
    check::{CheckedExpression, ir::CheckedFunction},
};
use std::fmt::Write;

/// 64-bit ABI（AAPCS64 / System V x86-64）では 16 バイト以下の構造体はレジスタ 2 本で値渡しされる。
/// 16 バイトを超える構造体だけをポインタ渡しにして、小さな構造体の再帰等での速度低下を避ける。
const SMALL_STRUCT_VALUE_LIMIT_BYTES: usize = 16;

fn align_up(offset: usize, align: usize) -> usize {
    let align = align.max(1);
    (offset + (align - 1)) & !(align - 1)
}

fn type_layout(ty: Type, context: &ExpressionContext) -> (usize, usize) {
    use crate::ast::{CIntegerType, FloatType, IntegerType};
    match ty {
        Type::Bool => (1, 1),
        Type::Char => (4, 4),
        Type::Integer(t) => match t {
            IntegerType::I8 | IntegerType::U8 => (1, 1),
            IntegerType::I16 | IntegerType::U16 => (2, 2),
            IntegerType::I32 | IntegerType::U32 | IntegerType::Len => (4, 4),
            IntegerType::I64 | IntegerType::U64 => (8, 8),
        },
        Type::Float(FloatType::F32) => (4, 4),
        Type::Float(FloatType::F64) => (8, 8),
        Type::Enum(e) => type_layout(Type::Integer(e.base), context),
        Type::CInteger(t) => match t {
            CIntegerType::Char => (1, 1),
            CIntegerType::Int | CIntegerType::Uint => (4, 4),
            CIntegerType::Long | CIntegerType::Ulong | CIntegerType::Size => (8, 8),
        },
        Type::Pointer { .. } | Type::CPointer | Type::Symbol | Type::StaticFunction(_) => (8, 8),
        Type::Function(_) => (16, 8),
        Type::Text => (32, 8),
        Type::ArrayRef { id, .. } => {
            let rank = context.arrays.get(id).rank;
            (align_up(8 + 4 * rank, 8), 8)
        }
        Type::Exact => (32, 16),
        Type::Struct(id) => {
            let mut offset = 0usize;
            let mut align = 1usize;
            for field in &context.structs[id].fields {
                let (field_size, field_align) = type_layout(field.ty, context);
                offset = align_up(offset, field_align);
                offset += field_size;
                align = align.max(field_align);
            }
            (align_up(offset, align), align)
        }
        Type::Void
        | Type::Array(_)
        | Type::Parameter(_)
        | Type::Application(_)
        | Type::StaticShape(_)
        | Type::Shape(_)
        | Type::GenericInt
        | Type::GenericFloat
        | Type::Numeric(_) => (0, 1),
    }
}

/// 構造体の値仮引数を C の読み取り専用ポインタで渡すべきか（16 バイト超の構造体のみ）。
pub(super) fn pass_struct_by_pointer(ty: Type, context: &ExpressionContext) -> bool {
    matches!(ty, Type::Struct(_)) && type_layout(ty, context).0 > SMALL_STRUCT_VALUE_LIMIT_BYTES
}

/// 16 バイト超の構造体の値仮引数は C では読み取り専用ポインタで受け取る。
pub(super) fn parameter_type(ty: Type, context: &ExpressionContext) -> String {
    if pass_struct_by_pointer(ty, context) {
        format!("const {} *restrict", ty.c_name())
    } else {
        ty.c_name()
    }
}

/// 呼び出し中に値が変わり得ない構造体の場所かを判定する。
/// fix束縛・値仮引数・値キャプチャーと、それらの構造体フィールド連鎖だけが該当する。
/// var束縛・ポインタ参照外し・参照キャプチャー・#メソッドのselfは含めない。
pub(super) fn immutable_struct_place(
    expr: &CheckedExpression,
    context: &ExpressionContext,
) -> bool {
    match &expr.value {
        crate::check::ir::Value::Local(id) => context.immutable_locals.contains(id),
        crate::check::ir::Value::Receiver {
            mutable: false,
            target,
        } => immutable_struct_place(target, context),
        crate::check::ir::Value::Field { target, .. } => {
            matches!(target.ty, Type::Struct(_)) && immutable_struct_place(target, context)
        }
        _ => false,
    }
}

pub(super) fn declaration(function: &CheckedFunction, context: &ExpressionContext) -> String {
    let mut parameters = function
        .parameters
        .iter()
        .enumerate()
        .map(|(id, ty)| {
            if id == 0 && function.mutable_receiver {
                format!("{} *const suz_ref_0", ty.c_name())
            } else if pass_struct_by_pointer(*ty, context) {
                format!("const {} *restrict suz_ref_{id}", ty.c_name())
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
    context.report_function.clone_from(&function.name);
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
    for (id, ty) in function.parameters.iter().enumerate() {
        if !(id == 0 && function.mutable_receiver) && pass_struct_by_pointer(*ty, context) {
            context.references.insert(id);
        }
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
    let mut c = format!("{} {{\n", declaration(function, context));
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

fn emit_argument(
    argument: &CheckedExpression,
    external: bool,
    setup: &mut Vec<String>,
    names: &mut Vec<String>,
    context: &mut ExpressionContext,
) {
    let by_reference = matches!(
        argument.value,
        crate::check::ir::Value::Receiver { mutable: true, .. }
    );
    if !external && !by_reference && pass_struct_by_pointer(argument.ty, context) {
        if immutable_struct_place(argument, context) {
            let value = expression(argument, context);
            names.push(format!("&({value})"));
            return;
        }
        let unwrapped = match &argument.value {
            crate::check::ir::Value::Receiver {
                mutable: false,
                target,
            } => target.as_ref(),
            _ => argument,
        };
        if let crate::check::ir::Value::With { target, fields } = &unwrapped.value {
            let index = context.temporaries.len();
            context.temporaries.push(argument.ty);
            let destination = format!("suz_cmp_{index}");
            setup.extend(super::structs::with_into(
                &destination,
                target,
                fields,
                context,
            ));
            names.push(format!("&{destination}"));
            return;
        }
        let index = context.temporaries.len();
        context.temporaries.push(argument.ty);
        let value = expression(argument, context);
        setup.push(format!("suz_cmp_{index} = {value}"));
        names.push(format!("&suz_cmp_{index}"));
        return;
    }
    let index = context.temporaries.len();
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

pub(super) fn call(
    id: usize,
    arguments: &[CheckedExpression],
    context: &mut ExpressionContext,
    error: Option<&str>,
) -> String {
    let external = context.external_ids.contains(&id);
    let mut setup = Vec::new();
    let mut names = Vec::new();
    for argument in arguments {
        emit_argument(argument, external, &mut setup, &mut names, context);
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
        emit_argument(argument, false, &mut setup, &mut names, context);
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
                .map(|t| parameter_type(*t, context))
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
            arguments.extend(ty.parameters.iter().map(|ty| parameter_type(*ty, context)));
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
            .map(|(i, ty)| format!("{} argument_{i}", parameter_type(*ty, context)))
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
