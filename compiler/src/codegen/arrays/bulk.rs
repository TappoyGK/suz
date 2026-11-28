//! 1次元配列への一括書き込み（`out#[] = 式`・`out#[s:e] = 式`・マスク付き書き込み）の C23 コード生成。
use super::super::{
    ExpressionContext,
    expressions::{c_string, expression},
};
use crate::{
    ast::{Location, Type},
    check::{CheckedExpression, Value},
};
use std::fmt::Write;

/// 1次元配列への一括書き込み（`out#[] = 式`、`out#[s:e] = 式`、複合代入）をC23へ出力する。
/// オペランドを左から右へ1回評価し、ループ前に範囲・長さを1回検査してから、
/// 非重複・完全一致・ずれた重なりに応じてC23 §6.7.3.1に準拠するループを切り替える。
pub(in crate::codegen) fn emit_bulk_write(
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
    let out_array = context
        .arrays
        .get(bulk.target.ty.array_id().unwrap())
        .clone();
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
        (
            super::dim_length_expr(&out_array, &format!("suz_bulk_out_{bid}"), None, 0),
            None,
        )
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
        let len_expr = super::dim_length_expr(&out_array, &format!("suz_bulk_out_{bid}"), None, 0);
        writeln!(
            c,
            "{inner}const size_t suz_bulk_n_{bid} = (size_t){len_expr};"
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
            let fail = super::panic(
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
                super::super::soa::gather_expr(
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
                            super::super::soa::gather_expr(
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
                        super::super::soa::gather_expr(
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
            let scatter = super::super::soa::scatter_stmts(
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
