//! SoA 配列参照の C23 コード生成（単一ブロック確保・列取り出し・gather/scatter・定数 SoA・要素更新）。
use super::{
    ExpressionContext,
    arrays::{constant_element, direct_reference, element_offset_parts, panic_call, temporary},
    expressions::{c_string, expression},
    structs::field_name,
};
use crate::{
    ast::{
        Location, Type,
        arrays::{SoaLeaf, soa_field_range, soa_leaves},
    },
    check::{CheckedExpression, CheckedStatement, Value},
};
use std::fmt::Write;

pub(super) fn leaves(struct_id: usize, context: &ExpressionContext) -> Vec<SoaLeaf> {
    soa_leaves(struct_id, &|sid| {
        context.structs[sid].fields.iter().map(|f| f.ty).collect()
    })
}

pub(super) fn field_leaf_range(
    struct_id: usize,
    path: &[usize],
    context: &ExpressionContext,
) -> (usize, usize) {
    let mut cur_struct = struct_id;
    let mut offset = 0;
    let mut count = leaves(struct_id, context).len();
    for &field_idx in path {
        let (sub_off, sub_cnt) = soa_field_range(cur_struct, field_idx, &|sid| {
            context.structs[sid].fields.iter().map(|f| f.ty).collect()
        });
        offset += sub_off;
        count = sub_cnt;
        if let Type::Struct(next_id) = context.structs[cur_struct].fields[field_idx].ty {
            cur_struct = next_id;
        }
    }
    (offset, count)
}

pub(super) fn gather_expr(
    struct_id: usize,
    cols_expr: &str,
    offset_expr: &str,
    leaf_start: usize,
    context: &ExpressionContext,
) -> (String, usize) {
    let decl = &context.structs[struct_id];
    let struct_ty = Type::Struct(struct_id);
    let mut cursor = leaf_start;
    let mut parts = Vec::with_capacity(decl.fields.len());
    for (idx, field) in decl.fields.iter().enumerate() {
        let fname = field_name(struct_ty, idx, context);
        let value = if let Type::Struct(sub_id) = field.ty {
            let (sub_expr, next) = gather_expr(sub_id, cols_expr, offset_expr, cursor, context);
            cursor = next;
            sub_expr
        } else {
            let c_ty = field.ty.c_name();
            let expr = format!("((const {c_ty} *)({cols_expr})[{cursor}])[{offset_expr}]");
            cursor += 1;
            expr
        };
        parts.push(format!(".{fname} = {value}"));
    }
    (
        format!("({}){{ {} }}", struct_ty.c_name(), parts.join(", ")),
        cursor,
    )
}

pub(super) fn scatter_stmts(
    struct_id: usize,
    cols_expr: &str,
    offset_expr: &str,
    val_expr: &str,
    leaf_start: usize,
    context: &ExpressionContext,
) -> String {
    let all_leaves = leaves(struct_id, context);
    let mut out = String::new();
    for (i, leaf) in all_leaves.iter().enumerate() {
        let c_ty = leaf.ty.c_name();
        let access = field_access_chain(Type::Struct(struct_id), &leaf.path, context);
        write!(
            out,
            "(({c_ty} *)({cols_expr})[{}])[{offset_expr}] = ({val_expr}){access}; ",
            leaf_start + i
        )
        .unwrap();
    }
    out
}

fn field_access_chain(base_ty: Type, path: &[usize], context: &ExpressionContext) -> String {
    let mut ty = base_ty;
    let mut out = String::new();
    for &idx in path {
        let fname = field_name(ty, idx, context);
        write!(out, ".{fname}").unwrap();
        let Type::Struct(sid) = ty else {
            unreachable!()
        };
        ty = context.structs[sid].fields[idx].ty;
    }
    out
}

pub(super) fn column(
    target: &CheckedExpression,
    leaf_offset: usize,
    leaf_count: usize,
    result_ty: Type,
    context: &mut ExpressionContext,
) -> String {
    let target_array = context.arrays.get(target.ty.array_id().unwrap());
    let result_array = context.arrays.get(result_ty.array_id().unwrap());
    let (setup, source) = if direct_reference(target) {
        (String::new(), format!("({})", expression(target, context)))
    } else {
        let source = temporary(target.ty, context);
        let value = expression(target, context);
        (format!("{source} = {value}, "), source)
    };
    let lengths = (0..target_array.rank)
        .map(|axis| format!("{source}.lengths[{axis}]"))
        .collect::<Vec<_>>()
        .join(", ");
    if result_array.soa {
        let cols = (leaf_offset..leaf_offset + leaf_count)
            .map(|c| format!("{source}.cols[{c}]"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "({setup}({}){{ .cols = {{ {cols} }}, .lengths = {{ {lengths} }} }})",
            result_ty.c_name()
        )
    } else {
        format!(
            "({setup}({}){{ .data = {source}.cols[{leaf_offset}], .lengths = {{ {lengths} }} }})",
            result_ty.c_name()
        )
    }
}

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

pub(super) fn constant(value: &CheckedExpression, context: &mut ExpressionContext) -> String {
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

/// `particles.[i].life` や `particles.[i].pos.x` の直接読み出しでは、不要な列を gather しない。
pub(super) fn try_direct_field_read(
    expr: &CheckedExpression,
    context: &mut ExpressionContext,
) -> Option<String> {
    let mut path = Vec::new();
    let mut cur = expr;
    while let Value::Field { target, index } = &cur.value {
        path.push(*index);
        cur = target;
    }
    if path.is_empty() {
        return None;
    }
    let Value::ArrayRead {
        address,
        handler: None,
    } = &cur.value
    else {
        return None;
    };
    let Value::ArrayElement { target, .. } = &address.value else {
        return None;
    };
    let array = context.arrays.get(target.ty.array_id()?);
    if !array.soa {
        return None;
    }
    let Type::Struct(struct_id) = array.element else {
        return None;
    };
    path.reverse();
    let (leaf_offset, leaf_count) = field_leaf_range(struct_id, &path, context);
    let (setup, condition, source, offset) = element_offset_parts(address, false, context);
    let value = if let Type::Struct(sub_id) = expr.ty {
        debug_assert!(leaf_count >= 1);
        gather_expr(
            sub_id,
            &format!("{source}.cols"),
            &offset,
            leaf_offset,
            context,
        )
        .0
    } else {
        let c_ty = expr.ty.c_name();
        format!("((const {c_ty} *){source}.cols[{leaf_offset}])[{offset}]")
    };
    Some(format!("({setup}, (void)({condition}), {value})"))
}

/// `particles#[i] = ...`、`particles.[i]#life = ...`、`particles.[i].pos#x += ...` を出力する。
#[allow(clippy::too_many_arguments)]
pub(super) fn try_emit_store(
    c: &mut String,
    target: &CheckedExpression,
    id: usize,
    value: &CheckedExpression,
    body: &[CheckedStatement],
    depth: usize,
    next_assignment: &mut usize,
    context: &mut ExpressionContext,
) -> bool {
    let mut path = Vec::new();
    let mut cur = target;
    while let Value::Field {
        target: inner,
        index,
    } = &cur.value
    {
        path.push(*index);
        cur = inner;
    }
    let Value::Deref(address) = &cur.value else {
        return false;
    };
    let Value::ArrayElement {
        target: array_target,
        ..
    } = &address.value
    else {
        return false;
    };
    let Some(array_id) = array_target.ty.array_id() else {
        return false;
    };
    let array = context.arrays.get(array_id);
    if !array.soa {
        return false;
    }
    let Type::Struct(struct_id) = array.element else {
        return false;
    };
    path.reverse();
    let (leaf_offset, _leaf_count) = field_leaf_range(struct_id, &path, context);
    let indent = "    ".repeat(depth);
    let (setup, condition, source, offset) = element_offset_parts(address, true, context);
    writeln!(c, "{indent}{{").unwrap();
    writeln!(c, "{indent}    {setup};").unwrap();
    writeln!(c, "{indent}    (void)({condition});").unwrap();
    writeln!(c, "{indent}    const size_t suz_soa_off_{id} = {offset};").unwrap();
    if let Type::Struct(sub_id) = target.ty {
        writeln!(
            c,
            "{indent}    void *const *const suz_soa_cols_{id} = {source}.cols;"
        )
        .unwrap();
        let cols_expr = format!("suz_soa_cols_{id}");
        if !body.is_empty() {
            let gathered = gather_expr(
                sub_id,
                &cols_expr,
                &format!("suz_soa_off_{id}"),
                leaf_offset,
                context,
            )
            .0;
            writeln!(
                c,
                "{indent}    {} suz_soa_tmp_{id} = {gathered};",
                target.ty.c_name()
            )
            .unwrap();
            writeln!(
                c,
                "{indent}    {} const suz_local_{id} = &suz_soa_tmp_{id};",
                target.ty.pointer().c_name()
            )
            .unwrap();
            super::statements::emit_block(c, body, depth + 1, next_assignment, context);
        }
        let val_c = expression(value, context);
        writeln!(
            c,
            "{indent}    const {} suz_soa_val_{id} = {val_c};",
            target.ty.c_name()
        )
        .unwrap();
        let scatter = scatter_stmts(
            sub_id,
            &cols_expr,
            &format!("suz_soa_off_{id}"),
            &format!("suz_soa_val_{id}"),
            leaf_offset,
            context,
        );
        writeln!(c, "{indent}    {scatter}").unwrap();
    } else {
        let leaf_ty = target.ty.c_name();
        writeln!(
            c,
            "{indent}    {leaf_ty} *const suz_local_{id} = (({leaf_ty} *){source}.cols[{leaf_offset}]) + suz_soa_off_{id};"
        )
        .unwrap();
        super::statements::emit_block(c, body, depth + 1, next_assignment, context);
        let val_c = expression(value, context);
        writeln!(c, "{indent}    *suz_local_{id} = {val_c};").unwrap();
    }
    writeln!(c, "{indent}}}").unwrap();
    true
}

/// `particles.[i]#method(...)` や `particles.[i].pos#method(...)` の `#` メソッド呼び出しで、
/// SoA 要素を一時変数へ gather して渡し、呼び出し後に scatter で書き戻す式を構築する。
pub(super) fn try_mutable_receiver(
    target: &CheckedExpression,
    setup: &mut Vec<String>,
    names: &mut Vec<String>,
    context: &mut ExpressionContext,
) -> Option<String> {
    let mut path = Vec::new();
    let mut cur = target;
    while let Value::Field {
        target: inner,
        index,
    } = &cur.value
    {
        path.push(*index);
        cur = inner;
    }
    let Value::Deref(address) = &cur.value else {
        return None;
    };
    let Value::ArrayElement {
        target: array_target,
        ..
    } = &address.value
    else {
        return None;
    };
    let array = context.arrays.get(array_target.ty.array_id()?);
    if !array.soa {
        return None;
    }
    let Type::Struct(struct_id) = array.element else {
        return None;
    };
    path.reverse();
    let (leaf_offset, _) = field_leaf_range(struct_id, &path, context);
    let (elem_setup, condition, source, offset) = element_offset_parts(address, true, context);

    let src_idx = context.temporaries.len();
    context.temporaries.push(array_target.ty);
    let src_tmp = format!("suz_cmp_{src_idx}");
    setup.push(format!(
        "{src_tmp} = ({elem_setup}, (void)({condition}), {source})"
    ));

    let off_idx = context.temporaries.len();
    context
        .temporaries
        .push(Type::Integer(crate::ast::IntegerType::U64));
    let off_tmp = format!("suz_cmp_{off_idx}");
    setup.push(format!("{off_tmp} = (uint64_t)({offset})"));

    let val_idx = context.temporaries.len();
    context.temporaries.push(target.ty);
    let val_tmp = format!("suz_cmp_{val_idx}");

    let writeback = if let Type::Struct(sub_id) = target.ty {
        let gathered = gather_expr(
            sub_id,
            &format!("{src_tmp}.cols"),
            &off_tmp,
            leaf_offset,
            context,
        )
        .0;
        setup.push(format!("{val_tmp} = {gathered}"));
        let sub_leaves = leaves(sub_id, context);
        sub_leaves
            .iter()
            .enumerate()
            .map(|(i, leaf)| {
                let c_ty = leaf.ty.c_name();
                let access = field_access_chain(target.ty, &leaf.path, context);
                format!(
                    "(({c_ty} *){src_tmp}.cols[{}])[{off_tmp}] = {val_tmp}{access}",
                    leaf_offset + i
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        let c_ty = target.ty.c_name();
        setup.push(format!(
            "{val_tmp} = ((const {c_ty} *){src_tmp}.cols[{leaf_offset}])[{off_tmp}]"
        ));
        format!("(({c_ty} *){src_tmp}.cols[{leaf_offset}])[{off_tmp}] = {val_tmp}")
    };
    names.push(format!("&{val_tmp}"));
    Some(writeback)
}
