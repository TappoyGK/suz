//! Cヘッダーとの型照合、C固有型の変換、文字列のバイト列取得を生成する。
use super::{ExpressionContext, expressions::expression};
use crate::{
    ast::{IntegerType, Location, Type},
    check::{CheckedExpression, ir::CheckedProgram},
};
use std::fmt::Write;

pub(super) fn declarations(c: &mut String, program: &CheckedProgram) {
    if program.externs.is_empty() {
        return;
    }
    c.push_str("#if !__has_builtin(__builtin_types_compatible_p)\n#error \"C function type checking requires __builtin_types_compatible_p\"\n#endif\n");
    let mut headers = std::collections::BTreeSet::new();
    for f in &program.externs {
        if headers.insert(&f.header) {
            writeln!(c, "#include <{}>", f.header).unwrap();
        }
    }
    for (index, f) in program.externs.iter().enumerate() {
        let id = program.external_base + index;
        let types = if f.parameters.is_empty() {
            "void".into()
        } else {
            f.parameters
                .iter()
                .map(|p| p.ty.c_name())
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
            .map(|(i, p)| format!("{} a{i}", p.ty.c_name()))
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
        // 64ビットsize_tでは検査自体を残さず、狭い対象だけで切り詰めを防ぐ。
        c.push_str(r#"static inline size_t suz_ffi_byte_len(uint64_t length, const char *source, size_t line, size_t column) {
#if SIZE_MAX < UINT64_MAX
    if (length > SIZE_MAX) suz_panic(source, line, column, "byte length exceeds size_t [rt.ffi-byte-len]");
#else
    (void)source; (void)line; (void)column;
#endif
    return (size_t)length;
}
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
