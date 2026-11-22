# 汎用

[目次と書式](../spec-cases.md)

### flow-068 戻り値だけに汎用型を持つ関数を呼ぶ

主根拠：`[num.fn]`。

仮引数がなくても、戻り値に `int` を持つ関数は汎用関数である。受け取り先から具体型を決める。

**ソース：**

```suz
fn one() -> int {
    return 1
}
fn main() -> u8 {
    fix small: u8 = one()
    fix large: u64 = one()
    return ifexpr small == 1 && large == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-069 引数・戻り値・ローカル変数を同じ具体型にする

主根拠：`[num.fn-same]`。

補助根拠：`[num.fn-body-type]`、`[num.fn-recursion]`。

`count` の引数・戻り値・`previous` は、最初の呼び出しでは `u8`、次の呼び出しでは `u16` になる。再帰呼び出しでも、それぞれ同じ具体型を使う。

**ソース：**

```suz
fn count(n: int) -> int {
    if n == 0 { return 0 }
    fix previous: int = count(n - 1)
    return previous + 1
}
fn main() -> u8 {
    fix small: u8 = 3
    fix large: u16 = 300
    fix a = count(small)
    fix b = count(large)
    return ifexpr a == 3 && b == 300 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-070 独立したintとfloat、候補のない呼び出し

主根拠：`[num.fn-resolve]`。

`first` の `int` は `u8`、`float` は `f32` になる。`zero()` は受け取り先の要求型がなければ `i64`、`narrow` の初期値では `u8` になる。`half(4.0)` では受け取り先の `f32` を使う。

**ソース：**

```suz
fn first(value: int, factor: float) -> int {
    return value
}
fn zero() -> int {
    return 0
}
fn half(value: float) -> float {
    return value * 0.5
}
fn main() -> u8 {
    fix value: u8 = 12
    fix factor: f32 = 0.5
    fix picked = first(value, factor)
    fix plain = zero()
    fix narrow: u8 = zero()
    fix scaled: f32 = half(4.0)
    return ifexpr picked == 12 && plain == 0 && narrow == 0 && scaled == 2.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-071 intの結果を浮動小数点型で受け取る

主根拠：`[num.fn-convert]`。

補助根拠：`[num.fn-resolve]`。

`f64` は `int` の型候補に入らない。`identity(input)` は `u8` を返し、その結果を損失なく `f64` に広げる。算術式を渡した呼び出しでは、要求型なしで決まる `u8` を使う。

**ソース：**

```suz
fn identity(value: int) -> int {
    return value
}
fn main() -> u8 {
    fix input: u8 = 3
    fix result: f64 = identity(input)
    fix sum: u8 = identity(input + input)
    return ifexpr result == 3.0 && sum == 6 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-072 floatを返す内側の呼び出しをintの引数に渡す

主根拠：`[num.fn-convert]`。

補助根拠：`[num.fn-nested]`。

`half` の戻り値は `f32` であり、`int` の引数には渡せない。種類が異なる呼び出し間では具体型をそろえない。

**ソース：**

```suz
fn half(value: float) -> float {
    return value * 0.5
}
fn identity(value: int) -> int {
    return value
}
fn main() -> u8 {
    fix input: f32 = 4.0
    fix result = identity(half(input))
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-convert]` | 9 | 27 |

### flow-073 入れ子の呼び出し全体で具体型を決める

主根拠：`[num.fn-nested]`。

`input` の `u8` と受け取り先の `u16` をそろえ、`square` と `clamp` の `int` をともに `u16` にする。二乗はオーバーフローせず、40000を返す。

**ソース：**

```suz
fn square(value: int) -> int {
    return value * value
}
fn clamp(value: int, lo: int, hi: int) -> int {
    return ifexpr value < lo { lo } elif value > hi { hi } else { value }
}
fn main() -> u8 {
    fix input: u8 = 200
    fix result: u16 = clamp((square(input)), 0, 50_000)
    return ifexpr result == 40_000 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-074 汎用検査の診断にはintを使う

主根拠：`[num.fn-check]`。

呼び出しで型を決める前に、戻り値のboolがintと一致しないことを検出する。診断の型名はintとなる。

**ソース：**

```suz
fn identity(value: int) -> int {
    return true
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 2 | 12 |

### flow-075 型に依存する判定を保留して外側の演算を検査する

主根拠：`[num.fn-check-judge]`。

300が収まるかは具体型に依存するため保留する。その式とboolの加算はどの具体型でもできないため、本体で診断する。

**ソース：**

```suz
fn add(value: int) -> int {
    fix result = (value +% 300) + true
    return result
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.operator-types]` | 2 | 35 |

### flow-076 呼び出しで決まった型が本体の戻り値型に収まらない

主根拠：`[num.fn-body]`。

`small` の `int` は `u16` になる。本体の `return value` は `u8` へ暗黙に縮小できないため、呼び出し位置に診断し、本体の違反位置も示す。

**ソース：**

```suz
fn small(value: int) -> u8 {
    return value
}
fn main() -> u8 {
    fix value: u16 = 300
    return small(value)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-body]` | 6 | 12 |

### flow-077 汎用検査を打ち切るまでに保留した判定を呼び出しで行う

主根拠：`[num.fn-body]`。

補助根拠：`[num.fn-check]`。

300がintに収まるかを保留した後で、boolへの整数の代入を検出する。呼び出しでは先に保留した判定を行い、u8に300が収まらないことを報告する。関連診断は7行18列の `[lit.range]` となる。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix k = f(a)
    return 0
}
fn f(n: int) -> int {
    fix y = n +% 300
    fix z: bool = 1
    return n
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-body]` | 3 | 13 |

### flow-078 汎用検査を打ち切った後の判定は呼び出しで行わない

主根拠：`[num.fn-body]`。

補助根拠：`[num.fn-check]`。

boolへの整数の代入で汎用検査を打ち切るため、その後の300については判定を保留していない。呼び出し位置ではエラーを出さず、本体の違反を宣言順に報告する。

**ソース：**

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix k = f(a)
    return 0
}
fn f(n: int) -> int {
    fix z: bool = 1
    fix y = n +% 300
    return n
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 7 | 19 |

### flow-079 未使用の汎用関数から未宣言の名前を参照する

主根拠：`[num.fn-unused]`。

`unused` を呼び出さなくても、`missing` が未宣言であることは検出する。

**ソース：**

```suz
fn unused(value: int) -> int {
    return missing
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[decl.local-scope]` | 2 | 12 |

### flow-080 未使用関数の型に依存しない誤りを検出する

主根拠：`[num.fn-unused]`。

`narrowed` の初期化は具体型によって成否が変わるが、整数リテラルを `text` にする初期化は具体型によらず誤りである。前の文の型検査に失敗する組み合わせがあっても、後の誤りを見逃さない。

**ソース：**

```suz
fn unused(value: int) -> u8 {
    fix narrowed: u8 = value
    fix message: text = 5
    return narrowed
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 3 | 25 |

### flow-081 未使用関数の整数演算にもallowを適用する

主根拠：`[num.fn-unused]`。
補助根拠：`[allow.unused]`。

関数を呼び出さなくても整数演算の警告対象を集計する。警告設定を省略した場合も、対象があるためallowを受理する。強制設定のエラーは抑制しない。

**ソース：**

```suz
fn next(value: int) -> int {
    return value + 1 allow [diagnostic.arithmetic: bounded by the caller]
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
| 警告 | 正常終了 | 0 | 0 |
| 強制 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 2 | 18 |

### flow-082 未使用関数でもすべての具体型で一定の比較を警告する

主根拠：`[num.fn-warning]`。

`value >= 0` は符号付き整数型では一定にならず、警告しない。`small >= 0` はどの具体型でも常に真になるため、関数を呼ばなくても1回警告する。

**ソース：**

```suz
fn nonnegative(value: int) -> bool {
    return value >= 0
}
fn always(value: int) -> bool {
    fix small: u8 = 5
    return small >= 0
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 1 |

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[diagnostic.constant-comparison]` | 6 | 18 |

### flow-083 宣言していないfloatを変換先に使う

主根拠：`[num.fn-undeclared]`。

関数の引数型と戻り値型に現れる汎用型は `int` だけである。`float` は関数本体の変換先にも使えない。

**ソース：**

```suz
fn convert(value: int) -> int {
    fix converted = value as float
    return value
}
fn main() -> u8 {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-undeclared]` | 2 | 27 |

### flow-084 本体のint注釈と注釈なしの型推論を区別する

主根拠：`[num.fn-body-type]`。

`typed` は呼び出しごとに `u8`・`u64` となり、255に1を加えるラップ演算の結果は0・256となる。注釈のない `inferred` はどちらも `i64` となり、結果は256となる。

**ソース：**

```suz
fn values(input: int) -> int {
    fix typed: int = 255
    fix inferred = 255
    if inferred +% 1 != 256 { return 99 }
    return typed +% 1
}
fn main() -> u8 {
    fix small: u8 = 1
    fix large: u64 = 1
    return ifexpr values(small) == 0 && values(large) == 256 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-085 安全な具体型で呼び出してもキャストのorを省略できない

主根拠：`[num.fn-cast]`。

この呼び出しでは `int` が `u8` になるが、他の具体型では変換が失敗しうる。関数本体の `as u8` には `or` が必要である。

**ソース：**

```suz
fn narrow(value: int) -> u8 {
    return value as u8
}
fn main() -> u8 {
    fix value: u8 = 1
    return narrow(value)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-cast]` | 2 | 18 |

### flow-086 具体型を経由した定数のキャストを受理する

主根拠：`[num.fn-cast]`。

中間の `u16` への変換後も、内側の `as% int` による値の違いを判定する。1はすべての具体型で最終的に `u8` に収まる。

**ソース：**

```suz
fn narrow(input: int) -> u8 {
    return ((1 as% int) as u16 or 0) as u8
}
fn main() -> u8 {
    fix input: u8 = 0
    return ifexpr narrow(input) == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-087 具体型を経由した定数のキャストにもorを要求する

主根拠：`[num.fn-cast]`。

中間の `u16` への変換後も、内側の `as% int` による値の違いを判定する。300は `int` が `u16` などの場合に `u8` に収まらない。呼び出しで `u8` を選んでも、末尾の `as u8` に `or` が必要である。

**ソース：**

```suz
fn narrow(input: int) -> u8 {
    return ((300 as% int) as u16 or 0) as u8
}
fn main() -> u8 {
    fix input: u8 = 0
    return ifexpr narrow(input) == 1 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[num.fn-cast]` | 2 | 40 |

### flow-088 汎用関数同士を相互再帰で呼ぶ

主根拠：`[num.fn-recursion]`。

`even` と `odd` を行き来しても、各呼び出しで決まった `u8`・`u16` を引き継ぐ。

**ソース：**

```suz
fn even(value: int) -> bool {
    if value == 0 { return true }
    return odd(value - 1)
}
fn odd(value: int) -> bool {
    if value == 0 { return false }
    return even(value - 1)
}
fn main() -> u8 {
    fix small: u8 = 4
    fix large: u16 = 301
    return ifexpr even(small) && odd(large) { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### flow-089 mainの戻り値をintにする

主根拠：`[num.fn-main]`。

`main` の戻り値は `u8` であり、汎用型の `int` にはできない。

**ソース：**

```suz
fn main() -> int {
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[prog.main]` | 1 | 14 |

### generic-001 型引数を指定して構造体を作る

主根拠：`[generic.parameters]`。

**ソース：**

```suz
struct Box[T] { value: T }
fn wrap[T](value: T) -> Box[T] { return Box[T] { value: value } }
fn main() -> u8 {
    fix b = wrap[i32](10)
    if b.value != 10 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### generic-002 列挙型に型引数を宣言できない

主根拠：`[generic.parameters]`。

**ソース：**

```suz
enum Kind[T]: u8 { A }
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[enum.base]` | 1 | 10 |

### generic-003 型引数名を重複できない

主根拠：`[generic.parameters]`。

**ソース：**

```suz
fn same[T, T](x: T) -> T { return x }
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.parameters]` | 1 | 12 |

### generic-004 voidを型引数に使えない

主根拠：`[generic.type-arguments]`。

**ソース：**

```suz
fn identity[T](value: T) -> T { return value }
fn main() -> u8 {
    identity[void]()
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.type-arguments]` | 3 | 14 |

### generic-005 型引数の個数を合わせる

主根拠：`[generic.type-arguments]`。

**ソース：**

```suz
fn identity[T](value: T) -> T { return value }
fn main() -> u8 {
    fix value = identity[i32, i32](1)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.type-arguments]` | 3 | 17 |

### generic-006 構造体リテラルの型引数を省略できない

主根拠：`[generic.struct]`。

**ソース：**

```suz
struct Box[T] { value: T }
fn main() -> u8 {
    fix b = Box { value: 10 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.type-arguments]` | 3 | 17 |

### generic-007 具体的な実引数から型を推論する

主根拠：`[generic.inference]`。

**ソース：**

```suz
fn first[T](a: T, b: T) -> T { return a }
fn main() -> u8 {
    fix small: i8 = 2
    fix value = first(1, small)
    fix same: i8 = value
    if same != 1 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### generic-008 異なる具体型を1つの型引数へまとめない

主根拠：`[generic.inference]`。

**ソース：**

```suz
fn first[T](a: T, b: T) -> T { return a }
fn main() -> u8 {
    fix small: i8 = 1
    fix wide: i64 = 2
    fix value = first(small, wide)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.inference]` | 5 | 30 |

### generic-009 戻り値の要求型で実引数の推論を変えない

主根拠：`[generic.inference]`。

**ソース：**

```suz
fn first[T](a: T, b: T) -> T { return a }
fn main() -> u8 {
    fix value: i8 = first(1, 2)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 3 | 21 |

### generic-010 要求なしで型引数の加算を使えない

主根拠：`[generic.operations]`。

**ソース：**

```suz
fn sum[T](a: T, b: T) -> T { return a + b }
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.operations]` | 1 | 39 |

### generic-011 加算の要求を満たす型で呼ぶ

主根拠：`[generic.operations]`。

**ソース：**

```suz
@needs T: +(T) -> T
fn sum[T](a: T, b: T) -> T { return a + b }
fn main() -> u8 {
    if sum[i32](2, 3) != 5 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### generic-012 型引数のない関数に要求を付けない

主根拠：`[generic.needs]`。

**ソース：**

```suz
@needs T: +(T) -> T
fn sum(a: i32) -> i32 { return a }
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.needs]` | 1 | 8 |

### generic-013 要求と宣言の間を空行で離さない

主根拠：`[generic.needs-adjacent]`。

**ソース：**

```suz
@needs T: +(T) -> T

fn sum[T](a: T) -> T { return a }
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.needs]` | 2 | 1 |

### generic-014 加算のない構造体は要求を満たさない

主根拠：`[generic.needs-check]`。

**ソース：**

```suz
struct Value { n: i32 }
@needs T: +(T) -> T
fn sum[T](a: T, b: T) -> T { return a + b }
fn main() -> u8 {
    fix a = Value { n: 1 }
    fix b = sum[Value](a, a)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.needs-check]` | 6 | 13 |

### generic-015 汎用の整数式も中間値を切り詰めない

主根拠：`[generic.arithmetic]`。

**ソース：**

```suz
@needs T: *(T) -> T
@needs T: +(T) -> T
fn multiply_add[T](a: T, b: T, c: T) -> T { return a * b + c }
fn main() -> u8 {
    if multiply_add[i8](100, 2, -100) != 100 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### generic-016 名前付きの要求一覧を使う

主根拠：`[generic.needs-group]`。

**ソース：**

```suz
needs Ordered[T] {
    T: <(T) -> bool
    T: <=(T) -> bool
}
@needs Ordered[T]
fn in_range[T](value: T, low: T, high: T) -> bool { return low <= value && value < high }
fn main() -> u8 {
    if not in_range(2, 1, 3) || in_range(3, 1, 3) { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### generic-017 要求一覧を循環させない

主根拠：`[generic.needs-expand]`。

**ソース：**

```suz
needs Cycle[T] {
    Cycle[T]
}
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.needs-expand]` | 1 | 1 |

### generic-018 型引数の内側のnullableを暗黙変換しない

主根拠：`[generic.invariance]`。

**ソース：**

```suz
struct Box[T] { value: T }
fn use(b: Box[ptr? i32]) -> void {}
fn main() -> u8 {
    fix ptr p = new (1 as i32)
    fix b = Box[ptr i32] { value: p }
    use(b)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 6 | 9 |

### generic-019 名前付き型引数の関数値には具体型を書く

主根拠：`[generic.function-value]`。

**ソース：**

```suz
fn identity[T](value: T) -> T { return value }
fn main() -> u8 {
    fix f = fn identity
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.type-arguments]` | 3 | 13 |

### generic-020 型引数を持つfnshapeで関数を受け取る

主根拠：`[generic.fnshape]`。

**ソース：**

```suz
fn identity[T](value: T) -> T { return value }
fnshape Identity[T](T) -> T
fn apply[T](f: fn Identity[T], value: T) -> T { return f(value) }
fn main() -> u8 {
    if apply[i32](fn identity[i32], 4) != 4 { return 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### generic-021 名前付き型引数と匿名汎用を混ぜない

主根拠：`[generic.numeric-existing]`。

**ソース：**

```suz
fn mix[T](value: T, number: int) -> T { return value }
fn main() -> u8 {

    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.numeric-existing]` | 1 | 32 |

### generic-022 型引数が増え続ける再帰を拒否する

主根拠：`[generic.recursion]`。

**ソース：**

```suz
struct Box[T] { value: T }
fn repeat[T](value: T) -> void { repeat[Box[T]](Box[T] { value: value }) }
fn main() -> u8 {
    repeat[i32](1)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[generic.check]` | 4 | 5 |
