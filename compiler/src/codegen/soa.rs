//! SoA 配列参照の C23 コード生成（単一ブロック確保・列取り出し・gather/scatter・定数 SoA・要素更新）。
mod init;
use super::{
    ExpressionContext,
    arrays::{direct_reference, element_offset_parts, temporary},
    expressions::expression,
    structs::field_name,
};
use crate::{
    ast::{
        Type,
        arrays::{SoaLeaf, soa_field_range, soa_leaves},
    },
    check::{CheckedExpression, CheckedStatement, Value},
};
pub(super) use init::{allocate, constant};
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
    let lengths = (0..target_array.rank())
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
