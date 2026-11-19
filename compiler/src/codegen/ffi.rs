//! Cヘッダーとの型照合、C固有型の変換、文字列のバイト列取得を生成する。
use super::{ExpressionContext, expressions::expression};
use crate::{
    ast::{IntegerType, Location, Type},
    check::{CheckedExpression, ir::CheckedProgram},
};
use std::fmt::Write;

pub(super) fn declarations(c: &mut String, program: &CheckedProgram) {
    if program.externs.is_empty() && !program.structs.iter().any(|s| s.external.is_some()) {
        return;
    }
    c.push_str("#if !__has_builtin(__builtin_types_compatible_p)\n#error \"C interface type checking requires __builtin_types_compatible_p\"\n#endif\n");
    let mut headers = std::collections::BTreeSet::new();
    for header in program.externs.iter().map(|f| &f.header).chain(
        program
            .structs
            .iter()
            .filter_map(|s| s.external.as_ref().map(|s| &s.header)),
    ) {
        if headers.insert(header) {
            writeln!(c, "#include <{header}>").unwrap();
        }
    }
    for (id, declaration) in program.structs.iter().enumerate() {
        if let Some(external) = &declaration.external {
            struct_checks(c, id, declaration, external);
        }
    }
    for (index, f) in program.externs.iter().enumerate() {
        let id = program.external_base + index;
        let types = if f.parameters.is_empty() {
            "void".into()
        } else {
            f.parameters
                .iter()
                .map(|p| parameter_type(p, program))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(
            c,
            "#ifdef {}\n#error \"[tool.ffi-header] function macros are not supported\"\n#endif",
            f.c_name
        )
        .unwrap();
        writeln!(c, "_Static_assert(__builtin_types_compatible_p(__typeof__(&{}), {} (*)({types})), \"[tool.ffi-header] incompatible C function: {}\");", f.c_name, f.result.c_name(), f.c_name).unwrap();
        let args = f
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{} a{i}", parameter_type(p, program)))
            .collect::<Vec<_>>()
            .join(", ");
        let names = (0..f.parameters.len())
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let args = if args.is_empty() { "void" } else { &args };
        let returned = if f.result == Type::Void {
            ""
        } else {
            "return "
        };
        // 未使用の宣言でもヘッダー照合は行うが、リンク対象は実際に呼ぶ関数だけ。
        writeln!(c, "static inline __attribute__((unused)) {} suz_fn_{id}({args}) {{ {returned}{}({names}); }}", f.result.c_name(), f.c_name).unwrap();
    }
}

/// C側のconstとSuz側のポインタ束縛を区別する。構造体はヘッダーのタグで照合する。
fn parameter_type(parameter: &crate::ast::ExternParameter, program: &CheckedProgram) -> String {
    let Some(target) = parameter.ty.pointee() else {
        return parameter.ty.c_name();
    };
    let target = if let Type::Struct(id) = target {
        format!(
            "struct {}",
            program.structs[id]
                .external
                .as_ref()
                .expect("checked C struct pointer")
                .c_name
        )
    } else {
        target.c_name()
    };
    let qualifier = if parameter.read_only { "const " } else { "" };
    format!("{qualifier}{target} *")
}

/// 実体にはヘッダーの型を使い、Suzが宣言した通常のC配置との一致を検査する。
/// 同じサイズの別型へのキャストでは、Cの型に基づく別名規則を満たせない。
fn struct_checks(
    c: &mut String,
    id: usize,
    declaration: &crate::ast::StructDeclaration,
    external: &crate::ast::ExternStruct,
) {
    let tag = &external.c_name;
    writeln!(
        c,
        "#ifdef {tag}\n#error \"[tool.ffi-header] struct tag macros are not supported\"\n#endif"
    )
    .unwrap();
    writeln!(c, "struct suz_ffi_layout_{id} {{").unwrap();
    for (index, field) in declaration.fields.iter().enumerate() {
        writeln!(c, "    {} suz_field_{index};", field.ty.c_name()).unwrap();
    }
    c.push_str("};\n");
    for property in ["sizeof", "_Alignof"] {
        writeln!(c, "_Static_assert({property}(struct {tag}) == {property}(struct suz_ffi_layout_{id}), \"[tool.ffi-header] incompatible C struct layout: {tag}\");").unwrap();
    }
    for (index, field) in declaration.fields.iter().enumerate() {
        let name = &field.name;
        writeln!(c, "#ifdef {name}\n#error \"[tool.ffi-header] struct field macros are not supported\"\n#endif").unwrap();
        writeln!(c, "_Static_assert(offsetof(struct {tag}, {name}) == offsetof(struct suz_ffi_layout_{id}, suz_field_{index}), \"[tool.ffi-header] incompatible C field offset: {tag}.{name}\");").unwrap();
        writeln!(c, "_Static_assert(__builtin_types_compatible_p(__typeof__(((struct {tag} *)0)->{name}), {}), \"[tool.ffi-header] incompatible C field type: {tag}.{name}\");", field.ty.c_name()).unwrap();
        // types_compatible_pはトップレベルのconst/volatileを無視するため、アドレスの型も照合する。
        writeln!(c, "_Static_assert(__builtin_types_compatible_p(__typeof__(&((struct {tag} *)0)->{name}), {} *), \"[tool.ffi-header] incompatible C field qualifiers: {tag}.{name}\");", field.ty.c_name()).unwrap();
    }
}

fn bounds(ty: Type) -> (String, String) {
    match ty {
        Type::CInteger(t) => {
            let (lo, hi) = t.limits();
            (lo.into(), hi.into())
        }
        Type::Integer(t) => {
            let (lo, hi) = t.bounds();
            (
                format!("(-(_BitInt(128)){}ULL)", -lo),
                format!("((unsigned _BitInt(128)){hi}ULL)"),
            )
        }
        _ => unreachable!(),
    }
}

pub(super) fn exact_fits(ty: Type, value: &str) -> String {
    let (lo, hi) = bounds(ty);
    format!(
        "(!{value}.failed && ({value}.negative ? {value}.magnitude <= (unsigned _BitInt(128))(-(_BitInt(128))({lo})) : {value}.magnitude <= (unsigned _BitInt(128))({hi})))"
    )
}

pub(super) fn exact_converted(ty: Type, value: &str) -> String {
    format!(
        "({})({value}.negative ? -(_BitInt(128)){value}.magnitude : (_BitInt(128)){value}.magnitude)",
        ty.c_name()
    )
}

pub(super) fn cast_parts(
    operand: &CheckedExpression,
    target: Type,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    let id = context.temporaries.len();
    context.temporaries.push(operand.ty);
    let value = expression(operand, context);
    let name = format!("suz_cmp_{id}");
    let setup = format!("{name} = {value}");
    if operand.ty == Type::Exact {
        return (
            setup,
            exact_fits(target, &name),
            exact_converted(target, &name),
        );
    }
    let converted = format!("({}){name}", target.c_name());
    if matches!(target, Type::Float(_)) {
        return (setup, "1".into(), converted);
    }
    let (lo, hi) = bounds(target);
    let condition = if matches!(operand.ty, Type::Float(_)) {
        // 0方向への丸めを考慮し、下限-1より大きい値まで受け付ける。
        // 64ビットの下限ではdoubleが下限-1を区別できないため、下限以上で検査する。
        // 上限+1は2のべき乗なのでdoubleでも正確に比較できる。
        format!(
            "(((double)((_BitInt(128))({lo}) - 1) == (double)({lo}) ? (double){name} >= (double)({lo}) : (double){name} > (double)((_BitInt(128))({lo}) - 1)) && (double){name} < (double)((unsigned _BitInt(128))({hi}) + 1))"
        )
    } else {
        format!(
            "((_BitInt(128)){name} >= (_BitInt(128))({lo}) && (_BitInt(128)){name} <= (_BitInt(128))({hi}))"
        )
    };
    (setup, condition, converted)
}

/// Cの符号付き狭小化の処理系依存動作を避け、下位ビットを符号へ写す。
pub(super) fn truncate(
    operand: &CheckedExpression,
    target: Type,
    context: &mut ExpressionContext,
) -> String {
    let id = context.temporaries.len();
    context.temporaries.push(Type::Integer(IntegerType::U64));
    let name = format!("suz_cmp_{id}");
    let value = expression(operand, context);
    let value = if operand.ty == Type::Exact {
        format!("suz_exact_bits({value})")
    } else {
        format!("(uint64_t)({value})")
    };
    let (lo, hi) = bounds(target);
    let bits = format!("(sizeof({}) * CHAR_BIT)", target.c_name());
    let mask = format!("(UINT64_MAX >> (64 - {bits}))");
    let converted = format!(
        "((_BitInt(128))({lo}) < 0 && {name} > (uint64_t)({hi}) ? -(_BitInt(128))({mask} - {name}) - 1 : (_BitInt(128)){name})"
    );
    format!(
        "({name} = {value} & {mask}, ({}){converted})",
        target.c_name()
    )
}

/// 宣言だけのC型も含めて検出し、FFIを使わないプログラムには制約を追加しない。
pub(super) fn checks(c: &mut String, program: &CheckedProgram, context: &ExpressionContext) {
    let uses_type = |predicate: fn(&Type) -> bool| {
        program
            .functions
            .iter()
            .any(|f| predicate(&f.result) || f.parameters.iter().any(predicate))
            || program
                .function_types
                .iter()
                .any(|f| predicate(&f.result) || f.parameters.iter().any(predicate))
            || program
                .structs
                .iter()
                .any(|s| s.fields.iter().any(|f| predicate(&f.ty)))
            || program
                .externs
                .iter()
                .any(|f| predicate(&f.result) || f.parameters.iter().any(|p| predicate(&p.ty)))
    };
    if context.uses_c_pointer || uses_type(|ty| matches!(ty, Type::CPointer)) {
        // 参照先のconstと、Suzの固定束縛に付けるconstを分離する。
        c.push_str("typedef const void *suz_ffi_bytes;\n");
    }
    let uses_c_integer = context.uses_c_integer || uses_type(|ty| matches!(ty, Type::CInteger(_)));
    if uses_c_integer || !program.externs.is_empty() {
        c.push_str("_Static_assert(sizeof(long) <= 8 && sizeof(size_t) <= 8, \"Suz FFI requires C integers of at most 64 bits\");\n");
    }
    if context.uses_c_byte_len {
        // 64ビットsize_tでは最適化なしでも呼び出しを残さず、狭い対象だけで切り詰めを防ぐ。
        c.push_str(r#"#if SIZE_MAX < UINT64_MAX
static inline size_t suz_ffi_byte_len(uint64_t length, const char *source, size_t line, size_t column) {
    if (length > SIZE_MAX) suz_panic(source, line, column, "byte length exceeds size_t [rt.ffi-byte-len]");
    return (size_t)length;
}
#else
#define suz_ffi_byte_len(length, source, line, column) ((size_t)(length))
#endif
"#);
    }
    for check in &context.c_literal_checks {
        c.push_str(check);
    }
}

/// 対象C環境が分かる段でリテラルを検査し、実行時の関数呼び出しや分岐を作らない。
pub(super) fn literal(
    value: crate::numeric::Int,
    ty: Type,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    let (lo, hi) = bounds(ty);
    let (condition, number) = if value.magnitude > u64::MAX as u128 {
        // どの対応C型にも収まらない。C自身の整数リテラルの桁数制限には依存しない。
        ("0".to_owned(), "0".to_owned())
    } else {
        let sign = if value.negative { "-" } else { "" };
        let number = format!("({sign}(_BitInt(128))UINT64_C({}))", value.magnitude);
        (
            format!("{number} >= (_BitInt(128))({lo}) && {number} <= (_BitInt(128))({hi})"),
            number,
        )
    };
    let message = super::expressions::c_string(&format!(
        "{}:{}:{}: integer literal is outside {} [ffi.literal-range]",
        context.source_name,
        location.line,
        location.column,
        ty.name()
    ));
    context
        .c_literal_checks
        .insert(format!("_Static_assert({condition}, {message});\n"));
    format!("({}){number}", ty.c_name())
}

/// symbolの実体またはtextのビューから、コピーせずに先頭アドレスを取り出す。
pub(super) fn bytes(operand: &CheckedExpression, context: &mut ExpressionContext) -> String {
    let value = expression(operand, context);
    let bytes = if operand.ty == Type::Symbol {
        format!("({value})->bytes")
    } else {
        format!("({value}).bytes")
    };
    format!("((const void *){bytes})")
}

/// 文字数ではなくバイト数を読み、Cのsize_tへ切り詰めずに変換する。
pub(super) fn byte_len(
    operand: &CheckedExpression,
    location: Location,
    context: &mut ExpressionContext,
) -> String {
    context.uses_c_byte_len = true;
    let value = expression(operand, context);
    let length = if operand.ty == Type::Symbol {
        format!("({value})->length")
    } else {
        format!("({value}).length")
    };
    format!(
        "suz_ffi_byte_len({length}, {}, {}, {})",
        super::expressions::c_string(&context.source_name),
        location.line,
        location.column
    )
}
