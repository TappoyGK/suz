//! 定数配列の C23 静的データ生成と起動時初期化。
use super::super::{ExpressionContext, expressions::expression};
use crate::{
    ast::Type,
    check::{CheckedExpression, Value},
};
use std::fmt::Write;

/// 配列本体は関数の外に一度だけ置く。参照を得る場所では先頭と寸法だけを返す。
/// symbolの登録と非有限floatのビット復元は起動時に行い、静的領域へ保存する。
pub(in crate::codegen) fn constant(
    value: &CheckedExpression,
    context: &mut ExpressionContext,
) -> String {
    if context.arrays.get(value.ty.array_id().unwrap()).soa {
        return super::super::soa::constant(value, context);
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
    let all_lengths: Vec<String> = lengths
        .iter()
        .map(|v| match v.value {
            Value::Integer { value, .. } => value.to_string(),
            _ => unreachable!("constant array dimension"),
        })
        .collect();
    let lengths = all_lengths.join(", ");
    let rt_lengths: Vec<String> = all_lengths
        .iter()
        .enumerate()
        .filter(|(i, _)| array.dims[*i].is_none())
        .map(|(_, s)| s.clone())
        .collect();
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
        "({}){{ .data = suz_constant_array_{id}{} }}",
        value.ty.c_name(),
        super::lengths_field_init(&rt_lengths)
    )
}

pub(in crate::codegen) fn constant_element(
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
                        super::super::structs::field_name(value.ty, *index, context)
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
