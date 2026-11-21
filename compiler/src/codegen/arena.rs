//! 初期値を評価してからarenaに確保し、格納後のポインタを返す。
use super::{
    ExpressionContext,
    expressions::{c_string, expression},
};
use crate::{
    ast::{Location, Type},
    check::CheckedExpression,
};

pub(super) fn allocate(
    value: &CheckedExpression,
    ty: Type,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    if matches!(value.value, crate::check::Value::Array { .. }) {
        return super::arrays::allocate(value, ty, location, context);
    }
    context.uses_arena = true;
    let initial = context.temporaries.len();
    context.temporaries.push(value.ty);
    let pointer = context.temporaries.len();
    context.temporaries.push(ty);
    let value = expression(value, context);
    format!(
        "(suz_cmp_{initial} = {value}, suz_cmp_{pointer} = suz_arena_alloc(sizeof(suz_cmp_{initial}), alignof(typeof(suz_cmp_{initial})), {}, {}, {}), *suz_cmp_{pointer} = suz_cmp_{initial}, suz_cmp_{pointer})",
        c_string(&context.source_name),
        location.line,
        location.column
    )
}
