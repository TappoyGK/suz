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
    if !context.arrays.entries.is_empty() {
        c.push_str("#include <string.h>\n");
    }
    for (id, array) in context.arrays.entries.iter().enumerate() {
        if array.soa {
            let Type::Struct(struct_id) = array.element else {
                continue;
            };
            let leaf_count = super::soa::leaves(struct_id, context).len();
            writeln!(
                c,
                "typedef struct {{ void *cols[{leaf_count}]; int32_t lengths[{}]; }} suz_array_{id};",
                array.rank
            )
            .unwrap();
            writeln!(c, "typedef suz_array_{id} suz_readonly_array_{id};").unwrap();
            continue;
        }
        writeln!(
            c,
            "typedef struct {{ const void *data; int32_t lengths[{}]; }} suz_readonly_array_{id};",
            array.rank
        )
        .unwrap();
        writeln!(
            c,
            "typedef struct {{ void *data; int32_t lengths[{}]; }} suz_array_{id};",
            array.rank
        )
        .unwrap();
    }
}

/// 不変の配列参照のデータと長さは束縛時に取り出し、アクセスごとの構造体の読み取りを省く。
pub(super) fn cache(
    c: &mut String,
    id: usize,
    ty: Type,
    indent: &str,
    context: &ExpressionContext,
) {
    let array = context.arrays.get(ty.array_id().unwrap());
    let source = context.local(id);
    if !array.soa {
        writeln!(
            c,
            "{indent}{} *const suz_array_data_{id} = ({source}).data;\n{indent}(void)suz_array_data_{id};",
            element_type(array.element, ty)
        )
        .unwrap();
    }
    for dimension in 0..array.rank {
        writeln!(c, "{indent}const int32_t suz_array_length_{id}_{dimension} = ({source}).lengths[{dimension}];\n{indent}(void)suz_array_length_{id}_{dimension};").unwrap();
    }
}

pub(super) fn temporary(ty: Type, context: &mut ExpressionContext) -> String {
    let id = context.temporaries.len();
    context.temporaries.push(ty);
    format!("suz_cmp_{id}")
}

/// 呼び出しや添字の検査を含まない参照は、先頭位置と寸法を直接読み出せる。
pub(super) fn direct_reference(value: &CheckedExpression) -> bool {
    match &value.value {
        Value::Local(_) => true,
        Value::Field { target, .. } | Value::Deref(target) => direct_reference(target),
        _ => false,
    }
}

/// 先頭位置と寸法だけを移す。副作用を持ち得る変換元は一時変数で評価を1回に保つ。
pub(super) fn readonly(value: &CheckedExpression, context: &mut ExpressionContext) -> String {
    let id = value.ty.array_id().unwrap();
    let array = context.arrays.get(id);
    if array.soa {
        return expression(value, context);
    }
    let rank = array.rank;
    let (setup, source) = if direct_reference(value) {
        (String::new(), format!("({})", expression(value, context)))
    } else {
        let source = temporary(value.ty, context);
        let value = expression(value, context);
        (format!("{source} = {value}, "), source)
    };
    let lengths = (0..rank)
        .map(|axis| format!("{source}.lengths[{axis}]"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "({setup}(suz_readonly_array_{id}){{ .data = {source}.data, .lengths = {{ {lengths} }} }})"
    )
}

/// 保証済みの不変の添字は、評価順を保存する一時変数を必要としない。
/// 加減算は非負かつlen_t内と証明された形だけをsize_tで計算する。
pub(super) fn proven_index(
    index: &CheckedExpression,
    context: &mut ExpressionContext,
) -> Option<String> {
    use crate::ast::BinaryOperator;
    use crate::check::{IntegerOperation, exact::ExactKind};
    match &index.value {
        Value::Local(id) if context.immutable_locals.contains(id) => {
            Some(expression(index, context))
        }
        Value::Integer { value, .. } if *value >= 0 => Some(expression(index, context)),
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler: None,
            ..
        } => {
            let ExactKind::Binary(op @ (BinaryOperator::Add | BinaryOperator::Subtract), a, b) =
                &e.kind
            else {
                return None;
            };
            let ExactKind::Leaf(base) = &a.kind else {
                return None;
            };
            let Value::Local(id) = base.value else {
                return None;
            };
            let ExactKind::Literal(n) = b.kind else {
                return None;
            };
            let n = n.to_i128()?;
            if !context.immutable_locals.contains(&id)
                || !(0..=i32::MAX as i128).contains(&n)
                || !e.range?.contained(0, i32::MAX as i128)
                || !a.range?.contained(0, i32::MAX as i128)
            {
                return None;
            }
            Some(format!(
                "((size_t){} {} (size_t){n})",
                context.local(id),
                op.c_symbol()
            ))
        }
        _ => None,
    }
}

pub(super) fn panic_call(location: Location, reason: &str, context: &ExpressionContext) -> String {
    panic(location, reason, context)
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
    if context.arrays.get(ty.array_id().unwrap()).soa {
        return super::soa::allocate(value, ty, location, context);
    }
    let Value::Array {
        values,
        lengths,
        repeated,
        metadata,
    } = &value.value
    else {
        unreachable!()
    };
    context.uses_arena = true;
    let element = context.arrays.get(ty.array_id().unwrap()).element.c_name();
    let mut c = String::from("({ ");
    for count in &metadata.constant_counts {
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
    write!(c, "size_t suz_bytes = suz_count * sizeof({element}); void *suz_data = suz_arena_alloc(suz_bytes ? suz_bytes : 1, alignof({element}), {}, {}, {}); ", c_string(&context.source_name), location.line, location.column).unwrap();
    if *repeated && zero_bits(&values[0]) {
        c.push_str("(void)suz_initial_0; memset(suz_data, 0, suz_bytes); ");
    } else if *repeated {
        write!(c, "for (size_t suz_i = 0; suz_i < suz_count; ++suz_i) (({element} *)suz_data)[suz_i] = suz_initial_0; ").unwrap();
    } else {
        for i in 0..values.len() {
            let slot = metadata.slots.get(i).copied().unwrap_or(i);
            write!(c, "(({element} *)suz_data)[{slot}] = suz_initial_{i}; ").unwrap();
        }
    }
    c.push_str("suz_array_value.data = suz_data; suz_array_value; })");
    c
}

/// 失敗した添字より後の式を実行せず、全検査に成功したときだけ要素のオフセットと対象参照を作る。
pub(super) fn element_offset_parts(
    expr: &CheckedExpression,
    write: bool,
    context: &mut ExpressionContext,
) -> (String, String, String, String) {
    let Value::ArrayElement { target, indices } = &expr.value else {
        unreachable!()
    };
    let cached = match target.value {
        Value::Local(id) if context.immutable_locals.contains(&id) => Some(id),
        _ => None,
    };
    let immutable = cached.is_some();
    let source = if immutable {
        format!("({})", expression(target, context))
    } else {
        temporary(target.ty, context)
    };
    let mut tests = Vec::new();
    let setup = if immutable {
        "(void)0".into()
    } else if unhandled(target) {
        let (setup, valid, value) = access_parts(target, write, context);
        tests.push(sequence(
            &valid,
            &format!("({source} = {value}, true)"),
            write,
        ));
        setup
    } else {
        format!("{source} = {}", expression(target, context))
    };
    let mut offset = String::new();
    for (dimension, (index, location, proven)) in indices.iter().enumerate() {
        let direct = proven.then(|| proven_index(index, context)).flatten();
        let (local, evaluate) = if let Some(direct) = direct {
            (direct, String::new())
        } else if matches!(index.value, Value::Local(id) if context.immutable_locals.contains(&id))
        {
            (expression(index, context), String::new())
        } else {
            let local = temporary(index.ty, context);
            let evaluate = format!("{local} = {}, ", expression(index, context));
            (local, evaluate)
        };
        let length = cached.map_or_else(
            || format!("{source}.lengths[{dimension}]"),
            |id| format!("suz_array_length_{id}_{dimension}"),
        );
        let check = if *proven {
            format!("({evaluate}true)")
        } else {
            format!("({evaluate}(uint64_t){local} < (uint64_t){length})")
        };
        tests.push(if write && !proven {
            context.uses_arena = true;
            let negative = if index.ty.integer_bounds().unwrap().0 < 0 {
                format!("{local} < 0")
            } else {
                "false".into()
            };
            let fail = format!(
                "suz_array_bounds({}, {}, {}, {}, (uint64_t){local}, {negative}, {length})",
                c_string(&context.source_name),
                location.line,
                location.column,
                dimension + 1
            );
            format!("({check} || ({fail}, false))")
        } else {
            check
        });
        offset = if dimension == 0 {
            format!("(size_t){local}")
        } else {
            format!("({offset} * (size_t){length} + (size_t){local})")
        };
    }
    let condition = if write {
        format!(
            "({}, true)",
            tests
                .iter()
                .map(|t| format!("(void)({t})"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        tests.join(" && ")
    };
    (setup, condition, source, offset)
}

/// 失敗した添字より後の式を実行せず、全検査に成功したときだけ要素のアドレスを作る。
fn parts(
    expr: &CheckedExpression,
    write: bool,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    let Value::ArrayElement { target, .. } = &expr.value else {
        unreachable!()
    };
    let cached = match target.value {
        Value::Local(id) if context.immutable_locals.contains(&id) => Some(id),
        _ => None,
    };
    let (setup, condition, source, offset) = element_offset_parts(expr, write, context);
    let data = cached.map_or_else(
        || {
            format!(
                "({} *){source}.data",
                element_type(expr.ty.pointee().unwrap(), target.ty)
            )
        },
        |id| format!("suz_array_data_{id}"),
    );
    let pointer = format!("({data} + {offset})");
    (setup, condition, pointer)
}

pub(super) fn address(expr: &CheckedExpression, context: &mut ExpressionContext) -> String {
    let (setup, condition, pointer) = parts(expr, true, context);
    format!("({setup}, (void)({condition}), {pointer})")
}

pub(super) fn read_parts(
    expr: &CheckedExpression,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    access_parts(expr, false, context)
}

/// 書き込みは失敗時に脱出するため、後続の検査へ常に進むことをCにも明示する。
fn sequence(first: &str, next: &str, write: bool) -> String {
    if write {
        format!("((void)({first}), {next})")
    } else {
        format!("({first} && {next})")
    }
}

fn unhandled(expr: &CheckedExpression) -> bool {
    matches!(
        expr.value,
        Value::ArrayRead { handler: None, .. } | Value::ArrayView { handler: None, .. }
    )
}

fn access_parts(
    expr: &CheckedExpression,
    write: bool,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    if let Value::ArrayRead { address, .. } = &expr.value {
        if let Value::ArrayElement { target, .. } = &address.value
            && let Some(aid) = target.ty.array_id()
            && context.arrays.get(aid).soa
        {
            let Type::Struct(struct_id) = context.arrays.get(aid).element else {
                unreachable!("soa array element must be a struct");
            };
            let (setup, condition, source, offset) = element_offset_parts(address, write, context);
            let (gathered, _) =
                super::soa::gather_expr(struct_id, &format!("{source}.cols"), &offset, 0, context);
            return (setup, condition, gathered);
        }
        let (setup, condition, pointer) = parts(address, write, context);
        return (setup, condition, format!("(*{pointer})"));
    }
    let Value::ArrayView {
        target,
        start,
        end,
        index,
        proven,
        location,
        ..
    } = &expr.value
    else {
        unreachable!()
    };
    let array = context.arrays.get(target.ty.array_id().unwrap());
    let source = temporary(target.ty, context);
    let (setup, previous) = if unhandled(target) {
        let (setup, valid, value) = access_parts(target, write, context);
        (
            setup,
            sequence(&valid, &format!("({source} = {value}, true)"), write),
        )
    } else {
        (
            format!("{source} = {}", expression(target, context)),
            "true".into(),
        )
    };
    let mut evaluations = Vec::new();
    let mut bound = |value: &Option<Box<CheckedExpression>>, default: String| {
        value.as_ref().map_or(default, |value| {
            let local = temporary(value.ty, context);
            evaluations.push(format!("{local} = {}", expression(value, context)));
            local
        })
    };
    let first = bound(start, "0".into());
    let last = bound(end, format!("{source}.lengths[0]"));
    let test = if *proven {
        "true".into()
    } else if *index {
        format!("(uint64_t){first} < (uint64_t){source}.lengths[0]")
    } else {
        format!(
            "((uint64_t){first} <= (uint64_t){last} && (uint64_t){last} <= (uint64_t){source}.lengths[0])"
        )
    };
    // 同じ角括弧の境界は両方評価してから検査する。前の角括弧が失敗したら評価しない。
    evaluations.push(if write && !proven {
        context.uses_arena = true;
        let negative = |v: &Option<Box<CheckedExpression>>, name: &str| {
            if v.as_ref().is_some_and(|v| v.ty.integer_bounds().unwrap().0 < 0) {
                format!("{name} < 0")
            } else { "false".into() }
        };
        let fail = if *index {
            format!("suz_array_bounds({}, {}, {}, 1, (uint64_t){first}, {}, {source}.lengths[0])",
                c_string(&context.source_name), location.line, location.column, negative(start, &first))
        } else {
            format!("suz_array_slice_bounds({}, {}, {}, (uint64_t){first}, {}, (uint64_t){last}, {}, {source}.lengths[0])",
                c_string(&context.source_name), location.line, location.column, negative(start, &first), negative(end, &last))
        };
        format!("({test} || ({fail}, false))")
    } else {
        test
    });
    let condition = sequence(&previous, &format!("({})", evaluations.join(", ")), write);
    let stride = (1..array.rank)
        .map(|i| format!("(size_t){source}.lengths[{i}]"))
        .collect::<Vec<_>>();
    let stride = if stride.is_empty() {
        "1".into()
    } else {
        stride.join(" * ")
    };
    let lengths = if *index {
        (1..array.rank)
            .map(|i| format!("{source}.lengths[{i}]"))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        std::iter::once(format!("(int32_t)((uint64_t){last} - (uint64_t){first})"))
            .chain((1..array.rank).map(|i| format!("{source}.lengths[{i}]")))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let value = if array.soa {
        let Type::Struct(struct_id) = array.element else {
            unreachable!("soa array element must be a struct");
        };
        let all_leaves = super::soa::leaves(struct_id, context);
        let cols = all_leaves
            .iter()
            .enumerate()
            .map(|(c, leaf)| {
                format!(
                    "({} *){source}.cols[{c}] + (size_t){first} * ({stride})",
                    leaf.ty.c_name()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "({}){{ .cols = {{ {cols} }}, .lengths = {{ {lengths} }} }}",
            expr.ty.c_name()
        )
    } else {
        format!(
            "({}){{ .data = ({} *){source}.data + (size_t){first} * ({stride}), .lengths = {{ {lengths} }} }}",
            expr.ty.c_name(),
            element_type(array.element, target.ty)
        )
    };
    (setup, condition, value)
}

/// 表現が全ビット0と分かるスカラーだけを一括初期化する。
/// NULLポインタ・構造体のパディング・負の浮動小数点ゼロには適用しない。
fn zero_bits(value: &CheckedExpression) -> bool {
    match &value.value {
        Value::Integer { value: 0, .. } | Value::Bool(false) | Value::Float(0) => true,
        Value::Cast(inner) if value.ty.is_integer() || value.ty.is_float() => zero_bits(inner),
        _ => false,
    }
}

/// 入口で参照を保存する。各要素は反復の直前に読み、前の反復による書き込みを反映する。
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_loop(
    c: &mut String,
    id: usize,
    item: Type,
    source: &CheckedExpression,
    reverse: bool,
    body: &[crate::check::CheckedStatement],
    depth: usize,
    next_assignment: &mut usize,
    context: &mut ExpressionContext,
) {
    let indent = "    ".repeat(depth);
    let array = context.arrays.get(source.ty.array_id().unwrap());
    let element = element_type(array.element, source.ty);
    let ty = source.ty.c_name();
    let source = expression(source, context);
    writeln!(
        c,
        "{indent}{{\n{indent}    const {ty} suz_iter_{id} = {source};"
    )
    .unwrap();
    // データと各寸法は反復中に変わらない。構造体からの読み取りを入口で済ませる。
    if !array.soa {
        writeln!(
            c,
            "{indent}    {} *const suz_iter_data_{id} = suz_iter_{id}.data;",
            element
        )
        .unwrap();
    }
    for dimension in 0..array.rank {
        writeln!(c, "{indent}    const int32_t suz_iter_length_{id}_{dimension} = suz_iter_{id}.lengths[{dimension}];").unwrap();
    }
    let (initial, condition, position) = if reverse {
        (
            format!("suz_iter_length_{id}_0"),
            format!("suz_cursor_{id} > 0"),
            format!("--suz_cursor_{id}"),
        )
    } else {
        (
            "0".into(),
            format!("suz_cursor_{id} < suz_iter_length_{id}_0"),
            format!("suz_cursor_{id}++"),
        )
    };
    writeln!(c, "{indent}    int32_t suz_cursor_{id} = {initial};\n{indent}    while ({condition}) {{\n{indent}        const size_t suz_pos_{id} = {position};").unwrap();
    let value = if array.soa {
        let Type::Struct(struct_id) = array.element else {
            unreachable!("soa array element must be a struct");
        };
        if array.rank == 1 {
            super::soa::gather_expr(
                struct_id,
                &format!("suz_iter_{id}.cols"),
                &format!("suz_pos_{id}"),
                0,
                context,
            )
            .0
        } else {
            let lengths = (1..array.rank)
                .map(|i| format!("suz_iter_length_{id}_{i}"))
                .collect::<Vec<_>>();
            let stride = lengths
                .iter()
                .map(|s| format!("(size_t){s}"))
                .collect::<Vec<_>>()
                .join(" * ");
            let all_leaves = super::soa::leaves(struct_id, context);
            let cols = all_leaves
                .iter()
                .enumerate()
                .map(|(c, leaf)| {
                    format!(
                        "({} *)suz_iter_{id}.cols[{c}] + suz_pos_{id} * ({stride})",
                        leaf.ty.c_name()
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "({}){{ .cols = {{ {cols} }}, .lengths = {{ {} }} }}",
                item.c_name(),
                lengths.join(", ")
            )
        }
    } else if array.rank == 1 {
        format!("suz_iter_data_{id}[suz_pos_{id}]")
    } else {
        let lengths = (1..array.rank)
            .map(|i| format!("suz_iter_length_{id}_{i}"))
            .collect::<Vec<_>>();
        let stride = lengths
            .iter()
            .map(|s| format!("(size_t){s}"))
            .collect::<Vec<_>>()
            .join(" * ");
        format!(
            "({}){{ .data = suz_iter_data_{id} + suz_pos_{id} * ({stride}), .lengths = {{ {} }} }}",
            item.c_name(),
            lengths.join(", ")
        )
    };
    writeln!(
        c,
        "{indent}        typeof({}) const suz_local_{id} = {value};\n{indent}        (void)suz_local_{id};",
        item.c_name()
    )
    .unwrap();
    context.immutable_locals.insert(id);
    if item.array_id().is_some() {
        cache(c, id, item, &format!("{indent}        "), context);
    }
    super::statements::emit_block(c, body, depth + 2, next_assignment, context);
    writeln!(c, "{indent}    }}\n{indent}}}").unwrap();
}

fn element_type(element: Type, reference: Type) -> String {
    if reference.is_readonly_array() {
        format!("{} const", element.c_name())
    } else {
        element.c_name()
    }
}

/// 配列本体は関数の外に一度だけ置く。参照を得る場所では先頭と寸法だけを返す。
/// symbolの登録と非有限floatのビット復元は起動時に行い、静的領域へ保存する。
pub(super) fn constant(value: &CheckedExpression, context: &mut ExpressionContext) -> String {
    if context.arrays.get(value.ty.array_id().unwrap()).soa {
        return super::soa::constant(value, context);
    }
    let Value::Array {
        values,
        lengths,
        repeated,
        metadata,
    } = &value.value
    else {
        unreachable!()
    };
    let array = context.arrays.get(value.ty.array_id().unwrap());
    let initial = values
        .iter()
        .map(|v| constant_element(v, context))
        .collect::<Vec<_>>();
    let static_initial = initial.iter().all(|(_, literal)| *literal);
    let lengths = lengths
        .iter()
        .map(|v| match v.value {
            Value::Integer { value, .. } => value.to_string(),
            _ => unreachable!("constant array dimension"),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let count = *metadata.constant_counts.last().unwrap();
    let key = format!(
        "{:?}:{lengths}:{repeated}:{}",
        value.ty.with_nilable(false),
        initial
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let id = if let Some(id) = context.constant_arrays.get(&key) {
        *id
    } else {
        let id = context.constant_arrays.len();
        context.constant_arrays.insert(key, id);
        let element = array.element.c_name();
        let name = format!("suz_constant_array_{id}");
        let size = count.max(1);
        for count in &metadata.constant_counts {
            writeln!(context.constant_array_data, r#"_Static_assert(UINT64_C({count}) <= SIZE_MAX / sizeof({element}), "array byte size overflows size_t [array.layout]");"#).unwrap();
        }
        if static_initial {
            let initial = if count == 0 {
                "0".into()
            } else if *repeated {
                format!("[0 ... {}] = {}", count - 1, initial[0].0)
            } else {
                initial
                    .iter()
                    .map(|(text, _)| text.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            writeln!(
                context.constant_array_data,
                "static {element} const {name}[{size}] = {{ {initial} }};"
            )
            .unwrap();
        } else {
            // 起動時の代入が必要な要素と、Cの静的初期値で表せる要素を分ける。
            let static_values = initial
                .iter()
                .enumerate()
                .filter(|(_, (_, literal))| *literal)
                .map(|(i, (text, _))| format!("[{i}] = {text}"))
                .collect::<Vec<_>>();
            let initializer = if static_values.is_empty() {
                "0".into()
            } else {
                static_values.join(", ")
            };
            writeln!(
                context.constant_array_data,
                "static {element} {name}[{size}] = {{ {initializer} }};"
            )
            .unwrap();
            if *repeated && count != 0 {
                writeln!(
                    context.constant_array_init,
                    "    for (size_t i = 0; i < UINT64_C({count}); ++i) {name}[i] = {};",
                    initial[0].0
                )
                .unwrap();
            } else if !repeated {
                for (index, (initial, literal)) in initial.iter().enumerate() {
                    if *literal {
                        continue;
                    }
                    writeln!(
                        context.constant_array_init,
                        "    {name}[{index}] = {initial};"
                    )
                    .unwrap();
                }
            }
        }
        id
    };
    format!(
        "({}){{ .data = suz_constant_array_{id}, .lengths = {{ {lengths} }} }}",
        value.ty.c_name()
    )
}

pub(super) fn constant_element(
    value: &CheckedExpression,
    context: &mut ExpressionContext,
) -> (String, bool) {
    match &value.value {
        Value::Struct { fields } => {
            let mut literal = true;
            let fields = fields
                .iter()
                .map(|(index, field)| {
                    let (text, constant) = constant_element(field, context);
                    literal &= constant;
                    format!(
                        ".{} = {text}",
                        super::structs::field_name(value.ty, *index, context)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            (format!("({}){{ {fields} }}", value.ty.c_name()), literal)
        }
        Value::Text { .. } => (expression(value, context), value.ty != Type::Symbol),
        Value::Float(bits) => {
            let finite = match value.ty {
                Type::Float(crate::ast::FloatType::F32) => f32::from_bits(*bits as u32).is_finite(),
                _ => f64::from_bits(*bits).is_finite(),
            };
            (expression(value, context), finite)
        }
        _ => (expression(value, context), true),
    }
}

/// 1次元配列への一括書き込み（`out#[] = 式`、`out#[s:e] = 式`、複合代入）をC23へ出力する。
/// オペランドを左から右へ1回評価し、ループ前に範囲・長さを1回検査してから、
/// 非重複・完全一致・ずれた重なりに応じてC23 §6.7.3.1に準拠するループを切り替える。
pub(super) fn emit_bulk_write(
    c: &mut String,
    bulk: &crate::check::ir::CheckedBulkWrite,
    depth: usize,
    next_assignment: &mut usize,
    context: &mut ExpressionContext,
) {
    use crate::check::ir::BulkInput;
    let bid = *next_assignment;
    *next_assignment += 1;
    let indent = "    ".repeat(depth);
    let inner = "    ".repeat(depth + 1);
    let out_array = context.arrays.get(bulk.target.ty.array_id().unwrap());
    let out_elem_ty = out_array.element;
    let dst_c_ty = out_elem_ty.c_name();

    writeln!(c, "{indent}{{").unwrap();
    let target_expr = expression(&bulk.target, context);
    writeln!(
        c,
        "{inner}const {} suz_bulk_out_{bid} = {target_expr};",
        bulk.target.ty.c_name()
    )
    .unwrap();

    // 左辺の切り出し境界を左から右に評価する。
    let (start_var, start_neg) = if let Some(start) = &bulk.start {
        let s_expr = expression(start, context);
        writeln!(
            c,
            "{inner}const {} suz_bulk_s_{bid} = {s_expr};",
            start.ty.c_name()
        )
        .unwrap();
        let neg = start
            .ty
            .integer_bounds()
            .is_some_and(|(min, _)| min < 0)
            .then(|| format!("suz_bulk_s_{bid} < 0"));
        (format!("suz_bulk_s_{bid}"), neg)
    } else {
        ("0".into(), None)
    };
    let (end_var, end_neg) = if let Some(end) = &bulk.end {
        let e_expr = expression(end, context);
        writeln!(
            c,
            "{inner}const {} suz_bulk_e_{bid} = {e_expr};",
            end.ty.c_name()
        )
        .unwrap();
        let neg = end
            .ty
            .integer_bounds()
            .is_some_and(|(min, _)| min < 0)
            .then(|| format!("suz_bulk_e_{bid} < 0"));
        (format!("suz_bulk_e_{bid}"), neg)
    } else {
        (format!("suz_bulk_out_{bid}.lengths[0]"), None)
    };

    // 右辺の配列参照とブロードキャストスカラーを左から右へ1回ずつ評価する。
    // 不変ローカルの書き込み先と同じローカルを参照する入力は、書き込み先ポインタ経由の読み取りへまとめる。
    // 同じ不変ローカルを参照する入力が複数ある場合は、最初の入力ポインタを共有する。
    let immutable_target_local = if !bulk.slice
        && let Value::Local(id) = bulk.target.value
        && context.immutable_locals.contains(&id)
    {
        Some(id)
    } else {
        None
    };
    enum BulkArraySource {
        SameAsDst,
        Shared(usize),
        Own {
            array_id: usize,
            elem_ty: Type,
            soa: bool,
            length_proven: bool,
            location: Location,
        },
    }
    let mut seen_immutable_inputs: std::collections::BTreeMap<(usize, Type, bool), usize> =
        std::collections::BTreeMap::new();
    let mut array_inputs: Vec<(usize, BulkArraySource)> = Vec::new();
    for input in &bulk.inputs {
        match input {
            BulkInput::Array {
                array_id,
                elem_id,
                expr,
                length_proven,
                location,
            } => {
                let in_array = context.arrays.get(expr.ty.array_id().unwrap());
                let elem_ty = in_array.element;
                let in_soa = in_array.soa;
                let source = if immutable_target_local.is_some_and(|target_id| {
                    matches!(expr.value, Value::Local(in_id) if in_id == target_id)
                        && elem_ty == out_elem_ty
                        && in_soa == out_array.soa
                }) {
                    BulkArraySource::SameAsDst
                } else if let Value::Local(in_id) = expr.value
                    && context.immutable_locals.contains(&in_id)
                {
                    if let Some(&first_idx) = seen_immutable_inputs.get(&(in_id, elem_ty, in_soa)) {
                        BulkArraySource::Shared(first_idx)
                    } else {
                        let expr_c = expression(expr, context);
                        writeln!(
                            c,
                            "{inner}const {} suz_bulk_arr_{array_id} = {expr_c};",
                            expr.ty.c_name()
                        )
                        .unwrap();
                        seen_immutable_inputs.insert((in_id, elem_ty, in_soa), array_inputs.len());
                        BulkArraySource::Own {
                            array_id: *array_id,
                            elem_ty,
                            soa: in_soa,
                            length_proven: *length_proven,
                            location: *location,
                        }
                    }
                } else {
                    let expr_c = expression(expr, context);
                    writeln!(
                        c,
                        "{inner}const {} suz_bulk_arr_{array_id} = {expr_c};",
                        expr.ty.c_name()
                    )
                    .unwrap();
                    BulkArraySource::Own {
                        array_id: *array_id,
                        elem_ty,
                        soa: in_soa,
                        length_proven: *length_proven,
                        location: *location,
                    }
                };
                array_inputs.push((*elem_id, source));
            }
            BulkInput::Scalar { local_id, expr } => {
                let expr_c = expression(expr, context);
                writeln!(
                    c,
                    "{inner}const {} suz_local_{local_id} = {expr_c};\n{inner}(void)suz_local_{local_id};",
                    expr.ty.c_name()
                )
                .unwrap();
            }
        }
    }

    // ループ前の1回の範囲・長さ検査（失敗時は書き込み前にパニック）。
    if bulk.slice && !bulk.slice_proven {
        context.uses_arena = true;
        let s_neg = start_neg.as_deref().unwrap_or("false");
        let e_neg = end_neg.as_deref().unwrap_or("false");
        let mut cond = format!(
            "(uint64_t){start_var} <= (uint64_t){end_var} && (uint64_t){end_var} <= (uint64_t)suz_bulk_out_{bid}.lengths[0]"
        );
        if let Some(neg) = &start_neg {
            cond = format!("!({neg}) && {cond}");
        }
        if let Some(neg) = &end_neg {
            cond = format!("!({neg}) && {cond}");
        }
        writeln!(
            c,
            "{inner}if (!({cond})) suz_array_slice_bounds({}, {}, {}, (uint64_t){start_var}, {s_neg}, (uint64_t){end_var}, {e_neg}, suz_bulk_out_{bid}.lengths[0]);",
            c_string(&context.source_name),
            bulk.slice_location.line,
            bulk.slice_location.column,
        )
        .unwrap();
    }

    if bulk.slice {
        writeln!(
            c,
            "{inner}const size_t suz_bulk_n_{bid} = (size_t)((uint64_t){end_var} - (uint64_t){start_var});"
        )
        .unwrap();
    } else {
        writeln!(
            c,
            "{inner}const size_t suz_bulk_n_{bid} = (size_t)suz_bulk_out_{bid}.lengths[0];"
        )
        .unwrap();
    }

    for (_, source) in &array_inputs {
        if let BulkArraySource::Own {
            array_id,
            length_proven: false,
            location,
            ..
        } = source
        {
            let fail = panic(
                *location,
                "array bulk length mismatch [rt.array-bounds]",
                context,
            );
            writeln!(
                c,
                "{inner}if ((size_t)suz_bulk_arr_{array_id}.lengths[0] != suz_bulk_n_{bid}) {fail};"
            )
            .unwrap();
        }
    }

    let any_soa = out_array.soa
        || array_inputs
            .iter()
            .any(|(_, s)| matches!(s, BulkArraySource::Own { soa: true, .. }));
    if any_soa {
        let idx_var = format!("suz_bulk_i_{bid}");
        let dst_idx = if bulk.slice {
            format!("((size_t){start_var} + {idx_var})")
        } else {
            idx_var.clone()
        };
        let dst_elem_expr = |context: &ExpressionContext| {
            if out_array.soa {
                let Type::Struct(sid) = out_elem_ty else {
                    unreachable!()
                };
                super::soa::gather_expr(
                    sid,
                    &format!("suz_bulk_out_{bid}.cols"),
                    &dst_idx,
                    0,
                    context,
                )
                .0
            } else {
                format!("(({dst_c_ty} *)suz_bulk_out_{bid}.data)[{dst_idx}]")
            }
        };
        let mut saved = Vec::new();
        if let Some(out_id) = bulk.out_elem_id {
            let expr_str = dst_elem_expr(context);
            saved.push((out_id, context.bulk_bindings.insert(out_id, expr_str)));
        }
        for &(elem_id, ref source) in &array_inputs {
            let in_expr = match source {
                BulkArraySource::SameAsDst => dst_elem_expr(context),
                BulkArraySource::Shared(first) => match &array_inputs[*first].1 {
                    BulkArraySource::Own {
                        array_id,
                        elem_ty,
                        soa,
                        ..
                    } => {
                        if *soa {
                            let Type::Struct(sid) = *elem_ty else {
                                unreachable!()
                            };
                            super::soa::gather_expr(
                                sid,
                                &format!("suz_bulk_arr_{array_id}.cols"),
                                &idx_var,
                                0,
                                context,
                            )
                            .0
                        } else {
                            format!(
                                "((const {} *)suz_bulk_arr_{array_id}.data)[{idx_var}]",
                                elem_ty.c_name()
                            )
                        }
                    }
                    _ => unreachable!(),
                },
                BulkArraySource::Own {
                    array_id,
                    elem_ty,
                    soa,
                    ..
                } => {
                    if *soa {
                        let Type::Struct(sid) = *elem_ty else {
                            unreachable!()
                        };
                        super::soa::gather_expr(
                            sid,
                            &format!("suz_bulk_arr_{array_id}.cols"),
                            &idx_var,
                            0,
                            context,
                        )
                        .0
                    } else {
                        format!(
                            "((const {} *)suz_bulk_arr_{array_id}.data)[{idx_var}]",
                            elem_ty.c_name()
                        )
                    }
                }
            };
            saved.push((elem_id, context.bulk_bindings.insert(elem_id, in_expr)));
        }
        let mask_c = bulk.mask.as_ref().map(|mask| emit_bulk_mask(mask, context));
        let rhs_c = expression(&bulk.element_value, context);
        for (id, prev) in saved.into_iter().rev() {
            if let Some(prev) = prev {
                context.bulk_bindings.insert(id, prev);
            } else {
                context.bulk_bindings.remove(&id);
            }
        }
        writeln!(
            c,
            "{inner}for (size_t {idx_var} = 0; {idx_var} < suz_bulk_n_{bid}; ++{idx_var}) {{"
        )
        .unwrap();
        let body_indent = if mask_c.is_some() {
            format!("{inner}        ")
        } else {
            format!("{inner}    ")
        };
        if let Some(mask_c) = mask_c {
            writeln!(c, "{inner}    if ({mask_c}) {{").unwrap();
        }
        if out_array.soa {
            let Type::Struct(sid) = out_elem_ty else {
                unreachable!()
            };
            let val_var = format!("suz_bulk_val_{bid}");
            writeln!(c, "{body_indent}const {dst_c_ty} {val_var} = {rhs_c};").unwrap();
            let scatter = super::soa::scatter_stmts(
                sid,
                &format!("suz_bulk_out_{bid}.cols"),
                &dst_idx,
                &val_var,
                0,
                context,
            );
            writeln!(c, "{body_indent}{scatter}").unwrap();
        } else {
            writeln!(
                c,
                "{body_indent}(({dst_c_ty} *)suz_bulk_out_{bid}.data)[{dst_idx}] = {rhs_c};"
            )
            .unwrap();
        }
        if bulk.mask.is_some() {
            writeln!(c, "{inner}    }}").unwrap();
        }
        writeln!(c, "{inner}}}").unwrap();
        writeln!(c, "{indent}}}").unwrap();
        return;
    }

    // ポインタ準備
    if bulk.slice {
        writeln!(
            c,
            "{inner}{dst_c_ty} *const suz_bulk_dst_raw_{bid} = ({dst_c_ty} *)suz_bulk_out_{bid}.data + (size_t){start_var};"
        )
        .unwrap();
    } else {
        writeln!(
            c,
            "{inner}{dst_c_ty} *const suz_bulk_dst_raw_{bid} = ({dst_c_ty} *)suz_bulk_out_{bid}.data;"
        )
        .unwrap();
    }
    for (idx, (_, source)) in array_inputs.iter().enumerate() {
        if let BulkArraySource::Own {
            array_id, elem_ty, ..
        } = source
        {
            let in_c_ty = elem_ty.c_name();
            writeln!(
                c,
                "{inner}const {in_c_ty} *const suz_bulk_in_raw_{bid}_{idx} = (const {in_c_ty} *)suz_bulk_arr_{array_id}.data;"
            )
            .unwrap();
        }
    }

    let emit_loop = |c: &mut String,
                     loop_indent: &str,
                     dst_name: &str,
                     in_names: &[String],
                     context: &mut ExpressionContext| {
        let idx_var = format!("suz_bulk_i_{bid}");
        let mut saved = Vec::new();
        if let Some(out_id) = bulk.out_elem_id {
            saved.push((
                out_id,
                context
                    .bulk_bindings
                    .insert(out_id, format!("{dst_name}[{idx_var}]")),
            ));
        }
        for (j, &(elem_id, _)) in array_inputs.iter().enumerate() {
            saved.push((
                elem_id,
                context
                    .bulk_bindings
                    .insert(elem_id, format!("{}[{idx_var}]", in_names[j])),
            ));
        }
        let mask_c = bulk.mask.as_ref().map(|mask| emit_bulk_mask(mask, context));
        let rhs_c = expression(&bulk.element_value, context);
        for (id, prev) in saved.into_iter().rev() {
            if let Some(prev) = prev {
                context.bulk_bindings.insert(id, prev);
            } else {
                context.bulk_bindings.remove(&id);
            }
        }
        writeln!(
            c,
            "{loop_indent}for (size_t {idx_var} = 0; {idx_var} < suz_bulk_n_{bid}; ++{idx_var}) {{"
        )
        .unwrap();
        if let Some(mask_c) = mask_c {
            writeln!(c, "{loop_indent}    if ({mask_c}) {{").unwrap();
            writeln!(c, "{loop_indent}        {dst_name}[{idx_var}] = {rhs_c};").unwrap();
            writeln!(c, "{loop_indent}    }}").unwrap();
        } else {
            writeln!(c, "{loop_indent}    {dst_name}[{idx_var}] = {rhs_c};").unwrap();
        }
        writeln!(c, "{loop_indent}}}").unwrap();
    };

    let has_side_effect_call =
        bulk_has_call(&bulk.element_value) || bulk.mask.as_ref().is_some_and(mask_has_call);
    let distinct_inputs: Vec<(usize, Type)> = array_inputs
        .iter()
        .enumerate()
        .filter_map(|(j, (_, source))| match source {
            BulkArraySource::Own { elem_ty, .. } => Some((j, *elem_ty)),
            BulkArraySource::SameAsDst | BulkArraySource::Shared(_) => None,
        })
        .collect();

    let resolve_names = |dst_name: &str, prefix: &str| -> Vec<String> {
        array_inputs
            .iter()
            .enumerate()
            .map(|(j, (_, source))| match source {
                BulkArraySource::SameAsDst => dst_name.to_owned(),
                BulkArraySource::Shared(first) => format!("{prefix}_{bid}_{first}"),
                BulkArraySource::Own { .. } => format!("{prefix}_{bid}_{j}"),
            })
            .collect()
    };

    // 演算子メソッド等の呼び出しを含む場合は、書き込み先にも restrict を付けない（C23 §6.7.3.1）。
    if has_side_effect_call {
        let dst_name = format!("suz_bulk_dst_raw_{bid}");
        let in_names = resolve_names(&dst_name, "suz_bulk_in_raw");
        emit_loop(c, &inner, &dst_name, &in_names, context);
    } else if distinct_inputs.is_empty() {
        let dst_name = format!("suz_bulk_dst_{bid}");
        writeln!(
            c,
            "{inner}{dst_c_ty} *restrict {dst_name} = suz_bulk_dst_raw_{bid};"
        )
        .unwrap();
        let in_names = vec![dst_name.clone(); array_inputs.len()];
        emit_loop(c, &inner, &dst_name, &in_names, context);
    } else {
        writeln!(
            c,
            "{inner}const uintptr_t suz_bulk_dst_lo_{bid} = (uintptr_t)suz_bulk_dst_raw_{bid};"
        )
        .unwrap();
        writeln!(
            c,
            "{inner}const uintptr_t suz_bulk_dst_hi_{bid} = suz_bulk_dst_lo_{bid} + suz_bulk_n_{bid} * sizeof({dst_c_ty});"
        )
        .unwrap();
        let disjoint_cond = distinct_inputs
            .iter()
            .map(|&(j, elem_ty)| {
                let in_c_ty = elem_ty.c_name();
                format!(
                    "((uintptr_t)suz_bulk_in_raw_{bid}_{j} + suz_bulk_n_{bid} * sizeof({in_c_ty}) <= suz_bulk_dst_lo_{bid} || suz_bulk_dst_hi_{bid} <= (uintptr_t)suz_bulk_in_raw_{bid}_{j})"
                )
            })
            .collect::<Vec<_>>()
            .join(" && ");

        let branch_indent = "    ".repeat(depth + 2);
        // 分岐1: 書き込み先と一致する入力は書き込み先ポインタへ統合し、残りの入力がすべて非重複なら restrict を付ける。
        writeln!(c, "{inner}if ({disjoint_cond}) {{").unwrap();
        let dst_name = format!("suz_bulk_dst_{bid}");
        writeln!(
            c,
            "{branch_indent}{dst_c_ty} *restrict {dst_name} = suz_bulk_dst_raw_{bid};"
        )
        .unwrap();
        for &(j, elem_ty) in &distinct_inputs {
            let in_c_ty = elem_ty.c_name();
            writeln!(
                c,
                "{branch_indent}const {in_c_ty} *restrict suz_bulk_in_{bid}_{j} = suz_bulk_in_raw_{bid}_{j};"
            )
            .unwrap();
        }
        let restrict_ins = resolve_names(&dst_name, "suz_bulk_in");
        emit_loop(c, &branch_indent, &dst_name, &restrict_ins, context);
        writeln!(c, "{inner}}}").unwrap();

        // 分岐2: 重なり得る場合 -> restrict なしの添字順ループ。
        writeln!(c, "{inner}else {{").unwrap();
        let dst_name = format!("suz_bulk_dst_raw_{bid}");
        let raw_ins = resolve_names(&dst_name, "suz_bulk_in_raw");
        emit_loop(c, &branch_indent, &dst_name, &raw_ins, context);
        writeln!(c, "{inner}}}").unwrap();
    }

    writeln!(c, "{indent}}}").unwrap();
}

fn emit_bulk_mask(mask: &crate::check::ir::CheckedMask, context: &mut ExpressionContext) -> String {
    use crate::check::ir::CheckedMask;
    match mask {
        CheckedMask::Leaf(expr) => expression(expr, context),
        CheckedMask::And(l, r) => format!(
            "((int)(_Bool)({}) & (int)(_Bool)({}))",
            emit_bulk_mask(l, context),
            emit_bulk_mask(r, context)
        ),
        CheckedMask::Or(l, r) => format!(
            "((int)(_Bool)({}) | (int)(_Bool)({}))",
            emit_bulk_mask(l, context),
            emit_bulk_mask(r, context)
        ),
        CheckedMask::Xor(l, r) => format!(
            "((int)(_Bool)({}) ^ (int)(_Bool)({}))",
            emit_bulk_mask(l, context),
            emit_bulk_mask(r, context)
        ),
        CheckedMask::Not(inner) => format!("(!(_Bool)({}))", emit_bulk_mask(inner, context)),
    }
}

fn mask_has_call(mask: &crate::check::ir::CheckedMask) -> bool {
    use crate::check::ir::CheckedMask;
    match mask {
        CheckedMask::Leaf(expr) => bulk_has_call(expr),
        CheckedMask::And(l, r) | CheckedMask::Or(l, r) | CheckedMask::Xor(l, r) => {
            mask_has_call(l) || mask_has_call(r)
        }
        CheckedMask::Not(inner) => mask_has_call(inner),
    }
}

fn bulk_has_call(expr: &CheckedExpression) -> bool {
    match &expr.value {
        Value::Call { .. }
        | Value::Invoke { .. }
        | Value::HandledCall { .. }
        | Value::Block { .. } => true,
        _ => {
            let mut found = false;
            expr.value.visit_constant_parts(
                &mut |child| {
                    if bulk_has_call(child) {
                        found = true;
                    }
                },
                &mut |_| {},
            );
            found
        }
    }
}
