//! 構造体の値仮引数のポインタ渡し（`[codegen.struct-param]`）と、
//! 実引数のコピー有無（`[codegen.struct-arg]`）が生成Cに反映されることを検証する。
use super::generate;

fn compile_c(source: &str) -> String {
    let (program, _) =
        crate::analyze(source.as_bytes(), &crate::CompilationOptions::default()).unwrap();
    generate(
        &program,
        "struct-params.suz",
        crate::SymbolPoolLimit::default(),
        crate::ArenaOptions::default(),
        crate::LogLevel::default(),
        false,
        None,
    )
}

#[test]
fn struct_value_parameters_use_const_restrict_pointers_across_all_callables() {
    let source = r#"struct Inner { x: i64 }
struct Outer { inner: Inner, y: i64 }
fnshape ReadOuter(Outer) -> i64
fn Outer.get() -> i64 { return self.inner.x }
fn Outer#bump() -> void { self#y = self.y + 1 }
fn Outer.<(other: Outer) -> bool { return self.y < other.y }
fn take(a: Outer, arr: .[Outer]) -> i64 { return a.y + (len arr as i64) }
fn generic[T](v: T) -> T { return v }
fn apply_static(f: static fn ReadOuter, a: Outer) -> i64 { return f(a) }
fn apply_fn(f: fn ReadOuter, a: Outer) -> i64 { return f(a) }
fn read_outer(a: Outer) -> i64 { return a.y }
fn main() -> u8 {
    fix a = Outer { inner: Inner { x: 1 }, y: 2 }
    var b = a
    b#bump()
    fix arr: .[Outer] = .[Outer { inner: Inner { x: 1 }, y: 2 }]
    fix c = generic(a)
    fix s = apply_static(fn read_outer, a)
    fix f1 = apply_fn(fn read_outer, a)
    fix f2 = apply_fn(fn [a](p: Outer) -> i64 { return p.inner.x + a.y }, a)
    if a.get() + take(c, arr) + s + f1 + f2 > 0 && a < b { return 0 }
    return 1
}
"#;
    let c = compile_c(source);
    // 関数・メソッド・演算子・ジェネリック具体化・関数型・アダプタ・クロージャの構造体仮引数は const T *restrict。
    assert!(
        c.contains("const suz_struct_1 *restrict suz_ref_0"),
        "missing struct value parameter pointer:\n{c}"
    );
    // # メソッドのレシーバは従来どおり非 const ポインタ。
    assert!(
        c.contains("suz_struct_1 *const suz_ref_0"),
        "missing mutable receiver pointer:\n{c}"
    );
    // 配列参照は構造体の値仮引数ではないため値渡しのまま。
    assert!(
        c.contains("const suz_readonly_array_0 suz_local_1"),
        "array reference parameter should remain by-value:\n{c}"
    );
    // static fn / fn / アダプタ / クロージャの署名。
    assert!(
        c.contains("typedef int64_t (*suz_fntype_0)(const suz_struct_1 *restrict);"),
        "missing fntype pointer param:\n{c}"
    );
    assert!(
        c.contains("int64_t (*call)(void *, const suz_struct_1 *restrict)"),
        "missing callback pointer param:\n{c}"
    );
    assert!(
        c.contains("suz_adapt_0(void *environment, const suz_struct_1 *restrict argument_0)"),
        "missing adapt pointer param:\n{c}"
    );
    assert!(
        c.contains("suz_closure_0(void *environment, const suz_struct_1 *restrict suz_ref_0)"),
        "missing closure pointer param:\n{c}"
    );
    // 値キャプチャーした構造体もクロージャ入口でコピーせずポインタで束縛する。
    assert!(
        c.contains("const suz_struct_1 *const suz_ref_1 = &((const suz_closure_env_0 *)environment)->capture_0;"),
        "value-captured struct should be bound by const pointer:\n{c}"
    );
}

#[test]
fn struct_arguments_avoid_copies_for_immutable_places_and_copy_mutable_sources() {
    let source = r#"struct Inner { x: i64 }
struct Outer { inner: Inner, y: i64 }
fn read_outer(a: Outer) -> i64 { return a.y }
fn read_inner(a: Inner) -> i64 { return a.x }
fn Outer.sum() -> i64 { return self.inner.x + self.y }
fn Outer.<(other: Outer) -> bool { return self.y < other.y }
fn Outer#pass_self() -> i64 { return read_outer(self) }
fn make() -> Outer { return Outer { inner: Inner { x: 1 }, y: 2 } }
fn from_param(p: Outer) -> i64 {
    return read_outer(p) + read_inner(p.inner)
}
fn from_fix() -> i64 {
    fix a = Outer { inner: Inner { x: 1 }, y: 2 }
    fix b = Outer { inner: Inner { x: 3 }, y: 4 }
    fix ok = a < b
    return read_outer(a) + read_inner(a.inner) + a.sum() + (ifexpr ok { 1 } else { 0 })
}
fn from_var() -> i64 {
    var v = Outer { inner: Inner { x: 1 }, y: 2 }
    v#y = 3
    return read_outer(v) + read_inner(v.inner)
}
fn from_deref() -> i64 {
    fix p = new Outer { inner: Inner { x: 1 }, y: 2 }
    return read_outer(.p) + read_inner(p.inner)
}
fn from_rvalue() -> i64 {
    return read_outer(make())
}
fn main() -> u8 {
    var m = make()
    fix total = from_param(make()) + from_fix() + from_var() + from_deref() + from_rvalue() + m#pass_self()
    return ifexpr total > 0 { 0 } else { 1 }
}
"#;
    let c = compile_c(source);

    // 関数本体を抜き出す簡易ヘルパー。
    let body_of = |index: usize| -> &str {
        let marker = format!(" suz_fn_{index}(");
        let mut search = c.as_str();
        while let Some(pos) = search.find(&marker) {
            let tail = &search[pos..];
            let line_end = tail.find('\n').unwrap_or(tail.len());
            if tail[..line_end].ends_with(") {") {
                let end = tail.find("\n}\n").expect("function end");
                return &tail[..end];
            }
            search = &tail[marker.len()..];
        }
        panic!("definition for suz_fn_{index} not found:\n{c}");
    };

    // Outer#pass_self (id 4): # メソッドの self は一時変数へ 1 回コピーして渡す。
    let pass_self = body_of(4);
    assert!(
        pass_self.contains("suz_struct_1 suz_cmp_0;")
            && pass_self.contains("suz_cmp_0 = (*suz_ref_0)"),
        "mutable receiver self must copy before passing by value:\n{pass_self}"
    );

    // from_param (id 6): 値仮引数とそのフィールドは一時変数へコピーせず直接アドレスを渡す。
    let from_param = body_of(6);
    assert!(
        !from_param.contains("suz_struct_0 suz_cmp_")
            && !from_param.contains("suz_struct_1 suz_cmp_")
            && from_param.contains("&((*suz_ref_0))")
            && from_param.contains("&(((*suz_ref_0)).suz_field_0)"),
        "from_param should pass immutable param and field directly:\n{from_param}"
    );

    // from_fix (id 7): fix 束縛・フィールド・.メソッド・演算子メソッドは全て構造体の一時コピーなし。
    let from_fix = body_of(7);
    assert!(
        !from_fix.contains("suz_struct_1 suz_cmp_")
            && from_fix.contains("suz_fn_3(&(suz_local_0), &(suz_local_1))")
            && from_fix.contains("suz_fn_0(&(suz_local_0))")
            && from_fix.contains("suz_fn_1(&((suz_local_0).suz_field_0))")
            && from_fix.contains("suz_fn_2(&(suz_local_0))"),
        "from_fix should pass fix structs, fields, .method, and operator directly:\n{from_fix}"
    );

    // from_var (id 8): var 束縛とそのフィールドは一時変数へコピーしてアドレスを渡す。
    let from_var = body_of(8);
    assert!(
        from_var.contains("suz_struct_1 suz_cmp_")
            && from_var.contains("suz_struct_0 suz_cmp_")
            && from_var.contains("&suz_cmp_"),
        "from_var must copy var struct and field to temporaries:\n{from_var}"
    );

    // from_deref (id 9): ポインタ参照外しとそのフィールドは一時変数へコピーしてアドレスを渡す。
    let from_deref = body_of(9);
    assert!(
        from_deref.contains("suz_struct_1 suz_cmp_")
            && from_deref.contains("suz_struct_0 suz_cmp_")
            && from_deref.contains("&suz_cmp_"),
        "from_deref must copy dereferenced struct and field to temporaries:\n{from_deref}"
    );

    // from_rvalue (id 10): 戻り値の一時変数へ 1 回保存してそのままアドレスを渡す。
    let from_rvalue = body_of(10);
    assert!(
        from_rvalue.contains("suz_struct_1 suz_cmp_0;")
            && from_rvalue.contains("suz_cmp_0 = suz_fn_5(), suz_fn_0(&suz_cmp_0)"),
        "from_rvalue should store return value once and pass its address:\n{from_rvalue}"
    );
}
