# 仕様例：型（値域制限型）

### type-038 値域制限型の宣言（半開区間と1値型）
- 主根拠：`[range.decl]`
- 補助根拠：`[range.bounds]`
- 結果：正常終了 3

半開区間 `0..<256`（`u8` に収まる `0..=255`）や 1 値型 `3..=3` を含む値域制限型を宣言し、範囲内のリテラルで初期化できる。

```suz
range ByteValue: u8 = 0..<256
range Three: u8 = 3..=3

fn main() -> u8 {
    fix b: ByteValue = 255
    fix t: Three = 3
    if b == 255 {
        return t
    }
    return 1
}
```

### type-039 値域制限型の基底型にlen_tを指定すると拒否する
- 主根拠：`[range.decl]`
- 結果：コンパイルエラー `[range.decl]` 1:12

値域制限型の基底型には固定幅整数型のみを指定でき、`len_t` は使えない。

```suz
range Bad: len_t = 0..=10

fn main() -> u8 {
    return 0
}
```

### type-040 値域制限型の基底型に値域制限型を指定すると拒否する
- 主根拠：`[range.decl]`
- 結果：コンパイルエラー `[range.decl]` 2:12

値域制限型の基底型に別の値域制限型は使えない。

```suz
range Note: u8 = 0..=127
range Sub: Note = 0..=10

fn main() -> u8 {
    return 0
}
```

### type-041 値域制限型の空区間を拒否する
- 主根拠：`[range.bounds]`
- 結果：コンパイルエラー `[range.bounds]` 1:7

半開区間 `0..<0` には含まれる値が 1 つもないためコンパイルエラーとなる。

```suz
range Empty: u8 = 0..<0

fn main() -> u8 {
    return 0
}
```

### type-042 値域制限型の逆転区間を拒否する
- 主根拠：`[range.bounds]`
- 結果：コンパイルエラー `[range.bounds]` 1:7

下限が上限より大きい区間 `5..=3` はコンパイルエラーとなる。

```suz
range Reversed: u8 = 5..=3

fn main() -> u8 {
    return 0
}
```

### type-043 値域制限型の基底型超過を拒否する
- 主根拠：`[range.bounds]`
- 結果：コンパイルエラー `[range.bounds]` 1:7

`0..=256` は基底型 `u8` の範囲を超えるためコンパイルエラーとなる。

```suz
range Over: u8 = 0..=256

fn main() -> u8 {
    return 0
}
```

### type-044 値域制限型から通常整数型・len_t・浮動小数点型へ暗黙に拡大変換する
- 主根拠：`[range.widen]`
- 補助根拠：`[conv.widen]`
- 結果：正常終了

値域制限型 `Note`（`0..=127`）の全範囲を正確に表せる `u8`・`len_t`・`f64` へ暗黙に拡大変換できる。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 100
    fix as_u8: u8 = n
    fix as_len: len_t = n
    fix as_f64: f64 = n
    if as_u8 == 100 && as_len == 100 && as_f64 == 100.0 {
        return 0
    }
    return 1
}
```

### type-045 同範囲の別名値域型の取り違えを拒否する
- 主根拠：`[range.widen]`
- 補助根拠：`[type.expected]`
- 結果：コンパイルエラー `[type.expected]` 10:17

同じ範囲 `0..=127` を持つ `Note` と `Velocity` でも別の名前付き型として区別し、暗黙に受け渡せない。

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127

fn play(n: Note) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v: Velocity = 64
    return play(v)
}
```

### type-046 通常整数型の変数から値域制限型への暗黙変換を拒否する
- 主根拠：`[range.widen]`
- 補助根拠：`[type.expected]`
- 結果：コンパイルエラー `[type.expected]` 5:19

`u8` の全範囲が `Wide`（`0..=1000`）に含まれていても、名前付きの値域制限型へは暗黙変換できない。

```suz
range Wide: u16 = 0..=1000

fn main() -> u8 {
    fix x: u8 = 10
    fix w: Wide = x
    return 0
}
```

### type-047 値域制限型への包含が静的に分かるasと整数からのas|飽和
- 主根拠：`[range.cast]`
- 補助根拠：`[expr.cast-risk]`、`[expr.as-sat]`
- 結果：正常終了

包含が静的に分かる明示変換（同範囲の `Note` → `Velocity`、`u8` → `0..=1000`）は `or` なしで変換でき、`as|` は値域の上下限へ飽和する。

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127
range Wide: u16 = 0..=1000

fn main() -> u8 {
    fix n: Note = 100
    fix as_u8: u8 = n
    fix v: Velocity = n as Velocity
    fix w: Wide = as_u8 as Wide
    fix clamped_hi: Note = 200 as| Note
    fix neg: i16 = -5
    fix clamped_lo: Note = neg as| Note
    if v == 100 && w == 100 && clamped_hi == 127 && clamped_lo == 0 {
        return 0
    }
    return 1
}
```

### type-048 値域制限型へのas%を拒否する
- 主根拠：`[range.cast]`
- 結果：コンパイルエラー `[range.cast]` 5:21

値域制限型は `as%` の変換先にできない。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix x: u16 = 200
    fix n: Note = x as% Note
    return 0
}
```

### type-049 文字列から値域制限型へのas%を拒否する
- 主根拠：`[range.cast]`
- 補助根拠：`[range.cast-text]`
- 結果：コンパイルエラー `[range.cast]` 4:24

`symbol`・`text` からも値域制限型への `as%` は対象外である。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = "10" as% Note or 0
    return 0
}
```

### type-050 浮動小数点型から値域制限型へのas|で±infを上下限へ飽和する
- 主根拠：`[range.cast-float-sat]`
- 補助根拠：`[expr.as-sat]`
- 結果：正常終了

浮動小数点型から値域制限型への `as|` では `inf` が上限、`-inf` が下限へ飽和する。

```suz
range Note: u8 = 0..=127
range Divisor: u16 = 1..=1000

fn main() -> u8 {
    fix pos_inf: f64 = inf
    fix neg_inf: f64 = -inf
    fix hi: Note = pos_inf as| Note or 0
    fix lo: Divisor = neg_inf as| Divisor or 50
    if hi == 127 && lo == 1 {
        return 0
    }
    return 1
}
```

### type-051 0を含まない値域型への浮動小数点as|でor省略を拒否する
- 主根拠：`[range.cast-float-sat]`
- 結果：コンパイルエラー `[range.cast-float-sat]` 5:24

`Divisor`（`1..=1000`）は `0` を含まないため、浮動小数点からの `as|` で NaN ハンドラ `or` を省略するとコンパイルエラーとなる。

```suz
range Divisor: u16 = 1..=1000

fn main() -> u8 {
    fix x: f64 = nan
    fix d: Divisor = x as| Divisor
    return 0
}
```

### type-052 文字列から値域制限型へのasとas|で値域を照合・飽和する
- 主根拠：`[range.cast-text]`
- 補助根拠：`[symbol.cast-range]`、`[symbol.cast-sat]`
- 結果：正常終了

文字列からの `as` は値域内のみ成功し、`as|` は値域の上下限へ飽和する。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix parsed: Note = "127" as Note or 0
    fix out_of_range: Note = "128" as Note or 9
    fix sat_hi: Note = "1000" as| Note or 0
    fix sat_lo: Note = "-5" as| Note or 1
    if parsed == 127 && out_of_range == 9 && sat_hi == 127 && sat_lo == 0 {
        return 0
    }
    return 1
}
```

### type-053 値域制限型フィールドを持つ構造体への文字列変換で値域を照合する
- 主根拠：`[range.cast-text]`
- 補助根拠：`[symbol.cast-struct-field]`、`[text.cast-view]`
- 結果：正常終了

値域制限型フィールドを持つ構造体へ `symbol`・`text` から `as` で変換すると、範囲内は成功し、範囲外（`200`）は失敗して `or` へ進む。

```suz
range Note: u8 = 0..=127

struct Pk {
    n: Note,
}

fn main() -> u8 {
    fix ok_sym: Pk = "Pk { n: 5 }" as Pk or Pk { n: 9 }
    fix bad_sym: Pk = "Pk { n: 200 }" as Pk or Pk { n: 9 }
    fix t_ok: text = "Pk { n: 127 }"
    fix t_bad: text = "Pk { n: 128 }"
    fix ok_txt = t_ok as Pk or Pk { n: 0 }
    fix bad_txt = t_bad as Pk or Pk { n: 7 }
    if ok_sym.n == 5 && bad_sym.n == 9 && ok_txt.n == 127 && bad_txt.n == 7 {
        return 0
    }
    return 1
}
```

### type-054 異なる値域型の直接比較を拒否する
- 主根拠：`[range.compare]`
- 結果：コンパイルエラー `[range.compare]` 7:10

同じ範囲を持つ `Note` と `Velocity` でも直接比較できない。

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix v: Velocity = 60
    if n == v {
        return 1
    }
    return 0
}
```

### type-055 値域制限型の範囲だけで決まる比較で定数比較警告を出す
- 主根拠：`[range.compare]`
- 補助根拠：`[diagnostic.constant-comparison]`
- 結果：正常終了 / 警告：`[diagnostic.constant-comparison]` 4:10

`Channel`（`0..<16`）に対する `c < 16` は常に真であるため、通常設定でも `[diagnostic.constant-comparison]` を警告する。

```suz
range Channel: u8 = 0..<16

fn check(c: Channel) -> u8 {
    if c < 16 {
        return 0
    }
    return 1
}

fn main() -> u8 {
    fix c: Channel = 3
    return check(c)
}
```

### type-056 値域制限型の算術・非零除算・非負剰余の格納
- 主根拠：`[range.arith]`
- 補助根拠：`[arith.width]`、`[arith.store]`
- 結果：正常終了

`(0..=127) + (0..=127)` の `u8` 格納、`x %% 24` の `Hour`（`0..=23`）への無失敗格納、および `0` を含まない `Divisor` による除算の格納失敗 `or` を確認する。

```suz
range Note: u8 = 0..=127
range Divisor: u16 = 1..=1000
range Hour: u8 = 0..=23

fn quotient(x: u16, d: Divisor) -> u8 {
    return x / d or 255
}

fn wrap_hour(x: i32) -> Hour {
    return x %% 24
}

fn main() -> u8 {
    fix n: Note = 100
    fix sum = n + n
    fix d: Divisor = 1
    fix q_ok = quotient(200, d)
    fix q_ovf = quotient(300, d)
    fix h: Hour = wrap_hour(-1)
    if sum == 200 && q_ok == 200 && q_ovf == 255 && h == 23 {
        return 0
    }
    return 1
}
```

### type-057 値域制限型へのラップ算術を拒否する
- 主根拠：`[range.prohibited-ops]`
- 結果：コンパイルエラー `[range.prohibited-ops]` 5:15

値域制限型はラップ算術（`+%` 等）の対象外である。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix m = n +% n
    return 0
}
```

### type-058 値域制限型への飽和算術を拒否する
- 主根拠：`[range.prohibited-ops]`
- 結果：コンパイルエラー `[range.prohibited-ops]` 5:15

値域制限型は飽和算術（`+|` 等）の対象外である。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix m = n +| n
    return 0
}
```

### type-059 値域制限型へのビット演算を拒否する
- 主根拠：`[range.prohibited-ops]`
- 結果：コンパイルエラー `[range.prohibited-ops]` 5:13

値域制限型を直接のオペランドとするビット演算は対象外である。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 60
    fix m = ~n
    return 0
}
```

### type-060 値域制限型を左辺とするシフトを拒否する
- 主根拠：`[range.prohibited-ops]`
- 結果：コンパイルエラー `[range.prohibited-ops]` 5:15

値域制限型はシフトの左オペランドにできない。

```suz
range Note: u8 = 0..=127

fn main() -> u8 {
    fix n: Note = 1
    fix m = n << 1
    return 0
}
```

### type-061 匿名汎用intの定義時検査は無関係なrange宣言に依存しない
- 主根拠：`[range.generic]`
- 補助根拠：`[num.fn-check-judge]`
- 結果：正常終了

`0` を含まない無関係な `range Divisor: u16 = 1..=1000` が宣言されていても、`fn zero() -> int { return 0 }` の定義時検査は失敗せず、`u8` での呼び出しが受理される。

```suz
range Divisor: u16 = 1..=1000

fn zero() -> int {
    return 0
}

fn main() -> u8 {
    fix z: u8 = zero()
    return z
}
```

### type-062 匿名汎用intを0を含まない値域型で具体化したときリテラル不適合を拒否する
- 主根拠：`[range.generic]`
- 補助根拠：`[num.fn-body]`、`[lit.range]`
- 結果：コンパイルエラー `[num.fn-body]` 8:22

`fn zero() -> int { return 0 }` を `Divisor`（`1..=1000`）で具体化すると、`0` が `Divisor` の範囲外であるためコンパイルエラーとなる。

```suz
range Divisor: u16 = 1..=1000

fn zero() -> int {
    return 0
}

fn main() -> u8 {
    fix d: Divisor = zero()
    return 0
}
```

### type-063 匿名汎用intで値域型へ具体化した算術の結果超過は実行時にパニックする
- 主根拠：`[range.generic]`
- 補助根拠：`[arith.store]`、`[rt.int-overflow]`
- 結果：パニック `[rt.int-overflow]` 4:14

`twice(note)` を `Note`（`0..=127`）で具体化したとき、`64 + 64 = 128` はコンパイルエラーとせず実行時の格納失敗としてパニックする。

```suz
range Note: u8 = 0..=127

fn twice(x: int) -> int {
    return x + x
}

fn main() -> u8 {
    fix n: Note = 64
    fix r = twice(n)
    return r
}
```

### type-064 匿名汎用intで同一値域型は保持し、通常整数候補があれば通常整数を選ぶ
- 主根拠：`[range.generic-resolve]`
- 補助根拠：`[num.int]`、`[num.fn-resolve]`
- 結果：正常終了

`min(note1, note2)` は `Note` を返し、要求型なしの `twice(note)` は `Note` で具体化され、`fix as_u8: u8 = twice(big)` は通常整数候補 `u8` で具体化される。

```suz
range Note: u8 = 0..=127

fn min(a: int, b: int) -> int {
    return ifexpr a < b { a } else { b }
}

fn twice(x: int) -> int {
    return x + x
}

fn main() -> u8 {
    fix n1: Note = 63
    fix n2: Note = 40
    fix m: Note = min(n1, n2)
    fix n_ok: Note = twice(n1)
    fix big: Note = 100
    fix as_u8: u8 = twice(big)
    if m == 40 && n_ok == 126 && as_u8 == 200 {
        return 0
    }
    return 1
}
```

### type-065 匿名汎用intで通常整数候補のない異なる値域型の混在を拒否する
- 主根拠：`[range.generic-resolve]`
- 補助根拠：`[num.fn-resolve]`
- 結果：コンパイルエラー `[num.fn-resolve]` 11:13

`Note` と `Velocity` のように異なる値域制限型だけが候補にある場合、`[num.fn-resolve]` で拒否する。

```suz
range Note: u8 = 0..=127
range Velocity: u8 = 0..=127

fn min(a: int, b: int) -> int {
    return ifexpr a < b { a } else { b }
}

fn main() -> u8 {
    fix n: Note = 60
    fix v: Velocity = 80
    fix m = min(n, v)
    return 0
}
```

### type-066 型引数Tは明示しても推論しても値域制限型をそのまま保持する
- 主根拠：`[range.type-arg]`
- 補助根拠：`[generic.parameters]`、`[generic.inference]`
- 結果：正常終了

型引数 `T` に値域制限型を明示（`keep[Note](n)`）しても推論（`keep(n)`）しても、基底型へ置き換えずに値域制限型をそのまま保持する。

```suz
range Note: u8 = 0..=127

fn keep[T](x: T) -> T {
    return x
}

fn main() -> u8 {
    fix n: Note = 63
    fix explicit: Note = keep[Note](n)
    fix inferred: Note = keep(n)
    if explicit == 63 && inferred == 63 {
        return 0
    }
    return 1
}
```

### type-067 値域制限型を対象とするmatchとmatchexpr
- 主根拠：`[range.match]`
- 補助根拠：`[match.pattern]`、`[match.exhaustive]`
- 結果：正常終了

値域制限型を対象とする `match`・`matchexpr` で、範囲内の定数パターンと末尾の `_` により分岐できる。

```suz
range Hour: u8 = 0..=23

fn main() -> u8 {
    fix h: Hour = 23
    fix m = matchexpr h {
        23 => 1
        _ => 0
    }
    match h {
        23 => { return ifexpr m == 1 { 0 } else { 1 } }
        _ => { return 2 }
    }
}
```

### type-068 値域制限型のmatchで範囲外パターンを拒否する
- 主根拠：`[range.match]`
- 補助根拠：`[lit.range]`、`[range.bounds]`、`[match.pattern]`
- 結果：コンパイルエラー `[lit.range]` 7:9

値域制限型を対象とする `match` のパターンには対象と同じ値域制限型を要求するため、範囲外リテラルは拒否する。

```suz
range Bit: u8 = 0..=1

fn main() -> u8 {
    fix b: Bit = 0
    match b {
        0 => { return 0 }
        2 => { return 1 }
        _ => { return 2 }
    }
}
```

### type-069 値域制限型のmatchで全値列挙時も_を必須とする
- 主根拠：`[range.match]`
- 補助根拠：`[match.exhaustive]`
- 結果：コンパイルエラー `[match.exhaustive]` 5:5

値域制限型の `match` ではすべての値を列挙しても末尾の `_` が必須である。

```suz
range Bit: u8 = 0..=1

fn main() -> u8 {
    fix b: Bit = 0
    match b {
        0 => { return 0 }
        1 => { return 1 }
    }
}
```

### type-070 値域制限型を構造体・関数・配列要素・テンプレート埋め込みに使う
- 主根拠：`[range.places]`
- 結果：正常終了

値域制限型を構造体フィールド・仮引数・戻り値・配列要素・テンプレート埋め込み（10進数表示）に利用できる。

```suz
range ByteValue: u8 = 0..<256
range Three: u8 = 3..=3

struct Packet {
    value: ByteValue,
    tag: Three,
}

fn pass(v: ByteValue) -> ByteValue {
    return v
}

fn main() -> u8 {
    fix p = Packet { value: 255, tag: 3 }
    fix arr: #[ByteValue] = new #[0; 2]
    arr#[0] = pass(p.value)
    fix s: symbol = "${arr.[0]}:${p.tag}"
    if s == "255:3" {
        return 0
    }
    return 1
}
```
