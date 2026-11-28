# 関数と制御（関数・fnshape）

[目次と書式](../spec-cases.md)

### flow-001 関数の戻り値型を省略する
- 主根拠：`[fn.return-type]`
- 結果：コンパイルエラー `[fn.return-type]` 1:13

値を返さない関数も、戻り値型を-> voidと明記する必要がある。

```suz
fn action() {}

fn main() -> u8 { return 0 }
```

### flow-002 仮引数の入れ子のフィールドを変更する
- 主根拠：`[fn.parameter]`
- 補助根拠：`[assign.target]`
- 結果：コンパイルエラー `[fn.parameter]` 5:5

仮引数のフィールドは、入れ子であっても変更できない。

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn change(value: Box) -> void {
    value.point#x = 9
}

fn main() -> u8 { return 0 }
```

### flow-003 仮引数と同名のローカル変数
- 主根拠：`[fn.parameter-scope]`
- 結果：コンパイルエラー `[fn.parameter-scope]` 2:9

valueが仮引数名と重複する。

```suz
fn use(value: i32) -> void {
    fix value = 1
}

fn main() -> u8 { return 0 }
```

### flow-004 関数呼び出しの引数が不足する
- 主根拠：`[fn.call]`
- 結果：コンパイルエラー `[fn.call]` 6:12

sumには2つの実引数が必要。

```suz
fn sum(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    return sum(1)
}
```

### flow-005 関数の実引数と仮引数の型が異なる
- 主根拠：`[fn.call]`
- 補助根拠：`[type.numeric-conversion]`
- 結果：コンパイルエラー `[type.expected]` 7:21

valueの型はu16であり、u8の仮引数へ暗黙変換しない。

```suz
fn identity(input: u8) -> u8 {
    return input
}

fn main() -> u8 {
    fix value: u16 = 1
    return identity(value)
}
```

### flow-006 関数の相互再帰
- 主根拠：`[fn.recursion]`
- 補助根拠：`[decl.top-level]`、`[fn.call]`
- 結果：正常終了

is_evenとis_oddが互いを呼び出す。0で再帰を止める。

```suz
fn is_even(n: u8) -> bool {
    if n == 0 {
        return true
    }
    return is_odd(n - 1)
}

fn is_odd(n: u8) -> bool {
    if n == 0 {
        return false
    }
    return is_even(n - 1)
}

fn main() -> u8 {
    return ifexpr is_even(4) && is_odd(3) { 0 } else { 1 }
}
```

### flow-007 void関数はreturnと末尾到達の両方で戻れる
- 主根拠：`[fn.return]`
- 補助根拠：`[fn.fallthrough]`
- 結果：正常終了

stopがtrueならreturnで戻り、falseなら本体末尾に到達して戻る。

```suz
fn visit(stop: bool) -> void {
    if stop {
        return
    }
}

fn main() -> u8 {
    visit(true)
    visit(false)
    return 0
}
```

### flow-008 void関数のreturnに値を付ける
- 主根拠：`[fn.return]`
- 結果：コンパイルエラー `[fn.return]` 2:12

```suz
fn action() -> void {
    return 1
}

fn main() -> u8 { return 0 }
```

### flow-009 値を返す関数で式のないreturnを書く
- 主根拠：`[fn.return]`
- 結果：コンパイルエラー `[fn.return]` 3:15

ほかの経路にreturn式があっても、値のないreturnはu8関数では使えない。

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return
    }
    return 1
}

fn main() -> u8 { return 0 }
```

### flow-010 非void関数にreturn式がない
- 主根拠：`[fn.value-return-required]`
- 結果：コンパイルエラー `[fn.value-return-required]` 3:1

末尾へ到達しなくても、return式が少なくとも1つ必要。

```suz
fn wait_forever() -> i32 {
    loop {}
}
fn main() -> u8 { return 0 }
```

### flow-011 関数末尾へ到達する経路
- 主根拠：`[fn.fallthrough]`
- 補助根拠：`[fn.flow-analysis]`
- 結果：コンパイルエラー `[fn.fallthrough]` 5:1

elseがなく、末尾へ進みうる。

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return 1
    }
}
fn main() -> u8 { return 0 }
```

### flow-059 revの内部だけでreturnして関数末尾に到達する
- 主根拠：`[fn.fallthrough]`
- 結果：コンパイルエラー `[fn.fallthrough]` 3:1

```suz
fn main() -> u8 {
    rev i in 0..<1 { return 3 }
}
```

### flow-012 if trueでもelseがなければ末尾へ進みうる
- 主根拠：`[fn.fallthrough]`
- 結果：コンパイルエラー `[fn.fallthrough]` 3:1

条件の値ではなく構文で判定する。returnは存在するが、最後のelseがないため関数末尾への到達を防げない。

```suz
fn main() -> u8 {
    if true { return 1 }
}
```

### flow-013 安全な処理と分岐からの明示的return
- 主根拠：`[fn.flow-analysis]`
- 補助根拠：`[if.elif]`
- 結果：正常終了 2

```suz
fn choose(value: i32) -> u8 {
    if value < 0 {
        return 1
    } elif value == 0 {
        return 2
    } else {
        return 3
    }
}

fn main() -> u8 {
    return choose(0)
}
```

### flow-014 while trueの内部から値を返す
- 主根拠：`[fn.flow-analysis]`
- 補助根拠：`[fn.value-return-required]`
- 結果：正常終了 3

条件はtrueリテラルで、ループを抜けるbreakはない。return式もあるため、関数末尾に別のreturnは必要ない。

```suz
fn main() -> u8 {
    while true {
        return 3
    }
}
```

### flow-015 trueの定数を条件にしたwhileは末尾へ進みうると判定する
- 主根拠：`[fn.flow-analysis]`
- 補助根拠：`[fn.fallthrough]`
- 結果：コンパイルエラー `[fn.fallthrough]` 7:1

条件に書かれているのは定数名であり、trueリテラルではない。関数末尾へ進みうるという構文上の判定になる。

```suz
fix ALWAYS = true

fn main() -> u8 {
    while ALWAYS {
        return 3
    }
}
```

### flow-016 反復があるforでも関数末尾への経路を残す
- 主根拠：`[fn.flow-analysis]`
- 補助根拠：`[fn.fallthrough]`
- 結果：コンパイルエラー `[fn.fallthrough]` 5:1

範囲が0..<1であっても、forは後続へ進みうるものとして判定する。ループ内のreturnだけでは関数末尾への到達を防げない。

```suz
fn main() -> u8 {
    for i in 0..<1 {
        return 3
    }
}
```

### flow-017 内側のbreakは外側のloopを抜けない
- 主根拠：`[fn.flow-analysis]`
- 補助根拠：`[loop.break]`
- 結果：正常終了 4

breakが抜けるのは内側のloopだけ。外側のloopには、それを抜けるbreakがない。

```suz
fn main() -> u8 {
    loop {
        loop {
            break
        }
        return 4
    }
}
```

### flow-018 実行されない分岐のbreakも到達判定で数える
- 主根拠：`[fn.flow-analysis]`
- 補助根拠：`[fn.fallthrough]`
- 結果：コンパイルエラー `[fn.fallthrough]` 8:1

条件がfalseでも、外側のloopを抜けるbreakが構文上存在する。ループ後にreturnがないためエラーになる。

```suz
fn main() -> u8 {
    loop {
        if false {
            break
        }
        return 3
    }
}
```

### flow-103 名前から作った関数値を呼ぶ
- 主根拠：`[fn.value-from-name]`
- 結果：正常終了

```suz
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix other = fn twice
    if other(3) != 6 { return 1 }
    return 0
}
```

### flow-104 mainを関数値にしない
- 主根拠：`[fn.value-from-name]`
- 結果：コンパイルエラー `[fn.value-from-name]` 2:13

```suz
fn main() -> u8 {
    fix f = fn main
    return 0
}
```

### flow-105 fnshape名だけでは型を書けない
- 主根拠：`[fn.type-mark]`
- 結果：コンパイルエラー `[fn.type-mark]` 4:12

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: Transform = fn twice
    return 0
}
```

### flow-106 fnの後ろにはfnshapeを指定する
- 主根拠：`[fn.type-mark]`
- 結果：コンパイルエラー `[fn.type-mark]` 2:22

```suz
fn main() -> u8 {
    fix f: static fn i32 = fn main
    return 0
}
```

### flow-107 呼び出し専用の関数を変数へ保存しない
- 主根拠：`[fn.type-call]`
- 結果：コンパイルエラー `[fn.type-call]` 4:12

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: fn Transform = fn twice
    return 0
}
```

### flow-108 呼び出し専用の関数を戻り値型にしない
- 主根拠：`[fn.type-call]`
- 結果：コンパイルエラー `[fn.type-call]` 3:13

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn get() -> fn Transform { return fn twice }
fn main() -> u8 {

    return 0
}
```

### flow-109 呼び出し専用の仮引数を返さない
- 主根拠：`[fn.call-only]`
- 結果：コンパイルエラー `[fn.call-only]` 2:57

```suz
fnshape Transform(i32) -> i32
fn get(f: fn Transform) -> static fn Transform { return f }
fn main() -> u8 {

    return 0
}
```

### flow-110 呼び出し専用の仮引数を括弧で転送しない
- 主根拠：`[fn.call-only]`
- 結果：コンパイルエラー `[fn.call-only]` 3:47

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn forward(f: fn Calc) -> i32 { return apply((f), 1) }
fn main() -> u8 {

    return 0
}
```

### flow-111 保存可能な関数値を呼び出し専用の仮引数へ渡す
- 主根拠：`[fn.type-convert]`
- 結果：正常終了

```suz
fnshape Calc(i32) -> i32
fn apply(f: fn Calc, value: i32) -> i32 { return f(value) }
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: static fn Calc = fn twice
    if apply(f, 3) != 6 { return 1 }
    return 0
}
```

### flow-112 fnを呼び出し式に付けない
- 主根拠：`[fn.value-mark]`
- 結果：コンパイルエラー `[fn.value-mark]` 5:20

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn pick(i: i32) -> static fn Transform { return fn twice }
fn main() -> u8 {
    fix f = fn pick(0)
    return 0
}
```

### flow-113 返された関数値をその場で呼ぶ
- 主根拠：`[fn.value-call]`
- 結果：正常終了

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { return n * 2 }
fn pick(i: i32) -> static fn Transform { return fn twice }
fn main() -> u8 {
    if pick(0)(3) != 6 { return 1 }
    return 0
}
```

### flow-114 引数で変数を更新しても呼ぶ関数は変わらない
- 主根拠：`[fn.value-evaluation]`
- 結果：正常終了

```suz
fnshape Transform(i32) -> i32
fn twice(n: i32) -> i32 { log info "twice"
    return n * 2 }
fn triple(n: i32) -> i32 { log info "triple"
    return n * 3 }
fn main() -> u8 {
    var f: static fn Transform = fn twice
    fix result = f(expr { log info "argument"
        f = fn triple
        3 })
    if result != 6 || f(3) != 9 { return 1 }
    return 0
}
```

**標準エラー：**

```text
info flow-114.suz:8: argument
info flow-114.suz:2: twice
info flow-114.suz:4: triple
```

### flow-115 関数値を比較しない
- 主根拠：`[fn.value-compare]`
- 結果：コンパイルエラー `[fn.value-compare]` 4:19

```suz
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f = fn twice
    fix equal = f == f
    return 0
}
```

### flow-116 関数値をテンプレートに埋め込まない
- 主根拠：`[fn.value-compare]`
- 結果：コンパイルエラー `[symbol.interpolation]` 4:31

```suz
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f = fn twice
    fix rendered: symbol = "${f}"
    return 0
}
```

### flow-117 要求型から匿名汎用の関数値を具体化する
- 主根拠：`[fn.value-generic]`
- 結果：正常終了

```suz
fnshape Transform(i32) -> i32
fn identity(n: int) -> int { return n }
fn main() -> u8 {
    fix action: static fn Transform = fn identity
    if action(3) != 3 { return 1 }
    return 0
}
```

### flow-118 匿名汎用の関数値には要求型が必要
- 主根拠：`[fn.value-generic]`
- 結果：コンパイルエラー `[fn.value-generic]` 3:13

```suz
fn identity(n: int) -> int { return n }
fn main() -> u8 {
    fix f = fn identity
    return 0
}
```

### flow-119 fnshapeの引数にvoidを使わない
- 主根拠：`[fnshape.decl]`
- 結果：コンパイルエラー `[type.void]` 1:9

```suz
fnshape Wrong(void) -> i32
fn main() -> u8 {

    return 0
}
```

### flow-120 fnshapeの引数にintを使わない
- 主根拠：`[fnshape.decl]`
- 結果：コンパイルエラー `[num.places]` 1:9

```suz
fnshape Wrong(int) -> i32
fn main() -> u8 {

    return 0
}
```

### flow-121 名前が違っても同じ形の関数型を渡せる
- 主根拠：`[fnshape.same]`
- 結果：正常終了

```suz
fnshape Transform(i32) -> i32
fnshape Other(i32) -> i32
fn apply(f: fn Other, n: i32) -> i32 { return f(n) }
fn twice(n: i32) -> i32 { return n * 2 }
fn main() -> u8 {
    fix f: static fn Transform = fn twice
    if apply(f, 3) != 6 { return 1 }
    return 0
}
```

### flow-122 fnshapeの再帰を拒否する
- 主根拠：`[fnshape.recursion]`
- 結果：コンパイルエラー `[fnshape.recursion]` 1:9

```suz
fnshape Recursive(static fn Recursive) -> void
fn main() -> u8 {

    return 0
}
```
