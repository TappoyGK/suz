//! 構造体のC型、リテラル、比較に必要な補助処理を生成する。
//!
//! 利用元: [`crate::codegen`]、[`crate::codegen::expressions`]。

use super::{ExpressionContext, expressions::expression};
use crate::{ast::Type, check::CheckedExpression};
use std::{collections::BTreeSet, fmt::Write};

/// 外部構造体はヘッダーの実際のメンバー名でアクセスする。
pub(super) fn field_name(ty: Type, index: usize, context: &ExpressionContext) -> String {
    let Type::Struct(id) = ty else {
        unreachable!("checked struct field")
    };
    if context.structs[id].external.is_some() {
        context.structs[id].fields[index].name.clone()
    } else {
        format!("suz_field_{index}")
    }
}

pub(super) fn literal(
    id: usize,
    fields: &[(usize, CheckedExpression)],
    context: &mut ExpressionContext,
) -> String {
    let mut setup = Vec::new();
    let mut initializers = Vec::new();
    for (field, value) in fields {
        let index = context.temporaries.len();
        context.temporaries.push(value.ty);
        let value = expression(value, context);
        setup.push(format!("suz_cmp_{index} = {value}"));
        initializers.push(format!(
            ".{} = suz_cmp_{index}",
            field_name(Type::Struct(id), *field, context)
        ));
    }
    setup.push(format!(
        "(suz_struct_{id}){{ {} }}",
        initializers.join(", ")
    ));
    format!("({})", setup.join(", "))
}

pub(super) fn with(
    target: &CheckedExpression,
    fields: &[(usize, CheckedExpression)],
    context: &mut ExpressionContext,
) -> String {
    let index = context.temporaries.len();
    context.temporaries.push(target.ty);
    // 初期値の呼び出しがコピー元を書き換える前に、構造体全体を保存する。
    let mut setup = vec![format!("suz_cmp_{index} = {}", expression(target, context))];
    for (field, value) in fields {
        let name = field_name(target.ty, *field, context);
        setup.push(format!(
            "suz_cmp_{index}.{name} = {}",
            expression(value, context)
        ));
    }
    setup.push(format!("suz_cmp_{index}"));
    format!("({})", setup.join(", "))
}

pub(super) fn emit(c: &mut String, context: &ExpressionContext) {
    for (id, declaration) in context.structs.iter().enumerate() {
        if declaration.symbolic {
            continue;
        }
        let tag = declaration
            .external
            .as_ref()
            .map_or_else(|| format!("suz_struct_{id}"), |s| s.c_name.clone());
        writeln!(c, "typedef struct {tag} suz_struct_{id};").unwrap();
    }
    super::functions::type_declarations(c, context);
    fn declaration(
        c: &mut String,
        id: usize,
        context: &ExpressionContext,
        emitted: &mut BTreeSet<usize>,
    ) {
        if !emitted.insert(id) {
            return;
        }
        if context.structs[id].external.is_some() || context.structs[id].symbolic {
            return;
        }
        for field in &context.structs[id].fields {
            if let Type::Struct(child) = field.ty {
                declaration(c, child, context, emitted);
            }
        }
        writeln!(c, "struct suz_struct_{id} {{").unwrap();
        for (index, field) in context.structs[id].fields.iter().enumerate() {
            writeln!(c, "    {} suz_field_{index};", field.ty.c_name()).unwrap();
        }
        c.push_str("};\n\n");
    }
    let mut emitted = BTreeSet::new();
    for id in 0..context.structs.len() {
        declaration(c, id, context, &mut emitted);
    }
    fn equality(
        c: &mut String,
        id: usize,
        context: &ExpressionContext,
        emitted: &mut BTreeSet<usize>,
    ) {
        if !emitted.insert(id) {
            return;
        }
        for field in &context.structs[id].fields {
            if let Type::Struct(child) = field.ty {
                equality(c, child, context, emitted);
            }
        }
        let comparisons = context.structs[id]
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                super::expressions::equality(
                    field.ty,
                    &format!("left.{}", field_name(Type::Struct(id), index, context)),
                    &format!("right.{}", field_name(Type::Struct(id), index, context)),
                )
            })
            .collect::<Vec<_>>()
            .join(" && ");
        writeln!(c, "static inline _Bool suz_equal_{id}(suz_struct_{id} left, suz_struct_{id} right) {{\n    return {comparisons};\n}}\n").unwrap();
    }
    emitted.clear();
    for id in &context.struct_equals {
        equality(c, *id, context, &mut emitted);
    }
}
