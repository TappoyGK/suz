//! 配列参照は実体の先頭と各次元の長さを持つ。確保前の評価と添字の短絡順序をここで保つ。
use super::{
    ExpressionContext,
    expressions::{c_string, expression},
};
use crate::{
    ast::{Location, Type},
    check::{CheckedExpression, Value},
};
use std::fmt::Write;

pub(super) fn declarations(c: &mut String, context: &ExpressionContext) {
    for (id, array) in context.arrays.entries.iter().enumerate() {
        writeln!(
            c,
            "typedef struct {{ void *data; int32_t lengths[{}]; }} suz_array_{id};",
            array.rank
        )
        .unwrap();
    }
}

fn temporary(ty: Type, context: &mut ExpressionContext) -> String {
    let id = context.temporaries.len();
    context.temporaries.push(ty);
    format!("suz_cmp_{id}")
}

fn panic(location: Location, reason: &str, context: &ExpressionContext) -> String {
    format!(
        "suz_panic({}, {}, {}, {})",
        c_string(&context.source_name),
        location.line,
        location.column,
        c_string(reason)
    )
}

/// 値と寸法をすべて評価した後に配置を検証する。ゼロ長でも初期値の評価は省略しない。
pub(super) fn allocate(
    value: &CheckedExpression,
    ty: Type,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    let Value::Array {
        values,
        lengths,
        repeated,
        constant_counts,
    } = &value.value
    else {
        unreachable!()
    };
    context.uses_arena = true;
    let element = context.arrays.get(ty.array_id().unwrap()).element.c_name();
    let mut c = String::from("({ ");
    for count in constant_counts {
        write!(c, r#"_Static_assert(UINT64_C({count}) <= SIZE_MAX / sizeof({element}), "array byte size overflows size_t [array.layout]"); "#).unwrap();
    }
    for (i, value) in values.iter().enumerate() {
        write!(
            c,
            "{element} suz_initial_{i} = {}; ",
            expression(value, context)
        )
        .unwrap();
    }
    // 寸法の検証より先に、各寸法の式を一度ずつ左から評価する。
    for (i, length) in lengths.iter().enumerate() {
        write!(
            c,
            "{} suz_length_{i} = {}; ",
            length.ty.c_name(),
            expression(length, context)
        )
        .unwrap();
    }
    write!(c, "{} suz_array_value; ", ty.c_name()).unwrap();

    for (i, length) in lengths.iter().enumerate() {
        let invalid = panic(
            location,
            &format!("array dimension {} is outside len_t [rt.array-size]", i + 1),
            context,
        );
        let negative = if length.ty.integer_bounds().unwrap().0 < 0 {
            format!("suz_length_{i} < 0 || ")
        } else {
            String::new()
        };
        write!(c, "if ({negative}(uint64_t)suz_length_{i} > INT32_MAX) {invalid}; suz_array_value.lengths[{i}] = (int32_t)suz_length_{i}; ").unwrap();
    }
    c.push_str("size_t suz_count = 1; ");
    let overflow = panic(
        location,
        "array layout overflows size_t [rt.array-size]",
        context,
    );
    // 外側がゼロでも、内側の部分配列の配置が表現可能であることを保証する。
    for i in (0..lengths.len()).rev() {
        write!(c, "if (suz_length_{i} != 0 && suz_count > SIZE_MAX / (size_t)suz_length_{i}) {overflow}; suz_count *= (size_t)suz_length_{i}; if (suz_count > SIZE_MAX / sizeof({element})) {overflow}; ").unwrap();
    }
    write!(c, "size_t suz_bytes = suz_count * sizeof({element}); suz_array_value.data = suz_arena_alloc(suz_bytes ? suz_bytes : 1, alignof({element}), {}, {}, {}); ", c_string(&context.source_name), location.line, location.column).unwrap();
    if *repeated {
        write!(c, "for (size_t suz_i = 0; suz_i < suz_count; ++suz_i) (({element} *)suz_array_value.data)[suz_i] = suz_initial_0; ").unwrap();
    } else {
        for i in 0..values.len() {
            write!(
                c,
                "(({element} *)suz_array_value.data)[{i}] = suz_initial_{i}; "
            )
            .unwrap();
        }
    }
    c.push_str("suz_array_value; })");
    c
}

/// 失敗した添字より後の式を実行せず、全検査に成功したときだけ要素のアドレスを作る。
fn parts(
    expr: &CheckedExpression,
    write: bool,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    let Value::ArrayElement { target, indices } = &expr.value else {
        unreachable!()
    };
    let source = temporary(target.ty, context);
    let mut tests = Vec::new();
    let setup = if let Value::ArrayRead {
        address,
        handler: None,
    } = &target.value
    {
        let (setup, valid, pointer) = parts(address, write, context);
        tests.push(format!("({valid} && ({source} = *{pointer}, true))"));
        setup
    } else {
        format!("{source} = {}", expression(target, context))
    };
    let mut offset = String::from("0");
    for (dimension, (index, location)) in indices.iter().enumerate() {
        let local = temporary(index.ty, context);
        let negative = if index.ty.integer_bounds().unwrap().0 < 0 {
            format!("{local} >= 0 && ")
        } else {
            String::new()
        };
        let check = format!(
            "({local} = {}, {negative}(uint64_t){local} < (uint64_t){source}.lengths[{dimension}])",
            expression(index, context)
        );
        tests.push(if write {
            context.uses_arena = true;
            let negative = if index.ty.integer_bounds().unwrap().0 < 0 { format!("{local} < 0") } else { "false".into() };
            let fail = format!("suz_array_bounds({}, {}, {}, {}, (uint64_t){local}, {negative}, {source}.lengths[{dimension}])", c_string(&context.source_name), location.line, location.column, dimension + 1);
            format!("({check} || ({fail}, false))")
        } else { check });
        offset = format!("({offset} * (size_t){source}.lengths[{dimension}] + (size_t){local})");
    }
    let pointer = format!(
        "(({} *){source}.data + {offset})",
        expr.ty.pointee().unwrap().c_name()
    );
    (setup, tests.join(" && "), pointer)
}

pub(super) fn address(expr: &CheckedExpression, context: &mut ExpressionContext) -> String {
    let (setup, condition, pointer) = parts(expr, true, context);
    format!("({setup}, (void)({condition}), {pointer})")
}

pub(super) fn read_parts(
    expr: &CheckedExpression,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    let Value::ArrayRead { address, .. } = &expr.value else {
        unreachable!()
    };
    let (setup, condition, pointer) = parts(address, false, context);
    (setup, condition, format!("(*{pointer})"))
}
