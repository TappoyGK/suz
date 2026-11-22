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
    writeln!(
        c,
        "{indent}{} *const suz_array_data_{id} = ({source}).data;\n{indent}(void)suz_array_data_{id};",
        element_type(array.element, ty)
    )
    .unwrap();
    for dimension in 0..array.rank {
        writeln!(c, "{indent}const int32_t suz_array_length_{id}_{dimension} = ({source}).lengths[{dimension}];\n{indent}(void)suz_array_length_{id}_{dimension};").unwrap();
    }
}

fn temporary(ty: Type, context: &mut ExpressionContext) -> String {
    let id = context.temporaries.len();
    context.temporaries.push(ty);
    format!("suz_cmp_{id}")
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
    if *repeated && zero_bits(&values[0]) {
        c.push_str("(void)suz_initial_0; memset(suz_array_value.data, 0, suz_bytes); ");
    } else if *repeated {
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
    // 不変の参照は添字評価で変わらないので直接読む。可変の参照・関数呼び出し・
    // 要素から取り出す参照は、添字を評価する前の値を保存する。
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
        // 不変の添字もそのまま読む。可変の添字は後続の添字で変わり得るため保存する。
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
    (
        setup,
        if write {
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
        },
        pointer,
    )
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
    let value = format!(
        "({}){{ .data = ({} *){source}.data + (size_t){first} * ({stride}), .lengths = {{ {lengths} }} }}",
        expr.ty.c_name(),
        element_type(array.element, target.ty)
    );
    (setup, condition, value)
}

/// 表現が全ビット0と分かるスカラーだけを一括初期化する。
/// nullポインタ・構造体のパディング・負の浮動小数点ゼロには適用しない。
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
    writeln!(
        c,
        "{indent}    {} *const suz_iter_data_{id} = suz_iter_{id}.data;",
        element
    )
    .unwrap();
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
    let value = if array.rank == 1 {
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
    if matches!(reference, Type::ReadOnlyArray(_)) {
        format!("{} const", element.c_name())
    } else {
        element.c_name()
    }
}

/// 配列本体は関数の外に一度だけ置く。参照を得る場所では先頭と寸法だけを返す。
/// symbolの登録と非有限floatのビット復元は起動時に行い、静的領域へ保存する。
pub(super) fn constant(value: &CheckedExpression, context: &mut ExpressionContext) -> String {
    let Value::Array {
        values,
        lengths,
        repeated,
        constant_counts,
    } = &value.value
    else {
        unreachable!()
    };
    let array = context.arrays.get(value.ty.array_id().unwrap());
    let mut static_initial = true;
    let initial = values
        .iter()
        .map(|v| {
            let (text, literal) = constant_element(v, context);
            static_initial &= literal;
            text
        })
        .collect::<Vec<_>>();
    let lengths = lengths
        .iter()
        .map(|v| match v.value {
            Value::Integer { value, .. } => value.to_string(),
            _ => unreachable!("constant array dimension"),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let count = *constant_counts.last().unwrap();
    let key = format!("{:?}:{lengths}:{repeated}:{}", value.ty, initial.join(", "));
    let id = if let Some(id) = context.constant_arrays.get(&key) {
        *id
    } else {
        let id = context.constant_arrays.len();
        context.constant_arrays.insert(key, id);
        let element = array.element.c_name();
        let name = format!("suz_constant_array_{id}");
        let size = count.max(1);
        for count in constant_counts {
            writeln!(context.constant_array_data, r#"_Static_assert(UINT64_C({count}) <= SIZE_MAX / sizeof({element}), "array byte size overflows size_t [array.layout]");"#).unwrap();
        }
        if static_initial {
            let initial = if count == 0 {
                "0".into()
            } else if *repeated {
                format!("[0 ... {}] = {}", count - 1, initial[0])
            } else {
                initial.join(", ")
            };
            writeln!(
                context.constant_array_data,
                "static {element} const {name}[{size}] = {{ {initial} }};"
            )
            .unwrap();
        } else {
            writeln!(
                context.constant_array_data,
                "static {element} {name}[{size}];"
            )
            .unwrap();
            if *repeated && count != 0 {
                writeln!(
                    context.constant_array_init,
                    "    for (size_t i = 0; i < UINT64_C({count}); ++i) {name}[i] = {};",
                    initial[0]
                )
                .unwrap();
            } else if !repeated {
                for (index, initial) in initial.iter().enumerate() {
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

fn constant_element(value: &CheckedExpression, context: &mut ExpressionContext) -> (String, bool) {
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
