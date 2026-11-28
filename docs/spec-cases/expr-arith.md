# 仕様例：式（算術・変換・キャスト）

### expr-001 ラップする算術・シフトと切り詰め
- 主根拠：`[expr.wrap]`
- 補助根拠：`[expr.as-trunc]`、`[expr.shift]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix wrapped = top +% 1
    fix down: u8 = 0 -% 1
    fix product: u8 = 128 *% 2
    fix neg_min: i8 = -%(-128 as i8)
    fix shl_wrap: u8 = (129 as u8) <<% 9
    fix shr_wrap: i8 = (-7 as i8) >>% 9
    fix lshr_wrap: i8 = (-2 as i8) >>>% 9
    fix truncated = 257 as% u8
    return ifexpr wrapped == 0 && down == 255 && product == 0 && neg_min == -128 &&
        shl_wrap == 2 && shr_wrap == -4 && lshr_wrap == 127 && truncated == 1 { 0 } else { 1 }
}
```

### expr-002 符号付き整数のラップとas%の符号の扱い
- 主根拠：`[expr.wrap]`
- 補助根拠：`[expr.as-trunc]`
- 結果：正常終了

i8の127 +% 1は-128。255 as% i8は-1。-1 as% u16は65535になる。

```suz
fn main() -> u8 {
    fix maximum: i8 = 127
    fix wrapped = maximum +% 1
    fix signed_value = 255 as% i8
    fix negative: i8 = -1
    fix widened = negative as% u16
    return ifexpr wrapped == -128 && signed_value == -1 &&
        widened == 65_535 { 0 } else { 1 }
}
```

### expr-079 飽和算術と単項飽和符号反転
- 主根拠：`[expr.sat]`
- 補助根拠：`[assign.compound]`
- 結果：正常終了

```suz
fn main() -> u8 {
    var vol: u8 = 250
    vol +|= 20
    vol -|= 250
    vol -|= 20
    fix s: i8 = -128
    fix inv: i8 = -| s
    fix m: i8 = (100 as i8) *| (2 as i8)
    if vol != 0 || inv != 127 || m != 127 { return 1 }
    return 0
}
```

### expr-080 飽和左シフト
- 主根拠：`[expr.sat-shift]`
- 補助根拠：`[expr.arithmetic-handler]`、`[assign.compound]`
- 結果：正常終了

```suz
fn main() -> u8 {
    var v: i8 = 64
    v <<|= 1 as u8
    fix huge: u8 = (1 as u8) <<| (100 as u8)
    fix neg: i8 = (1 as i8) <<| -1 or 77
    if v != 127 || huge != 255 || neg != 77 { return 1 }
    return 0
}
```

### expr-088 失敗しえない飽和左シフトに付けた or は拒否する
- 主根拠：`[expr.sat-shift]`
- 補助根拠：`[expr.arithmetic-handler]`
- 結果：コンパイルエラー `[expr.arithmetic-handler]` 2:20

```suz
fn f(a: i8, n: u8) -> i8 {
    return a <<| n or 9
}
fn main() -> u8 {
    return 0
}
```

### expr-081 整数・浮動小数点からの飽和変換
- 主根拠：`[expr.as-sat]`
- 補助根拠：`[expr.cast-binding]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix pcm: i16 = (50000 as i32) as| i16
    fix lo: u8 = (-5 as i32) as| u8
    fix f_nan: f64 = nan
    fix f_inf: f64 = inf
    fix fn_val: i16 = f_nan as| i16 or -9
    fix fi_val: i16 = f_inf as| i16 or -9
    if pcm != 32767 || lo != 0 || fn_val != -9 || fi_val != 32767 { return 1 }
    return 0
}
```

### expr-082 or のない浮動小数点飽和変換の警告と allow による抑制
- 主根拠：`[expr.as-sat]`
- 補助根拠：`[diagnostic.float-sat-nan]`、`[allow.clause]`
- 結果：正常終了 / 警告：`[diagnostic.float-sat-nan]` 2:14

```suz
fn to_pcm(x: f32) -> i16 {
    return x as| i16
}

fn to_pcm_allowed(x: f32) -> i16 {
    return x as| i16 allow [diagnostic.float-sat-nan: audio output treats NaN as silence]
}

fn main() -> u8 {
    fix a = to_pcm(nan)
    fix b = to_pcm_allowed(nan)
    return ifexpr a == 0 && b == 0 { 0 } else { 1 }
}
```

### expr-085 len_t を変換先とする飽和変換は拒否する
- 主根拠：`[expr.as-sat]`
- 補助根拠：`[len.type-operations]`
- 結果：コンパイルエラー `[expr.as-sat]` 2:31

```suz
fn main() -> u8 {
    fix a: len_t = (1 as i32) as| len_t
    return 0
}
```

### expr-086 整数からの飽和変換に付けた or は拒否する
- 主根拠：`[expr.as-sat]`
- 補助根拠：`[expr.cast-sat-handler-syntax]`
- 結果：コンパイルエラー `[expr.cast-sat-handler-syntax]` 2:28

```suz
fn main() -> u8 {
    fix a: u8 = (1 as i32) as| u8 or 0
    return a
}
```

### expr-023 比較相手から整数リテラルの型を決める
- 主根拠：`[expr.literal-type]`
- 結果：正常終了 7

左右どちらにリテラルを書いても、比較相手のtopからu64を要求する。比較結果に要求するboolは比較対象へ伝えない。

```suz
fn main() -> u8 {
    fix top: u64 = 18446744073709551615
    fix same: bool = top == 18446744073709551615
    if not same { return 1 }
    if not (18446744073709551615 == (top)) { return 2 }
    return 7
}
```

### expr-026 同じ優先順位の二項演算は左から結合する
- 主根拠：`[expr.associativity]`
- 結果：正常終了

10 - 3 - 2は5、20 / 2 / 2は5になる。右側を括弧でまとめると、それぞれ9と20になる。

```suz
fn main() -> u8 {
    fix subtracted = 10 - 3 - 2
    fix divided = 20 / 2 / 2
    fix grouped_subtraction = 10 - (3 - 2)
    fix grouped_division = 20 / (2 / 2)
    return ifexpr subtracted == 5 && divided == 5 &&
        grouped_subtraction == 9 && grouped_division == 20 { 0 } else { 1 }
}
```

### expr-029 負数の除算・剰余・非負剰余・右シフト
- 主根拠：`[expr.int-division]`
- 補助根拠：`[expr.shift]`
- 結果：正常終了

-7 / 3は-2、-7 % 3は-1、-7 %% 3は2。浮動小数点の-7.5 % 2.0は-1.5、-0.25 %% 1.0は0.75。符号付きの-7 >> 1は符号ビットで埋めるため-4、-7 >>> 1は0で埋めるため124になる。符号なしの249 >> 1は124になる。

```suz
fn main() -> u8 {
    fix negative: i8 = -7
    fix positive: u8 = 249
    fix quotient = negative / 3
    fix remainder = negative % 3
    fix euclid_int = negative %% 3
    fix float_rem: f64 = -7.5 % 2.0
    fix float_euclid: f64 = -0.25 %% 1.0
    fix signed_shift = negative >> 1
    fix logical_shift = negative >>> 1
    fix unsigned_shift = positive >> 1
    return ifexpr quotient == -2 && remainder == -1 && euclid_int == 2 &&
        float_rem == -1.5 && float_euclid == 0.75 &&
        signed_shift == -4 && logical_shift == 124 && unsigned_shift == 124 { 0 } else { 1 }
}
```

### expr-042 f32からf64への変換で有限値と特殊値を保つ
- 主根拠：`[expr.as]`
- 補助根拠：`[expr.nan-comparison]`、`[expr.cast-risk]`
- 結果：正常終了

f32からf64への変換は常に成功し、orを必要としない。有限値は正確に表し、無限大とNaNも保つ。

```suz
fn main() -> u8 {
    fix half: f32 = 0.5
    fix infinity: f32 = inf
    fix invalid: f32 = nan
    fix wide = half as f64
    fix wide_infinity = infinity as f64
    fix wide_invalid = invalid as f64
    return ifexpr wide == 0.5 && wide_infinity == inf &&
        wide_invalid != wide_invalid { 0 } else { 1 }
}
```

### expr-043 boolから整数へのasは使えない
- 主根拠：`[expr.as]`
- 結果：コンパイルエラー `[expr.as]` 2:22

boolから整数への変換はasが許可する型の組み合わせに含まれない。

```suz
fn main() -> u8 {
    fix value = true as u8
    return value
}
```

### expr-044 異なるenumへ直接asで変換する
- 主根拠：`[expr.as]`
- 結果：コンパイルエラー `[expr.as]` 6:24

enum同士の直接変換は許可されない。orを付けても許可される型の組み合わせは変わらない。

```suz
enum First: u8 { A, B }
enum Second: u8 { A, B }

fn main() -> u8 {
    fix source = First.A
    fix value = source as Second or Second.A
    return 0
}
```

### expr-071 整数とcharの変換で成功範囲と代替値を扱う
- 主根拠：`[char.cast]`
- 結果：正常終了

```suz
fn scalar(n: i32) -> char { return n as char or \? }
fn main() -> u8 {
    fix byte: u8 = 65
    fix ch = byte as char
    fix narrow: u8 = \あ as u8 or 7
    fix code = ch as u32
    log info "${ch} ${code} ${narrow}"
    fix surrogate = scalar(55296)
    fix negative = scalar(-1)
    fix above = scalar(1114112)
    log info "${surrogate} ${negative} ${above}"
    return 0
}
```

**標準エラー：**

```text
info expr-071.suz:7: A 65 7
info expr-071.suz:11: ? ? ?
```

### expr-045 必ず成功する変換と定数式の変換
- 主根拠：`[expr.cast-risk]`
- 補助根拠：`[const.cast-check]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix small: u8 = 7
    fix wide = small as i64
    fix literal = 255 as u8
    return ifexpr wide == 7 && literal == 255 { 0 } else { 1 }
}
```

### expr-046 値が小さくてもfixからの縮小変換にはorが必要
- 主根拠：`[expr.cast-risk]`
- 補助根拠：`[const.expression]`、`[expr.cast-handler]`
- 結果：コンパイルエラー `[expr.cast-handler]` 3:21

rawはconstではなく、変換は型で判定すると失敗しうる。

```suz
fn main() -> u8 {
    fix raw: i64 = 1
    fix value = raw as u8
    return value
}
```

### expr-047 変換元を1回だけ評価し、成功時の代替値を省略
- 主根拠：`[expr.cast-fallback]`
- 補助根拠：`[log.output]`
- 結果：正常終了 7

```suz
fn source() -> i64 {
    log info "source"
    return 7
}

fn fallback() -> u8 {
    log info "fallback"
    return 9
}

fn main() -> u8 {
    fix value = source() as u8 or fallback()
    return value
}
```

**標準エラー：**

```text
info expr-047.suz:2: source
```

### expr-048 変換失敗時だけ代替値を評価
- 主根拠：`[expr.cast-fallback]`
- 結果：正常終了 9

```suz
fn source() -> i64 {
    log info "source"
    return 300
}

fn fallback() -> u8 {
    log info "fallback"
    return 9
}

fn main() -> u8 {
    fix value = source() as u8 or fallback()
    return value
}
```

**標準エラー：**

```text
info expr-048.suz:2: source
info expr-048.suz:7: fallback
```

### expr-049 orの代替値の型不一致
- 主根拠：`[expr.cast-fallback]`
- 結果：コンパイルエラー `[type.expected]` 3:30

代替値にはu8を要求する。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix value = raw as u8 or false
    return 0
}
```

### expr-050 or return exprで関数を抜ける
- 主根拠：`[expr.cast-return]`
- 補助根拠：`[expr.block]`、`[expr.or-return-position]`
- 結果：正常終了 7

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count = raw as u8 or return expr {
        log info "conversion failed"
        7
    }
    log info "count=${count}"
    return 0
}
```

**標準エラー：**

```text
info expr-050.suz:4: conversion failed
```

### expr-051 or return exprの成功経路
- 主根拠：`[expr.cast-return]`
- 補助根拠：`[expr.or-return-position]`
- 結果：正常終了 3

```suz
fn main() -> u8 {
    fix raw: i64 = 3
    fix count = raw as u8 or return expr {
        log info "conversion failed"
        7
    }
    return count
}
```

### expr-052 戻り値voidのor return
- 主根拠：`[expr.cast-return]`
- 補助根拠：`[fn.return]`
- 結果：正常終了

```suz
fn report(raw: i64) -> void {
    fix count = raw as u8 or return
    log info "count=${count}"
}

fn main() -> u8 {
    report(300)
    log info "done"
    return 0
}
```

**標準エラー：**

```text
info expr-052.suz:8: done
```

### expr-059 キャストのorは右側の式全体を代替値にする
- 主根拠：`[expr.cast-binding]`
- 補助根拠：`[expr.cast-fallback]`
- 結果：正常終了

変換は成功するので、firstは7。secondは変換結果に2を加えて9になる。

```suz
fn main() -> u8 {
    fix raw: i64 = 7
    fix first = raw as u8 or 1 + 2
    fix second = (raw as u8 or 1) + 2
    return ifexpr first == 7 && second == 9 { 0 } else { 1 }
}
```

### expr-060 enumと整数の相互変換
- 主根拠：`[expr.enum-conversion]`
- 補助根拠：`[enum.variant]`、`[expr.cast-risk]`、`[rt.as-int-enum]`
- 結果：正常終了

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix raw: u8 = 1
    fix wave = raw as Wave or Wave.Sine
    fix code = wave as u8
    fix invalid: u8 = 9
    fix fallback = invalid as Wave or Wave.Sine
    return ifexpr code == 1 && fallback == Wave.Sine { 0 } else { 1 }
}
```

### expr-061 同じ加算の失敗をorで処理
- 主根拠：`[expr.arithmetic-handler]`
- 補助根拠：`[expr.arithmetic-fallback]`
- 結果：正常終了 7

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = (top + 1) or 7
    return result
}
```

### expr-062 浮動小数点の除算に演算失敗のorを付ける
- 主根拠：`[expr.arithmetic-handler]`
- 補助根拠：`[rt.float]`
- 結果：コンパイルエラー `[expr.arithmetic-handler]` 3:31

演算失敗を処理するorの対象は整数演算。浮動小数点の除算には付けられない。

```suz
fn main() -> u8 {
    fix zero = 0.0
    fix result = (1.0 / zero) or 0.0
    return 0
}
```

### expr-063 算術のorは対象の演算を1回だけ評価
- 主根拠：`[expr.arithmetic-fallback]`
- 補助根拠：`[expr.arithmetic-scope]`
- 結果：正常終了 7

```suz
fn left_value() -> u8 {
    log info "left"
    return 250
}

fn right_value() -> u8 {
    log info "right"
    return 10
}

fn alternative() -> u8 {
    log info "alternative"
    return 7
}

fn main() -> u8 {
    fix total = (left_value() + right_value()) or alternative()
    return total
}
```

**標準エラー：**

```text
info expr-063.suz:2: left
info expr-063.suz:7: right
info expr-063.suz:12: alternative
```

### expr-064 加算全体を処理してから代入する
- 主根拠：`[expr.arithmetic-fallback]`
- 補助根拠：`[assign.target]`
- 結果：正常終了 7

countへの加算が失敗した場合の代替値を、通常の代入で保存する。

```suz
fn main() -> u8 {
    var count: u8 = 255
    count = (count + 1) or 7
    return count
}
```

### expr-065 整数演算の失敗時に初期化を中断して戻る
- 主根拠：`[expr.arithmetic-fallback]`
- 補助根拠：`[expr.or-return-position]`
- 結果：正常終了 7

加算全体がfixの初期値なのでor returnを付けられる。オーバーフローしたら7を返し、後続のreturnには進まない。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = (top + 1) or return 7
    return result
}
```

### expr-066 通常の代入で除算と加算をそれぞれ処理する
- 主根拠：`[expr.arithmetic-scope]`
- 補助根拠：`[assign.target]`、`[expr.arithmetic-fallback]`
- 結果：正常終了

bind-028の複合代入を通常の代入に書き換えた形。divisorが0なら除算の代替値0を加えて255になる。1なら除算は成功するが加算がオーバーフローし、外側の代替値1を代入する。

```suz
fn calculate(divisor: u8) -> u8 {
    var count: u8 = 255
    count = (count + ((1 / divisor) or 0)) or 1
    return count
}

fn main() -> u8 {
    fix division_failed = calculate(0)
    fix addition_failed = calculate(1)
    return ifexpr division_failed == 255 && addition_failed == 1 { 0 } else { 1 }
}
```

### expr-067 orは算術式全体の格納失敗を処理する
- 主根拠：`[expr.arithmetic-scope]`
- 補助根拠：`[rt.int-overflow]`
- 結果：正常終了

途中の加算では失敗せず、結果256をu8へ格納するときに失敗する。orの代替値0を返す。

```suz
fn operation(value: u8) -> u8 {
    fix result = ((value + 1) * 1) or 0
    return result
}

fn main() -> u8 {
    return operation(255)
}
```

### expr-068 呼び出し先のパニックをorで捕まえない
- 主根拠：`[expr.arithmetic-scope]`
- 補助根拠：`[rt.panic]`、`[rt.div-zero]`
- 結果：パニック `[rt.div-zero]` 2:14

divide内の除算が失敗する。加算に付いたorは実行しない。標準エラーに除算の位置と理由を出す。

```suz
fn divide(divisor: i32) -> i32 {
    return 1 / divisor
}

fn main() -> u8 {
    fix result = (divide(0) + 1) or 0
    return 0
}
```

### expr-072 text全体を1つのスカラー値に変換する
- 主根拠：`[char.cast-symbol]`
- 結果：正常終了

```suz
fn parse(input: symbol) -> char { return input as char or \? }
fn main() -> u8 {
    fix emoji = parse("😀")
    fix empty = parse("")
    fix multiple = parse("ab")
    fix quoted = parse("\\a")
    log info "${emoji} ${empty} ${multiple} ${quoted}"
    return 0
}
```

**標準エラー：**

```text
info expr-072.suz:7: 😀 ? ? ?
```

### expr-073 構造体のcharフィールドを文字リテラルとして読む
- 主根拠：`[symbol.cast-struct-field]`
- 結果：正常終了

```suz
struct Letter { ch: char }
fn read(input: symbol) -> Letter { return input as Letter or Letter { ch: \? } }
fn main() -> u8 {
    fix good = read("Letter { ch: \\u{1F600} }")
    fix bad = read("Letter { ch: \\ab }")
    log info "${good.ch} ${bad.ch}"
    return 0
}
```

**標準エラー：**

```text
info expr-073.suz:6: 😀 ?
```

### expr-074 途中の値を保持して格納時だけ範囲を検査する
- 主根拠：`[arith.exact]`
- 結果：正常終了

和300と積20000を途中で切り詰めない。外側からu16を要求しても、or付きの内側の算術式はu8への格納に失敗し255を返す。単一の整数16777218はf32で正確に表せる。

```suz
fn main() -> u8 {
    fix a: u8 = 200
    fix b: u8 = 100
    fix sum: u16 = a + b
    fix scaled: u8 = a * b / 250
    fix fallback: u8 = a + b or 7
    fix bounded: u16 = (a + b or 255) * 2
    fix exact: f32 = 16_777_217 + 1
    return ifexpr sum == 300 && scaled == 80 && fallback == 7 && bounded == 510 && exact == 16_777_218.0 { 0 } else { 1 }
}
```

### expr-075 ゼロ除算より前の128ビット超過を拒否する
- 主根拠：`[arith.width]`
- 結果：コンパイルエラー `[arith.width]` 3:38

最初の積は符号なし128ビットで表せるが、次の積の見積もりは収まらない。実際の変数の値と後続のゼロ除算は、この検査を省略する理由にならない。

```suz
fn main() -> u8 {
    fix value: u64 = 1
    fix result: u64 = (value * value * value / 0) or 0
    return 0
}
```

### expr-089 通常のifガードで算術とasの範囲を狭めてorを省略できる
- 主根拠：`[arith.width]`
- 補助根拠：`[expr.cast-risk]`、`[array.bounds-proof]`
- 結果：正常終了

通常の `if 1 <= n <= 100` の本体内でも `n` の範囲が狭まり、`(n * 2) as u8` や `n as u8` の `or` を省略できる。

```suz
fn double_if_small(n: i32) -> u8 {
    if 1 <= n <= 100 {
        fix copy = n as u8
        if copy == n {
            return (n * 2) as u8
        }
    }
    return 0
}

fn main() -> u8 {
    if double_if_small(25) == 50 && double_if_small(200) == 0 {
        return 0
    }
    return 1
}
```

### expr-076 型の範囲から必ずゼロで割る式を拒否する
- 主根拠：`[arith.div-zero-always]`
- 結果：コンパイルエラー `[arith.div-zero-always]` 3:24

valueの実際の値を使わなくても、value % 1の範囲は0だけになる。失敗処理がないため拒否する。

```suz
fn main() -> u8 {
    fix value: u8 = 7
    fix result = value / (value % 1)
    return 0
}
```

### expr-077 整数と浮動小数点数を丸めずに比較する
- 主根拠：`[conv.compare]`
- 結果：正常終了

整数をf64へ丸めず、1の差を保つ。符号の異なる整数、NaN、負のゼロもそれぞれの値の規則で比較する。

```suz
fn main() -> u8 {
    fix integer: u64 = 9_007_199_254_740_993
    fix rounded: f64 = 9_007_199_254_740_992.0
    fix negative: i8 = -1
    return ifexpr integer > rounded && rounded < integer && negative < integer && integer != nan && not (integer <= nan) && 0 == -0.0 { 0 } else { 1 }
}
```

### expr-083 正確な算術式からの飽和変換
- 主根拠：`[arith.as-sat]`
- 補助根拠：`[arith.exact-use]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix x: i64 = 9223372036854775807
    fix y: i64 = (x + x) as| i64
    return ifexpr y == x { 0 } else { 1 }
}
```
