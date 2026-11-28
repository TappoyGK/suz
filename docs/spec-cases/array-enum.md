# 仕様例：配列（列挙型添字配列）

### array-002 列挙した行の長さが一致しない
- 主根拠：`[array.enumeration]`
- 結果：コンパイルエラー `[array.enumeration]` 2:18

```suz
fn main() -> u8 {
    fix a = new #[[1, 2], [3]]
    return 0
}
```

### array-040 要素として書いた定数配列は行ではなく値として扱う
- 主根拠：`[array.enumeration]`
- 補助根拠：`[array.constant]`
- 結果：正常終了

```suz
fix JAGGED: .[.[i32]] = .[.[1, 2], .[3]]
fn main() -> u8 {
    arena {
        fix a: #[.[i32]] = new #[.[1, 2], .[3, 4]]
        if (a.[0].[1] or 0) != 2 || len JAGGED.[0] != 2 || len JAGGED.[1] != 1 { return 1 }
    }
    return 0
}
```

### array-041 配列初期化の同じ段に値と行を混在させない
- 主根拠：`[array.enumeration]`
- 結果：コンパイルエラー `[array.enumeration]` 2:18

```suz
fn main() -> u8 {
    fix a = new #[.[1, 2], [3, 4]]
    return 0
}
```

### array-070 多次元配列の内側の行に列挙子キーを書けない
- 主根拠：`[array.enumeration]`
- 補助根拠：`[array.enum-type]`、`[array.enum-init]`
- 結果：コンパイルエラー `[array.enumeration]` 3:16

```suz
enum Wave: u8 { Saw, Square }
fn main() -> u8 {
    fix a = .[[Wave.Square: 10, Wave.Saw: 20]]
    return 0
}
```

### array-057 列挙型を添字にする定数関数表と配列参照の読み書き
- 主根拠：`[array.enum-type]`
- 補助根拠：`[array.enum-len]`、`[array.enum-init]`、`[array.enum-index]`、`[array.enum-dispatch]`、`[array.constant]`、`[array.readonly-conversion]`
- 結果：正常終了

```suz
enum Wave: u8 {
    Saw,
    Square,
    Triangle,
}

fnshape Shape(f32) -> f32

fn saw(p: f32) -> f32 { return p * 2.0 - 1.0 }
fn square(p: f32) -> f32 {
    if p < 0.5 { return 1.0 }
    return -1.0
}
fn triangle(p: f32) -> f32 {
    if p < 0.5 { return p * 4.0 - 1.0 }
    return 3.0 - p * 4.0
}

fix SHAPES: .[static fn Shape; Wave] = .[
    Wave.Saw: fn saw,
    Wave.Square: fn square,
    Wave.Triangle: fn triangle,
]

fn render(shapes: .[static fn Shape; Wave], levels: .[f32; Wave], wave: Wave, phase: f32) -> f32 {
    return shapes.[wave](phase) * levels.[wave]
}

fn main() -> u8 {
    fix levels: #[f32; Wave] = new #[0.25; Wave]
    levels#[Wave.Square] = 0.5
    fix wave = 1 as Wave or return 1
    fix v = render(SHAPES, levels, wave, 0.25)
    if len SHAPES != 3 || len levels != 3 { return 2 }
    if v != 0.5 { return 3 }
    return 0
}
```

### array-058 列挙型名のつづり間違いを配列型の宣言位置で拒否する
- 主根拠：`[array.enum-dimension]`
- 結果：コンパイルエラー `[array.enum-dimension]` 2:21

```suz
enum Wave: u8 { Saw, Square }
fn render(a: #[f32; Wav]) -> void {}
fn main() -> u8 { return 0 }
```

### array-059 列挙型添字配列と通常の配列は相互に変換できない
- 主根拠：`[array.enum-type]`
- 補助根拠：`[array.type]`、`[array.readonly-conversion]`
- 結果：コンパイルエラー `[type.expected]` 5:10

```suz
enum Wave: u8 { Saw, Square }
fn take(a: .[i32]) -> void {}
fn main() -> u8 {
    fix a: #[i32; Wave] = new #[0; Wave]
    take(a)
    return 0
}
```

### array-060 列挙型添字配列の初期化で列挙子を欠落させない
- 主根拠：`[array.enum-init]`
- 結果：コンパイルエラー `[array.enum-init]` 3:18

```suz
enum Wave: u8 { Saw, Square, Triangle }
fn main() -> u8 {
    fix a = new #[Wave.Saw: 1, Wave.Square: 2]
    return 0
}
```

### array-061 列挙型添字配列の初期化で列挙子を重複させない
- 主根拠：`[array.enum-init]`
- 結果：コンパイルエラー `[array.enum-init]` 3:28

```suz
enum Wave: u8 { Saw, Square }
fn main() -> u8 {
    fix a = .[Wave.Saw: 1, Wave.Saw: 2]
    return 0
}
```

### array-062 要求型があってもキーなしの初期化を列挙型添字配列にしない
- 主根拠：`[array.enum-init]`
- 補助根拠：`[array.initializer-type]`
- 結果：コンパイルエラー `[array.enum-init]` 3:28

```suz
enum Wave: u8 { Saw, Square, Triangle }
fn main() -> u8 {
    fix a: .[f32; Wave] = .[1.0, 2.0, 3.0]
    return 0
}
```

### array-063 記述順で評価し列挙子の宣言順で格納・反復する
- 主根拠：`[array.enum-init-order]`
- 補助根拠：`[array.enum-iteration]`
- 結果：正常終了

```suz
enum Wave: u8 { Saw, Square, Triangle }
fn emit(v: i32) -> i32 {
    log info "eval=${v}"
    return v
}
fn main() -> u8 {
    fix a = new #[
        Wave.Triangle: emit(30),
        Wave.Saw: emit(10),
        Wave.Square: emit(20),
    ]
    for x in a { log info "for=${x}" }
    rev x in a { log info "rev=${x}" }
    return 0
}
```

**標準エラー：**

```text
info array-063.suz:3: eval=30
info array-063.suz:3: eval=10
info array-063.suz:3: eval=20
info array-063.suz:12: for=10
info array-063.suz:12: for=20
info array-063.suz:12: for=30
info array-063.suz:13: rev=30
info array-063.suz:13: rev=20
info array-063.suz:13: rev=10
```

### array-064 列挙型添字配列に整数を添字として使えない
- 主根拠：`[array.enum-index]`
- 補助根拠：`[array.enum-len]`
- 結果：コンパイルエラー `[array.enum-index]` 5:20

```suz
enum Wave: u8 { Saw, Square }
fn main() -> u8 {
    fix a = .[10; Wave]
    for i in 0..<len a {
        fix v = a.[i] or 0
    }
    return 0
}
```

### array-065 列挙型添字配列を切り出せない
- 主根拠：`[array.enum-index]`
- 結果：コンパイルエラー `[array.enum-index]` 4:14

```suz
enum Wave: u8 { Saw, Square }
fn main() -> u8 {
    fix a = .[0; Wave]
    fix s = a.[0:1] or a
    return 0
}
```

### array-066 列挙型添字への不要なorは警告し連続アクセスの必要なorは警告しない
- 主根拠：`[array.enum-index]`
- 補助根拠：`[array.access-handler]`、`[array.redundant-handler]`
- 結果：正常終了 / 警告：`[array.redundant-handler]` 7:26

```suz
enum Wave: u8 { Saw, Square }
fn sample(rows: .[.[i32]; Wave], w: Wave, i: len_t) -> i32 {
    return rows.[w].[i] or 0
}
fn main() -> u8 {
    fix a: .[i32; Wave] = .[Wave.Saw: 1, Wave.Square: 2]
    fix v = a.[Wave.Saw] or 0
    fix rows: .[.[i32]; Wave] = .[Wave.Saw: .[10, 20], Wave.Square: .[30]]
    if v != 1 || sample(rows, Wave.Saw, 1) != 20 || sample(rows, Wave.Square, 5) != 0 { return 1 }
    return 0
}
```

### array-067 列挙型添字配列の反復中に後の要素を書き換えると後の反復に反映される
- 主根拠：`[array.enum-iteration]`
- 補助根拠：`[array.iteration]`
- 結果：正常終了

```suz
enum Wave: u8 { Saw, Square, Triangle }
fn main() -> u8 {
    fix a: #[i32; Wave] = new #[Wave.Saw: 1, Wave.Square: 2, Wave.Triangle: 3]
    var sum: i32 = 0
    for x in a {
        sum = sum + x or return 1
        if x == 1 { a#[Wave.Triangle] = 30 }
    }
    if sum != 33 { return 2 }
    return 0
}
```

### array-112 列挙型添字配列の2変数反復
- 主根拠：`[array.enum-iteration]`
- 補助根拠：`[array.iteration]`
- 結果：正常終了

```suz
enum Wave: u8 { Sine, Saw, Square }

fn main() -> u8 {
    fix gains: #[i32; Wave] = new #[Wave.Sine: 1, Wave.Saw: 2, Wave.Square: 3]
    for w, g in gains {
        gains#[w] = g * 10 or return 1
    }
    if gains.[Wave.Sine] != 10 || gains.[Wave.Saw] != 20 || gains.[Wave.Square] != 30 { return 2 }
    var rev_sum: i32 = 0
    rev w, g in gains {
        rev_sum = rev_sum * 100 + g or return 3
        if w == Wave.Saw { gains#[Wave.Sine] = 99 }
    }
    if rev_sum != 302099 { return 4 }
    return 0
}
```

### array-068 関数表の呼び出しは引数評価前に関数値を取得する
- 主根拠：`[array.enum-dispatch]`
- 補助根拠：`[fn.value-evaluation]`
- 結果：正常終了

```suz
enum Wave: u8 { Saw, Square }
fnshape Step(i32) -> i32
fn first(x: i32) -> i32 { return x + 1 or 0 }
fn second(x: i32) -> i32 { return x + 100 or 0 }
fn swap(t: #[static fn Step; Wave]) -> i32 {
    t#[Wave.Saw] = fn second
    return 5
}
fn main() -> u8 {
    fix t: #[static fn Step; Wave] = new #[Wave.Saw: fn first, Wave.Square: fn second]
    fix r1 = t.[Wave.Saw](swap(t))
    fix r2 = t.[Wave.Saw](5)
    if r1 != 6 || r2 != 105 { return 1 }
    return 0
}
```
