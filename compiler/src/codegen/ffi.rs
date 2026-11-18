//! Cヘッダーとの型照合と、C環境の整数範囲に依存する変換を生成する。
use super::{ExpressionContext, expressions::expression};
use crate::{
    ast::{IntegerType, Type},
    check::{CheckedExpression, ir::CheckedProgram},
};
use std::fmt::Write;

pub(super) fn declarations(c: &mut String, program: &CheckedProgram) {
    c.push_str("_Static_assert(sizeof(long) <= 8 && sizeof(size_t) <= 8, \"Suz FFI requires C integers of at most 64 bits\");\n");
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
            "#ifdef {}\n#error \"[ffi.header-type] function macros are not supported\"\n#endif",
            f.c_name
        )
        .unwrap();
        writeln!(c, "_Static_assert(__builtin_types_compatible_p(__typeof__(&{}), {} (*)({types})), \"[ffi.header-type] incompatible C function: {}\");", f.c_name, f.result.c_name(), f.c_name).unwrap();
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
