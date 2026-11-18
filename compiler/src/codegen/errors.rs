//! 失敗する呼び出しではエラー番号の出力引数を渡す。成功値の表現は通常の関数と同じ。
//! 脱出するハンドラーは文で、代替値は条件式で生成し、失敗時だけ評価する。
use super::{ExpressionContext, expressions::expression as value};
use crate::{
    ast::{IntegerType, Type},
    check::ir::{CallHandler, CheckedExpression, Value},
};
use std::fmt::Write;

fn call(call: &CheckedExpression, context: &mut ExpressionContext) -> (String, String) {
    let error = format!("suz_cmp_{}", context.temporaries.len());
    context.temporaries.push(Type::Integer(IntegerType::U64));
    let code = match &call.value {
        Value::Call { id, arguments } => {
            super::functions::call(*id, arguments, context, Some(&error))
        }
        Value::Invoke { target, arguments } => {
            super::functions::invoke(target, arguments, context, Some(&error))
        }
        _ => unreachable!(),
    };
    (code, error)
}

pub(super) fn expression(
    call_value: &CheckedExpression,
    handler: &CallHandler,
    context: &mut ExpressionContext,
) -> String {
    let (call, error) = call(call_value, context);
    let CallHandler::Value(fallback) = handler else {
        assert!(matches!(handler, CallHandler::Ignore));
        return call;
    };
    let fallback = value(fallback, context);
    if call_value.ty == Type::Void {
        return format!("({call}, {error} ? (void)({fallback}) : (void)0)");
    }
    let result = format!("suz_cmp_{}", context.temporaries.len());
    context.temporaries.push(call_value.ty);
    format!("({result} = {call}, {error} ? ({fallback}) : {result})")
}

pub(super) fn fail(
    error: Option<&CheckedExpression>,
    propagated: &str,
    context: &mut ExpressionContext,
) -> String {
    let error = match error {
        Some(error) => {
            let Type::Enum(ty) = error.ty else {
                unreachable!()
            };
            // 0は成功。型ごとの連続範囲へ写し、異なるerrorの同名バリアントを区別する。
            let offset = 1 + context.enums[..ty.id]
                .iter()
                .map(|e| e.ty.count as u64)
                .sum::<u64>();
            format!("(uint64_t)({}) + UINT64_C({offset})", value(error, context))
        }
        None => propagated.to_owned(),
    };
    let result = context.function_result.unwrap();
    let returned = if result == Type::Void {
        String::new()
    } else {
        format!("({}){{0}}", result.c_name())
    };
    format!("*suz_error = {error}; return {returned};")
}

pub(super) fn statement(
    c: &mut String,
    id: usize,
    call_value: &CheckedExpression,
    handler: &CallHandler,
    indent: &str,
    context: &mut ExpressionContext,
) {
    let (call, error) = call(call_value, context);
    if call_value.ty == Type::Void {
        writeln!(c, "{indent}{call};").unwrap();
    } else {
        writeln!(
            c,
            "{indent}const {} suz_local_{id} = {call};\n{indent}(void)suz_local_{id};",
            call_value.ty.c_name()
        )
        .unwrap();
    }
    let branch = match handler {
        CallHandler::Return(v) => format!("return {};", value(v, context)),
        CallHandler::Fail(v) => fail(v.as_ref(), &error, context),
        _ => unreachable!(),
    };
    writeln!(c, "{indent}if ({error}) {{ {branch} }}").unwrap();
}
