# 配列（列挙型添字配列）

[目次と書式](../spec-cases.md)

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

### array-113 多次元列挙型添字配列の型・寸法名の混在・空の次元・SoA列取り出し
- 主根拠：`[array.enum-type]`
- 補助根拠：`[array.dimension-equality]`、`[array.enum-len]`、`[array.enum-iteration]`、`[array.soa-column]`
- 結果：正常終了

```suz
enum Wave: u8 { Sine, Saw }
enum Ch: u8 { Left, Right }
struct Sample { amp: i32, pan: i32 }

fn sum_wave(a: .[i32; Wave, n], out: #[i32; Wave, n]) -> void {
    for w, row in a {
        for i in 0..<n {
            out.[w]#[i] = row.[i] + 1 or 0
        }
    }
}

fn main() -> u8 {
    fix per_wave: #[i32; Wave, _] = new #[
        Wave.Sine: [10, 20, 30],
        Wave.Saw:  [40, 50, 60],
    ]
    fix out: #[i32; Wave, _] = new #[0; Wave, 3]
    sum_wave(per_wave, out)
    if len out != 2 || len out.[Wave.Sine] != 3 { return 1 }
    if out.[Wave.Saw].[2] != 61 { return 2 }

    fix gains: .[i32; Wave, Ch] = .[
        Wave.Saw:  [Ch.Right: 4, Ch.Left: 3],
        Wave.Sine: [Ch.Left: 1, Ch.Right: 2],
    ]
    if gains.[Wave.Sine].[Ch.Right] != 2 || gains.[Wave.Saw].[Ch.Left] != 3 { return 3 }

    fix s: soa #[Sample; Wave, _] = new soa #[Sample { amp: 5, pan: -1 }; Wave, 2]
    fix amps: #[i32; Wave, _] = s.amp
    amps.[Wave.Saw]#[1] = 9
    if s.[Wave.Saw].[1].amp != 9 { return 4 }

    fix empty_inner = new #[0; Ch, 0]
    var inner_count: len_t = 0
    for ch, row in empty_inner {
        if len row != 0 { return 5 }
        inner_count = inner_count + 1 or return 6
    }
    if len empty_inner != 2 || inner_count != 2 { return 7 }

    fix empty_outer = new #[0; 0, Ch]
    var outer_count: len_t = 0
    for row in empty_outer {
        outer_count = outer_count + 1 or return 8
    }
    if len empty_outer != 0 || outer_count != 0 { return 9 }
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

### array-117 多次元列挙型添字の定数反復初期化と行・定数配列要素の区別
- 主根拠：`[array.enum-init]`
- 補助根拠：`[array.enumeration]`、`[array.constant]`
- 結果：正常終了

```suz
enum Wave: u8 { Sine, Saw }
enum Ch: u8 { Left, Right }

fix ZERO: .[i32; Ch, _] = .[0; Ch, 3]

fn main() -> u8 {
    if len ZERO != 2 || len ZERO.[Ch.Left] != 3 { return 1 }
    if ZERO.[Ch.Right].[2] != 0 { return 2 }

    fix table: #[.[i32]; Wave] = new #[Wave.Sine: .[1, 2], Wave.Saw: .[3]]
    if len table.[Wave.Sine] != 2 || len table.[Wave.Saw] != 1 { return 3 }
    if (table.[Wave.Sine].[1] or 0) != 2 || (table.[Wave.Saw].[0] or 0) != 3 { return 4 }

    fix grid: #[i32; Wave, _] = new #[Wave.Sine: [1, 2], Wave.Saw: [3, 4]]
    if grid.[Wave.Saw].[1] != 4 { return 5 }
    return 0
}
```

### array-116 多次元列挙型添字の要求型にキーなし列挙初期化は使えない
- 主根拠：`[array.enum-init]`
- 補助根拠：`[array.initializer-type]`
- 結果：コンパイルエラー `[array.enum-init]` 4:33

```suz
enum Ch: u8 { Left, Right }

fn main() -> u8 {
    fix a: #[f64; _, Ch] = new #[[0.0, 0.0]]
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

### array-115 多次元列挙型添字配列の初期化評価順とlenの副作用維持
- 主根拠：`[array.enum-init-order]`
- 補助根拠：`[array.enum-init]`、`[array.enum-len]`
- 結果：正常終了

```suz
enum Wave: u8 { Sine, Saw }
enum Ch: u8 { Left, Right }

fn step(n: mut i32) -> i32 {
    #n = .n + 1 or 0
    return .n
}

fn make_zero_len(n: mut i32) -> len_t {
    #n = .n + 10 or 0
    return 0
}

fn touch(a: .[i32; Wave, Ch], n: mut i32) -> .[i32; Wave, Ch] {
    #n = .n + 100 or 0
    return a
}

fn main() -> u8 {
    fix c = new (0 as i32)
    fix m = new #[
        Wave.Saw:  [Ch.Right: step(c), Ch.Left: step(c)],
        Wave.Sine: [Ch.Left: step(c), Ch.Right: step(c)],
    ]
    if m.[Wave.Saw].[Ch.Right] != 1 || m.[Wave.Saw].[Ch.Left] != 2 { return 1 }
    if m.[Wave.Sine].[Ch.Left] != 3 || m.[Wave.Sine].[Ch.Right] != 4 { return 2 }

    fix z = new #[step(c); Ch, make_zero_len(c)]
    if .c != 15 || len z != 2 { return 3 }

    fix n = len touch(m, c)
    if n != 2 || .c != 115 { return 4 }
    return 0
}
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

### array-118 先頭が長さの次元で内側が列挙型の配列の切り出しと添字の失敗処理
- 主根拠：`[array.enum-index]`
- 補助根拠：`[array.slice]`、`[array.access-handler]`
- 結果：正常終了

```suz
enum Ch: u8 { Left, Right }

fn head_frames(buf: #[i32; _, Ch], n: len_t) -> #[i32; _, Ch] {
    if n <= len buf {
        return buf.[0:n]
    }
    return buf.[:]
}

fn frame_len(buf: .[i32; _, Ch], i: len_t) -> len_t {
    fix row = buf.[i] or return 0
    return len row
}

fn read_right(buf: .[i32; _, Ch], i: len_t) -> i32 {
    return buf.[i].[Ch.Right] or 9
}

fn main() -> u8 {
    fix stereo: #[i32; _, Ch] = new #[
        [Ch.Right: 2, Ch.Left: 1],
        [Ch.Left: 3, Ch.Right: 4],
    ]
    fix head = head_frames(stereo, 1)
    if len head != 1 || head.[0].[Ch.Left] != 1 || head.[0].[Ch.Right] != 2 { return 1 }
    if frame_len(stereo, 1) != 2 || frame_len(stereo, 5) != 0 { return 2 }
    if read_right(stereo, 1) != 4 || read_right(stereo, 5) != 9 { return 3 }
    return 0
}
```

### array-114 先頭次元が列挙型の多次元配列は切り出せない
- 主根拠：`[array.enum-index]`
- 補助根拠：`[array.slice]`
- 結果：コンパイルエラー `[array.enum-index]` 4:18

```suz
enum Ch: u8 { Left, Right }

fn bad(planes: #[f32; Ch, _]) -> #[f32; Ch, _] {
    return planes.[0:1]
}

fn main() -> u8 {
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
