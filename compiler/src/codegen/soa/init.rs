//! SoA 配列の単一ブロック確保（`new soa`）と定数 SoA 配列の C23 コード生成。
use super::{leaves, scatter_stmts};
use crate::{
    ast::{Location, Type},
    check::{CheckedExpression, Value},
    codegen::{
        ExpressionContext,
        arrays::{constant_element, panic_call},
        expressions::{c_string, expression},
    },
};
use std::fmt::Write;

pub(in crate::codegen) fn allocate(
    value: &CheckedExpression,
    ty: Type,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
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
    let array = context.arrays.get(ty.array_id().unwrap());
    let Type::Struct(struct_id) = array.element else {
        unreachable!("soa array element must be a struct");
    };
    let elem_c = array.element.c_name();
    let all_leaves = leaves(struct_id, context);
    let mut c = String::from("({ ");
    for count in &metadata.constant_counts {
        for leaf in &all_leaves {
            let l_ty = leaf.ty.c_name();
            write!(
                c,
                r#"_Static_assert(UINT64_C({count}) <= SIZE_MAX / sizeof({l_ty}), "array byte size overflows size_t [array.layout]"); "#
            )
            .unwrap();
        }
    }
    for (i, val) in values.iter().enumerate() {
        write!(
            c,
            "{elem_c} suz_initial_{i} = {}; ",
            expression(val, context)
        )
        .unwrap();
    }
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
        let invalid = panic_call(
            location,
            &format!("array dimension {} is outside len_t [rt.array-size]", i + 1),
            context,
        );
        let negative = if length.ty.integer_bounds().unwrap().0 < 0 {
            format!("suz_length_{i} < 0 || ")
        } else {
            String::new()
        };
        write!(
            c,
            "if ({negative}(uint64_t)suz_length_{i} > INT32_MAX) {invalid}; suz_array_value.lengths[{i}] = (int32_t)suz_length_{i}; "
        )
        .unwrap();
    }
    c.push_str("size_t suz_count = 1; ");
    let overflow = panic_call(
        location,
        "array layout overflows size_t [rt.array-size]",
        context,
    );
    for i in (0..lengths.len()).rev() {
        write!(
            c,
            "if (suz_length_{i} != 0 && suz_count > SIZE_MAX / (size_t)suz_length_{i}) {overflow}; suz_count *= (size_t)suz_length_{i}; "
        )
        .unwrap();
    }
    c.push_str("size_t suz_bytes = 0; size_t suz_max_align = 1; ");
    for (col_idx, leaf) in all_leaves.iter().enumerate() {
        let l_ty = leaf.ty.c_name();
        write!(
            c,
            "if (alignof({l_ty}) > suz_max_align) suz_max_align = alignof({l_ty}); \
             if (suz_bytes > SIZE_MAX - (alignof({l_ty}) - 1)) {overflow}; \
             size_t suz_off_{col_idx} = (suz_bytes + alignof({l_ty}) - 1) & ~(alignof({l_ty}) - 1); \
             if (suz_count > (SIZE_MAX - suz_off_{col_idx}) / sizeof({l_ty})) {overflow}; \
             suz_bytes = suz_off_{col_idx} + suz_count * sizeof({l_ty}); "
        )
        .unwrap();
    }
    write!(
        c,
        "unsigned char *suz_block = suz_arena_alloc(suz_bytes ? suz_bytes : 1, suz_max_align, {}, {}, {}); ",
        c_string(&context.source_name),
        location.line,
        location.column
    )
    .unwrap();
    for col_idx in 0..all_leaves.len() {
        write!(
            c,
            "suz_array_value.cols[{col_idx}] = suz_block + suz_off_{col_idx}; "
        )
        .unwrap();
    }
    if *repeated {
        let scatter = scatter_stmts(
            struct_id,
            "suz_array_value.cols",
            "suz_i",
            "suz_initial_0",
            0,
            context,
        );
        write!(
            c,
            "for (size_t suz_i = 0; suz_i < suz_count; ++suz_i) {{ {scatter} }} "
        )
        .unwrap();
    } else {
        for i in 0..values.len() {
            let slot = metadata.slots.get(i).copied().unwrap_or(i);
            let scatter = scatter_stmts(
                struct_id,
                "suz_array_value.cols",
                &slot.to_string(),
                &format!("suz_initial_{i}"),
                0,
                context,
            );
            c.push_str(&scatter);
        }
    }
    c.push_str("suz_array_value; })");
    c
}

fn extract_struct_leaf<'a>(expr: &'a CheckedExpression, path: &[usize]) -> &'a CheckedExpression {
    let mut cur = expr;
    for &idx in path {
        let Value::Struct { fields } = &cur.value else {
            unreachable!("constant soa element must be a struct value");
        };
        cur = &fields
            .iter()
            .find(|(i, _)| *i == idx)
            .expect("field in constant struct")
            .1;
    }
    cur
}

pub(in crate::codegen) fn constant(
    value: &CheckedExpression,
    context: &mut ExpressionContext,
) -> String {
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
    let Type::Struct(struct_id) = array.element else {
        unreachable!("soa array element must be a struct");
    };
    let all_leaves = leaves(struct_id, context);
    let lengths_str = lengths
        .iter()
        .map(|v| match v.value {
            Value::Integer { value, .. } => value.to_string(),
            _ => unreachable!("constant array dimension"),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let count = *metadata.constant_counts.last().unwrap();
    let mut col_initials: Vec<Vec<(String, bool)>> = Vec::with_capacity(all_leaves.len());
    for leaf in &all_leaves {
        let items = values
            .iter()
            .map(|v| constant_element(extract_struct_leaf(v, &leaf.path), context))
            .collect();
        col_initials.push(items);
    }
    let key = format!(
        "soa:{:?}:{lengths_str}:{repeated}:{}",
        value.ty.with_nilable(false),
        col_initials
            .iter()
            .flat_map(|items| items.iter().map(|(t, _)| t.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let id = if let Some(id) = context.constant_arrays.get(&key) {
        *id
    } else {
        let id = context.constant_arrays.len();
        context.constant_arrays.insert(key, id);
        let size = count.max(1);
        for (col_idx, (leaf, initial)) in all_leaves.iter().zip(&col_initials).enumerate() {
            let l_ty = leaf.ty.c_name();
            let name = format!("suz_constant_array_{id}_col_{col_idx}");
            for c_cnt in &metadata.constant_counts {
                writeln!(
                    context.constant_array_data,
                    r#"_Static_assert(UINT64_C({c_cnt}) <= SIZE_MAX / sizeof({l_ty}), "array byte size overflows size_t [array.layout]");"#
                )
                .unwrap();
            }
            let static_initial = initial.iter().all(|(_, lit)| *lit);
            if static_initial {
                let init_str = if count == 0 {
                    "0".into()
                } else if *repeated {
                    format!("[0 ... {}] = {}", count - 1, initial[0].0)
                } else {
                    initial
                        .iter()
                        .map(|(t, _)| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                writeln!(
                    context.constant_array_data,
                    "static {l_ty} const {name}[{size}] = {{ {init_str} }};"
                )
                .unwrap();
            } else {
                let static_values = initial
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, lit))| *lit)
                    .map(|(i, (t, _))| format!("[{i}] = {t}"))
                    .collect::<Vec<_>>();
                let init_str = if static_values.is_empty() {
                    "0".into()
                } else {
                    static_values.join(", ")
                };
                writeln!(
                    context.constant_array_data,
                    "static {l_ty} {name}[{size}] = {{ {init_str} }};"
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
                    for (index, (init, lit)) in initial.iter().enumerate() {
                        if *lit {
                            continue;
                        }
                        writeln!(context.constant_array_init, "    {name}[{index}] = {init};")
                            .unwrap();
                    }
                }
            }
        }
        id
    };
    let cols = (0..all_leaves.len())
        .map(|c| format!("(void *)suz_constant_array_{id}_col_{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "({}){{ .cols = {{ {cols} }}, .lengths = {{ {lengths_str} }} }}",
        value.ty.c_name()
    )
}
