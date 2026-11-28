# 関数と制御（制御構文・反復・分岐）

[目次と書式](../spec-cases.md)

### flow-019 matchの対象を1回評価し、一致した腕だけ実行する
- 主根拠：`[match.target]`
- 結果：正常終了

subjectを1回呼び、値2に対応する腕だけを実行する。

```suz
fn subject() -> i32 {
    log info "subject"
    return 2
}

fn main() -> u8 {
    match subject() {
        1 => { log info "first" }
        2 => { log info "second" }
        _ => { log info "other" }
    }
    return 0
}
```

**標準エラー：**

```text
info flow-019.suz:2: subject
info flow-019.suz:9: second
```

### flow-064 charのmatchと比較の連鎖を使う
- 主根拠：`[match.target]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix ch = \5
    fix kind = matchexpr ch {
        \space => 1
        \5 => 2
        _ => 0
    }
    fix digit = \0 <= ch <= \9
    log info "${kind} ${digit}"
    return 0
}
```

**標準エラー：**

```text
info flow-064.suz:9: 2 true
```

### flow-020 整数matchの定数パターンとワイルドカード
- 主根拠：`[match.pattern]`
- 補助根拠：`[match.exhaustive]`
- 結果：正常終了 3

```suz
fix SELECTED: i32 = 2

fn main() -> u8 {
    fix input: i32 = 2
    match input {
        SELECTED => { return 3 }
        _ => { return 4 }
    }
}
```

### flow-021 boolでワイルドカードを使う
- 主根拠：`[match.pattern]`
- 結果：コンパイルエラー `[match.pattern]` 5:9

全値を列挙していてもboolの_は禁止。

```suz
fn main() -> u8 {
    match true {
        true => {}
        false => {}
        _ => {}
    }
    return 0
}
```

### flow-022 enumでワイルドカードを使う
- 主根拠：`[match.pattern]`
- 結果：コンパイルエラー `[match.pattern]` 7:9

enumの_は禁止。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    match Wave.Sine {
        Wave.Sine => {}
        Wave.Saw => {}
        _ => {}
    }
    return 0
}
```

### flow-023 評価後のパターンの重複
- 主根拠：`[match.pattern]`
- 結果：コンパイルエラー `[match.pattern]` 4:9

1 + 1と2は同じ値。

```suz
fn main() -> u8 {
    match 2 {
        1 + 1 => {}
        2 => {}
        _ => {}
    }
    return 0
}
```

### flow-024 整数matchの途中にワイルドカードを置く
- 主根拠：`[match.pattern]`
- 補助根拠：`[match.exhaustive]`
- 結果：コンパイルエラー `[match.pattern]` 3:9

整数の_は最後の腕に置く必要がある。

```suz
fn main() -> u8 {
    match 1 {
        _ => {}
        1 => {}
    }
    return 0
}
```

### flow-025 fixの変数をmatchのパターンにする
- 主根拠：`[match.pattern]`
- 補助根拠：`[const.expression]`
- 結果：コンパイルエラー `[const.expression]` 5:9

fixの変数は、数値リテラルで初期化しても定数式の構成要素にはならない。

```suz
fn main() -> u8 {
    fix selected: i32 = 1
    fix input: i32 = 1
    match input {
        selected => {}
        _ => {}
    }
    return 0
}
```

### flow-026 boolの全値を網羅するmatchexpr
- 主根拠：`[match.exhaustive]`
- 補助根拠：`[match.value]`
- 結果：正常終了 3

```suz
fn main() -> u8 {
    fix flag = true
    return matchexpr flag {
        true => 3
        false => 4
    }
}
```

### flow-027 enumの全列挙子を網羅するmatchexpr
- 主根拠：`[match.exhaustive]`
- 補助根拠：`[match.value]`
- 結果：正常終了 2

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix wave = Wave.Saw
    return matchexpr wave {
        Wave.Sine => 1
        Wave.Saw => 2
    }
}
```

### flow-028 boolの網羅漏れ
- 主根拠：`[match.exhaustive]`
- 結果：コンパイルエラー `[match.exhaustive]` 2:5

falseの腕が必要。

```suz
fn main() -> u8 {
    match true {
        true => {}
    }
    return 0
}
```

### flow-029 整数matchのワイルドカード漏れ
- 主根拠：`[match.exhaustive]`
- 結果：コンパイルエラー `[match.exhaustive]` 2:5

整数matchには最後の_が必要。

```suz
fn main() -> u8 {
    match 0 {
        0 => {}
    }
    return 0
}
```

### flow-030 enumの列挙子を網羅していないmatch
- 主根拠：`[match.exhaustive]`
- 結果：コンパイルエラー `[match.exhaustive]` 4:5

対象がWave.Sineと書かれていても、型の全列挙子を網羅する必要がある。Wave.Sawの腕がない。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    match Wave.Sine {
        Wave.Sine => {}
    }
    return 0
}
```

### flow-031 最後のmatchの腕と閉じ括弧を同じ行に書く
- 主根拠：`[match.separator]`
- 補助根拠：`[src.statement-end]`、`[match.arm]`
- 結果：正常終了 2

腕同士は改行で区切るが、最後の腕のブロックとmatchを閉じる波括弧の間は改行を省略できる。

```suz
fn main() -> u8 {
    match false {
        true => { return 1 }
        false => { return 2 } }
}
```

### flow-032 matchの腕を同じ行に並べる
- 主根拠：`[match.separator]`
- 結果：コンパイルエラー `[match.separator]` 3:20

ブロックを閉じても次の腕との間には改行が必要。最後の腕とmatchを閉じる波括弧の間だけ省略できる。

```suz
fn main() -> u8 {
    match true {
        true => {} false => {}
    }
    return 0
}
```

### flow-033 文のmatchとifは分岐の結果を捨てる
- 主根拠：`[branch.statement]`
- 補助根拠：`[match.exhaustive]`
- 結果：正常終了

```suz
fn main() -> u8 {
    if true {
        1
    } else {
        false
    }
    match false {
        true => 1
        false => false
    }
    return 0
}
```

### flow-034 expr末尾に値がない
- 主根拠：`[block.value]`
- 結果：コンパイルエラー `[block.value]` 4:5

末尾はログ文であり、値を持つ式ではない。

```suz
fn main() -> u8 {
    fix value = expr {
        log info "message"
    }
    return 0
}
```

### flow-035 exprの末尾をvoidの式にする
- 主根拠：`[block.value]`
- 補助根拠：`[branch.expression]`
- 結果：コンパイルエラー `[block.value]` 5:9

結果を変数に保存せず捨てる場合でも、expr自体はvoid以外の値を作る必要がある。

```suz
fn action() -> void {}

fn main() -> u8 {
    expr {
        action()
    }
    return 0
}
```

### flow-036 elseを次の行に置く
- 主根拠：`[expr.else-line]`
- 結果：コンパイルエラー `[expr.else-line]` 4:5

elseは直前の}と同じ行に置く。

```suz
fn main() -> u8 {
    if true {
    }
    else {
    }
    return 0
}
```

### flow-037 条件を順に評価し、成立後のelifを評価しない
- 主根拠：`[if.condition]`
- 補助根拠：`[if.elif]`
- 結果：正常終了 2

1番目の条件はfalse、2番目はtrue。3番目の条件を評価せず、2番目の分岐から戻る。

```suz
fn check(id: u8, result: bool) -> bool {
    log info "${id}"
    return result
}

fn main() -> u8 {
    if check(1, false) {
        return 1
    } elif check(2, true) {
        return 2
    } elif check(3, true) {
        return 3
    } else {
        return 4
    }
}
```

**標準エラー：**

```text
info flow-037.suz:2: 1
info flow-037.suz:2: 2
```

### flow-038 ifの条件に整数を使う
- 主根拠：`[if.condition]`
- 補助根拠：`[type.bool]`
- 結果：コンパイルエラー `[if.condition]` 2:8

整数の1はboolとして扱わない。

```suz
fn main() -> u8 {
    if 1 {
        return 1
    }
    return 0
}
```

### flow-039 評価されないelifの条件にもboolを要求する
- 主根拠：`[if.condition]`
- 結果：コンパイルエラー `[if.condition]` 4:12

先頭の条件がtrueでも、elifの条件の型は検査する。

```suz
fn main() -> u8 {
    if true {
        return 0
    } elif 1 {
        return 1
    } else {
        return 2
    }
}
```

### flow-040 whileのcontinueで更新後の条件を再評価する
- 主根拠：`[while.loop]`
- 結果：正常終了 2

countを0から1、1から2へ更新し、各反復のcontinueで9の代入を飛ばす。条件を再評価して2回で終了し、2を返す。continueをbreakとして扱うと1を返すため、区別できる。

```suz
fn main() -> u8 {
    var count: u8 = 0
    while count < 2 {
        count += 1
        continue
        count = 9
    }
    return count
}
```

### flow-041 whileの条件に整数を使う
- 主根拠：`[while.loop]`
- 補助根拠：`[type.bool]`
- 結果：コンパイルエラー `[while.loop]` 2:11

整数の0はboolとして扱わない。

```suz
fn main() -> u8 {
    while 0 {}
    return 0
}
```

### flow-042 loop内のbreakとcontinue
- 主根拠：`[loop.unconditional]`
- 補助根拠：`[loop.break]`、`[loop.continue]`
- 結果：正常終了 3

```suz
fn main() -> u8 {
    var count: u8 = 0
    loop {
        count += 1
        if count == 2 {
            continue
        }
        if count == 3 {
            break
        }
    }
    return count
}
```

### flow-043 forとrev専用の範囲記法を変数に保存する
- 主根拠：`[for.range]`
- 結果：コンパイルエラー `[for.range]` 2:18

..<の範囲記法はfor専用であり、値を作る式として使えない。

```suz
fn main() -> u8 {
    fix items = 0..<3
    return 0
}
```

### flow-060 両端を左から評価し、範囲を逆順に繰り返す
- 主根拠：`[rev.range]`
- 結果：正常終了

```suz
fn low() -> u8 { log info "low"
return 1 }
fn high() -> u8 { log info "high"
return 3 }
fn main() -> u8 {
    rev i in low()..<high() { log info "${i}" }
    rev i in 1..=3 { log info "${i}" }
    return 0
}
```

**標準エラー：**

```text
info flow-060.suz:1: low
info flow-060.suz:3: high
info flow-060.suz:6: 2
info flow-060.suz:6: 1
info flow-060.suz:7: 3
info flow-060.suz:7: 2
info flow-060.suz:7: 1
```

### flow-061 空の範囲では減算せず、両端を含む同値の範囲は1回繰り返す
- 主根拠：`[rev.bounds]`
- 結果：正常終了 1

```suz
fn main() -> u8 {
    fix low: i8 = -128
    rev i in low..<low { return 1 }
    rev i in 1..<0 { return 2 }
    rev i in 1..=0 { return 3 }
    var count: u8 = 0
    rev i in low..=low { count += 1 }
    return count
}
```

### flow-062 型の全範囲を逆順に進み、最小値でcontinueする
- 主根拠：`[rev.bounds]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix low: i8 = -128
    fix high: i8 = 127
    var count: u16 = 0
    var last: i8 = 0
    rev i in low..=high {
        count += 1
        last = i
        continue
    }
    log info "${count} ${last}"
    return 0
}
```

**標準エラー：**

```text
info flow-062.suz:11: 256 -128
```

### flow-065 文字範囲でサロゲートを飛ばし、両端でcontinueする
- 主根拠：`[char.range]`
- 結果：正常終了

```suz
fn main() -> u8 {
    for ch in \u{D7FF}..=\u{E000} {
        fix code = ch as u32
        log info "${code}"
    }
    rev ch in \u{D7FF}..=\u{E000} {
        fix code = ch as u32
        log info "${code}"
    }
    for ch in \u{10FFFF}..=\u{10FFFF} { continue }
    rev ch in \nul..=\nul { continue }
    rev ch in \nul..<\nul { return 1 }
    return 0
}
```

**標準エラー：**

```text
info flow-065.suz:4: 55295
info flow-065.suz:4: 57344
info flow-065.suz:8: 57344
info flow-065.suz:8: 55295
```

### flow-044 範囲演算子の前後の空白
- 主根拠：`[for.range-spacing]`
- 結果：コンパイルエラー `[for.range-spacing]` 2:16

..<は両端の式の境界に隣接する必要がある。

```suz
fn main() -> u8 {
    for i in 0 ..< 3 {
        log info "${i}"
    }
    return 0
}
```

### flow-045 範囲演算子の直後にコメントと改行を挟む
- 主根拠：`[for.range-spacing]`
- 補助根拠：`[src.line-continuation]`
- 結果：コンパイルエラー `[for.range-spacing]` 2:15

範囲演算子には両端への隣接が必要。ほかの二項演算子のように直後の改行で継続できない。

```suz
fn main() -> u8 {
    for i in 0..<// 終了値は次の行
        3 {}
    return 0
}
```

### flow-046 forの両端に異なる整数型を使う
- 主根拠：`[for.bounds]`
- 補助根拠：`[type.numeric-conversion]`
- 結果：コンパイルエラー `[type.expected]` 4:22

開始値のu8と終了値のu16は一致しない。

```suz
fn main() -> u8 {
    fix start: u8 = 0
    fix end: u16 = 3
    for i in start..<end {}
    return 0
}
```

### flow-047 変数を使う範囲と最大値を含む範囲
- 主根拠：`[for.bounds]`
- 補助根拠：`[type.context]`、`[for.variable]`
- 結果：正常終了 4

iとlastはu8。255の反復後に増分によるオーバーフローを起こさない。

```suz
fn main() -> u8 {
    fix end: u8 = 3
    fix top: u8 = 255
    var total: u8 = 0
    for i in 0..<end {
        total += i
    }
    for last in top..=top {
        total += 1
    }
    return total
}
```

### flow-048 空の範囲
- 主根拠：`[for.bounds]`
- 結果：正常終了

```suz
fn main() -> u8 {
    for i in 3..<3 {
        return 1
    }
    for j in 4..=3 {
        return 2
    }
    return 0
}
```

### flow-049 範囲の両端を左から1回だけ評価
- 主根拠：`[for.bounds]`
- 補助根拠：`[expr.eval-order]`
- 結果：正常終了

```suz
fn lower() -> u8 {
    log info "lower"
    return 0
}

fn upper() -> u8 {
    log info "upper"
    return 2
}

fn main() -> u8 {
    for i in lower()..<upper() {
        log info "i=${i}"
    }
    return 0
}
```

**標準エラー：**

```text
info flow-049.suz:2: lower
info flow-049.suz:7: upper
info flow-049.suz:13: i=0
info flow-049.suz:13: i=1
```

### flow-050 変更可能な終了値をfixにコピーして範囲に使う
- 主根拠：`[for.bound-reference]`
- 補助根拠：`[for.bounds]`
- 結果：正常終了 3

endの現在値をfixのlimitにコピーする。範囲はlimitを使うため、本体でendを変更しても3回反復する。

```suz
fn main() -> u8 {
    var end: u8 = 3
    fix limit = end
    var count: u8 = 0
    for i in 0..<limit {
        end = 0
        count += 1
    }
    return count
}
```

### flow-051 forの終了値にvarを直接使う
- 主根拠：`[for.bound-reference]`
- 結果：コンパイルエラー `[for.bound-reference]` 3:18

endを範囲式から直接参照している。現在値を使う場合は先にfixへコピーする。

```suz
fn main() -> u8 {
    var end: u8 = 3
    for i in 0..<end {
        end = 0
    }
    return 0
}
```

### flow-052 範囲の開始値の関数引数にvarを使う
- 主根拠：`[for.bound-reference]`
- 結果：コンパイルエラー `[for.bound-reference]` 7:23

開始値の式にある実引数も参照の検査対象になる。

```suz
fn identity(value: u8) -> u8 {
    return value
}

fn main() -> u8 {
    var start: u8 = 0
    for i in identity(start)..<3 {
        start = 1
    }
    return 0
}
```

### flow-053 範囲の終了値にvarのフィールドを使う
- 主根拠：`[for.bound-reference]`
- 結果：コンパイルエラー `[for.bound-reference]` 5:18

limitsのフィールドへの参照もvarからの参照として扱う。

```suz
struct Limits { end: u8 }

fn main() -> u8 {
    var limits = Limits { end: 3 }
    for i in 0..<limits.end {
        limits#end = 0
    }
    return 0
}
```

### flow-067 textの反復対象でvarを参照する
- 主根拠：`[for.bound-reference]`
- 結果：コンパイルエラー `[for.bound-reference]` 3:15

```suz
fn main() -> u8 {
    var message = "abc"
    for ch in message {}
    return 0
}
```

### flow-054 ループ変数への代入
- 主根拠：`[for.variable]`
- 結果：コンパイルエラー `[for.variable]` 3:9

ループ変数は変更不可。

```suz
fn main() -> u8 {
    for i in 0..<3 {
        i = 2
    }
    return 0
}
```

### flow-055 forの外からループ変数を参照する
- 主根拠：`[for.variable]`
- 結果：コンパイルエラー `[decl.local-scope]` 4:12

iを参照できるのはループ本体の中だけ。

```suz
fn main() -> u8 {
    fix end: u8 = 3
    for i in 0..<end {}
    return i
}
```

### flow-153 2変数反復で同じ変数名を重複させると拒否する
- 主根拠：`[for.variable]`
- 補助根拠：`[decl.duplicate]`
- 結果：コンパイルエラー `[decl.duplicate]` 3:12

```suz
fn main() -> u8 {
    fix a = new .[1, 2]
    for x, x in a {}
    return 0
}
```

### flow-154 2変数反復で外側の名前をシャドーイングすると拒否する
- 主根拠：`[for.variable]`
- 補助根拠：`[decl.shadowing]`
- 結果：コンパイルエラー `[decl.shadowing]` 4:9

```suz
fn main() -> u8 {
    fix a = new .[1, 2]
    fix i = 0
    for i, x in a {}
    return 0
}
```

### flow-066 textをスカラー値単位で前後に反復する
- 主根拠：`[char.for]`
- 結果：正常終了

```suz
fn read(value: text) -> text {
    log info "read"
    return value
}
fn main() -> u8 {
    var message = "aあ😀"
    fix target = message
    for ch in read(target) {
        log info "${ch}"
        message = "changed"
        continue
    }
    rev ch in read(target) {
        if ch == \a { break }
        log info "${ch}"
    }
    for ch in "" { return 1 }
    rev ch in "" { return 2 }
    log info "${message}"
    return 0
}
```

**標準エラー：**

```text
info flow-066.suz:2: read
info flow-066.suz:9: a
info flow-066.suz:9: あ
info flow-066.suz:9: 😀
info flow-066.suz:2: read
info flow-066.suz:15: 😀
info flow-066.suz:15: あ
info flow-066.suz:19: changed
```

### flow-155 範囲式に対する2変数反復は拒否する
- 主根拠：`[for.pair-target]`
- 結果：コンパイルエラー `[for.pair-target]` 2:17

```suz
fn main() -> u8 {
    for i, x in 0..<3 {}
    return 0
}
```

### flow-156 文字列に対する2変数反復は拒否する
- 主根拠：`[for.pair-target]`
- 結果：コンパイルエラー `[for.pair-target]` 2:17

```suz
fn main() -> u8 {
    for i, c in "abc" {}
    return 0
}
```

### flow-157 型名に対する2変数反復は拒否する
- 主根拠：`[for.pair-target]`
- 結果：コンパイルエラー `[for.pair-target]` 3:17

```suz
enum Wave: u8 { Sine }
fn main() -> u8 {
    for w, x in Wave {}
    return 0
}
```

### flow-158 列挙型名と値域制限型名をfor・revで反復する
- 主根拠：`[for.type-name]`
- 補助根拠：`[for.variable]`、`[array.enum-index]`
- 結果：正常終了

```suz
enum Wave: u8 { Sine, Saw, Square }
range Step: u8 = 0..<4
range One: i32 = 3..=3
range SignedByte: i8 = -128..=127
range FullU64: u64 = 0..=18446744073709551615
range HighU64: u64 = 18446744073709551614..=18446744073709551615

fn main() -> u8 {
    fix a: #[i32; Wave] = new #[0; Wave]
    for w in Wave {
        a#[w] = (w as i32) + 1 or return 1
    }
    if a.[Wave.Sine] != 1 || a.[Wave.Saw] != 2 || a.[Wave.Square] != 3 { return 2 }

    var rev_wave: i32 = 0
    rev w in Wave {
        fix v = a.[w]
        rev_wave = rev_wave * 10 + v or return 3
    }
    if rev_wave != 321 { return 4 }

    var step_fwd: i32 = 0
    for s in Step {
        step_fwd = step_fwd * 10 + (s as i32) or return 5
    }
    var step_rev: i32 = 0
    rev s in Step {
        step_rev = step_rev * 10 + (s as i32) or return 6
    }
    if step_fwd != 123 || step_rev != 3210 { return 7 }

    var one_sum: i32 = 0
    for o in One { one_sum = one_sum + (o as i32) or return 8 }
    rev o in One { one_sum = one_sum + (o as i32) or return 9 }
    if one_sum != 6 { return 10 }

    var sb: i32 = 0
    for b in SignedByte {
        sb = sb + 1 or return 11
        if b == 127 { continue }
    }
    rev b in SignedByte {
        sb = sb + 1 or return 12
        if b == -128 { continue }
    }
    if sb != 512 { return 13 }

    var full_ok: i32 = 0
    for u in FullU64 {
        if u == 0 {
            full_ok = full_ok + 1 or return 14
            break
        }
    }
    rev u in FullU64 {
        if u == 18446744073709551615 {
            full_ok = full_ok + 1 or return 15
            break
        }
    }
    for u in HighU64 {
        full_ok = full_ok + 1 or return 16
        if u == 18446744073709551615 { continue }
    }
    if full_ok != 4 { return 17 }
    return 0
}
```

### flow-159 構造体の型名の反復は拒否する
- 主根拠：`[for.type-name]`
- 結果：コンパイルエラー `[for.type-name]` 3:14

```suz
struct Vec2 { x: f32 }
fn main() -> u8 {
    for p in Vec2 {}
    return 0
}
```

### flow-160 宣言していない型名の反復は拒否する
- 主根拠：`[for.type-name]`
- 結果：コンパイルエラー `[for.type-name]` 2:14

```suz
fn main() -> u8 {
    for w in UnknownEnum {}
    return 0
}
```

### flow-056 forのcontinueは次の値へ進む
- 主根拠：`[loop.continue]`
- 補助根拠：`[for.bounds]`
- 結果：正常終了 4

iが2の反復では加算を飛ばす。1と3を加えた結果は4になる。

```suz
fn main() -> u8 {
    fix end: u8 = 3
    var total: u8 = 0
    for i in 1..=end {
        if i == 2 {
            continue
        }
        total += i
    }
    return total
}
```

### flow-063 入れ子のrevからbreakし、外側のforを続ける
- 主根拠：`[loop.break]`
- 結果：正常終了 2

```suz
fn main() -> u8 {
    var count: u8 = 0
    for outer in 0..<2 {
        rev inner in 0..<4 {
            if inner == 1 { break }
            if inner == 2 { continue }
            count += 1
        }
    }
    return count
}
```

### flow-057 ループの外にbreakを書く
- 主根拠：`[loop.jump-context]`
- 結果：コンパイルエラー `[loop.jump-context]` 2:5

```suz
fn main() -> u8 {
    break
    return 0
}
```

### flow-058 ループの外にcontinueを書く
- 主根拠：`[loop.jump-context]`
- 結果：コンパイルエラー `[loop.jump-context]` 2:5

```suz
fn main() -> u8 {
    continue
    return 0
}
```
