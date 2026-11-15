use super::{ExpressionContext, expressions::c_string};
use crate::ast::{FloatType, Type};
use std::{collections::BTreeMap, fmt::Write};

fn key(ty: Type) -> String {
    match ty {
        Type::Struct(id) => format!("struct_{id}"),
        Type::Enum(ty) => format!("enum_{}", ty.id),
        _ => ty.name().into(),
    }
}

pub(super) fn prepare(context: &mut ExpressionContext) {
    fn visit(
        ty: Type,
        structs: &[crate::ast::StructDeclaration],
        bits: &mut std::collections::BTreeSet<crate::ast::IntegerType>,
    ) {
        match ty {
            Type::Integer(ty) => {
                bits.insert(ty);
            }
            Type::Struct(id) => {
                for field in &structs[id].fields {
                    visit(field.ty, structs, bits);
                }
            }
            _ => {}
        }
    }
    for id in &context.struct_parsers {
        visit(
            Type::Struct(*id),
            &context.structs,
            &mut context.bit_helpers,
        );
    }
}

pub(super) fn emit(c: &mut String, context: &ExpressionContext) {
    fn schema(
        c: &mut String,
        ty: Type,
        context: &ExpressionContext,
        emitted: &mut BTreeMap<String, usize>,
    ) -> usize {
        let key = key(ty);
        if let Some(slots) = emitted.get(&key) {
            return *slots;
        }
        let (kind, bits, signed, name, fields, count, slots) = match ty {
            Type::Integer(ty) => (
                "SUZ_PARSE_INTEGER",
                ty.bits(),
                ty.bounds().0 < 0,
                "NULL".into(),
                "NULL".into(),
                0,
                1,
            ),
            Type::Float(ty) => (
                "SUZ_PARSE_FLOAT",
                if ty == FloatType::F32 { 32 } else { 64 },
                false,
                "NULL".into(),
                "NULL".into(),
                0,
                1,
            ),
            Type::Char => (
                "SUZ_PARSE_CHAR",
                0,
                false,
                "NULL".into(),
                "NULL".into(),
                0,
                1,
            ),
            Type::Bool => (
                "SUZ_PARSE_BOOL",
                0,
                false,
                "NULL".into(),
                "NULL".into(),
                0,
                1,
            ),
            Type::Symbol | Type::Text => (
                "SUZ_PARSE_TEXT",
                0,
                false,
                "NULL".into(),
                "NULL".into(),
                0,
                1,
            ),
            Type::Enum(ty) => {
                let decl = &context.enums[ty.id];
                writeln!(
                    c,
                    "static const suz_parse_field suz_schema_{key}_fields[] = {{"
                )
                .unwrap();
                for name in &decl.variants {
                    writeln!(c, "    {{{}, NULL, 0}},", c_string(name)).unwrap();
                }
                c.push_str("};\n");
                (
                    "SUZ_PARSE_ENUM",
                    0,
                    false,
                    c_string(&decl.name),
                    format!("suz_schema_{key}_fields"),
                    decl.variants.len(),
                    1,
                )
            }
            Type::Struct(id) => {
                let decl = &context.structs[id];
                let mut slots = 0;
                let mut fields = String::new();
                for field in &decl.fields {
                    let child_slots = schema(c, field.ty, context, emitted);
                    writeln!(
                        fields,
                        "    {{{}, &suz_schema_{}, {slots}}},",
                        c_string(&field.name),
                        self::key(field.ty)
                    )
                    .unwrap();
                    slots += child_slots;
                }
                writeln!(
                    c,
                    "static const suz_parse_field suz_schema_{key}_fields[] = {{\n{fields}}};"
                )
                .unwrap();
                (
                    "SUZ_PARSE_STRUCT",
                    0,
                    false,
                    c_string(&decl.name),
                    format!("suz_schema_{key}_fields"),
                    decl.fields.len(),
                    slots,
                )
            }
            Type::Exact | Type::Void => unreachable!(),
        };
        writeln!(c, "static const suz_parse_type suz_schema_{key} = {{{kind}, {bits}, {}, {name}, {fields}, {count}, {slots}}};", u8::from(signed)).unwrap();
        emitted.insert(key, slots);
        slots
    }
    fn assign(c: &mut String, ty: Type, path: &str, slot: &mut usize, context: &ExpressionContext) {
        if let Type::Struct(id) = ty {
            for (index, field) in context.structs[id].fields.iter().enumerate() {
                assign(
                    c,
                    field.ty,
                    &format!("{path}.suz_field_{index}"),
                    slot,
                    context,
                );
            }
            return;
        }
        let index = *slot;
        *slot += 1;
        let value = match ty {
            Type::Integer(ty) => format!("suz_bits_{}(values[{index}].bits)", ty.name()),
            Type::Enum(_) | Type::Bool | Type::Char => {
                format!("({})(values[{index}].bits)", ty.c_name())
            }
            Type::Float(ty) => format!("suz_parsed_{}(values[{index}].bits)", ty.name()),
            Type::Symbol | Type::Text => format!(
                "suz_text_from_parsed(bytes + values[{index}].start, values[{index}].length, location)"
            ),
            _ => unreachable!(),
        };
        let value = if ty == Type::Text {
            format!("suz_text_as_view({value})")
        } else {
            value
        };
        writeln!(c, "    {path} = {value};").unwrap();
    }
    let mut emitted = BTreeMap::new();
    for id in &context.struct_parsers {
        let ty = Type::Struct(*id);
        let slots = schema(c, ty, context, &mut emitted);
        writeln!(c, "static inline _Bool suz_parse_struct_{id}(suz_text_view input, suz_struct_{id} *output, suz_text_location location) {{\n    const char *bytes = suz_view_bytes(input);\n    suz_parsed_value values[{slots}];\n    if (!suz_parse_struct(bytes, suz_view_length(input), &suz_schema_struct_{id}, values)) return 0;\n    suz_struct_{id} result;").unwrap();
        // Structures without text fields still use the same cast helper signature.
        c.push_str("    (void)location;\n");
        assign(c, ty, "result", &mut 0, context);
        c.push_str("    *output = result;\n    return 1;\n}\n\n");
    }
}
