# 配列（一括書き込み・演算・マスク）

[目次と書式](../spec-cases.md)

### array-071 1次元配列の一括書き込み・切り出し・複合代入・要素演算と重なり
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.write]`・`[expr.as-sat]`・`[expr.arithmetic-handler]`
- 結果：正常終了

```suz
fn mix(out: #[f32; n], a: .[f32; n], b: .[f32; n], k: len_t) -> void {
    out#[] = a * 0.5 + b
    if k <= n {
        out#[0:k] += a.[0:k]
    }
}

fn main() -> u8 {
    fix out: #[f32] = new #[0.0; 4]
    fix a: .[f32] = .[2.0, 4.0, 6.0, 8.0]
    fix b: .[f32] = .[1.0, 1.0, 1.0, 1.0]
    mix(out, a, b, 2)
    if out.[0] != 4.0 || out.[1] != 7.0 || out.[2] != 4.0 || out.[3] != 5.0 { return 1 }

    // 飽和変換とスカラー埋め
    fix pcm = new #[0 as i16; 3]
    fix wave: .[f32] = .[-2.0, 0.5, 2.0]
    pcm#[] = (wave * 32767.0) as| i16 or 0
    if pcm.[0] != -32768 || pcm.[1] != 16383 || pcm.[2] != 32767 { return 2 }
    out#[] = 0.0
    if out.[0] != 0.0 || out.[3] != 0.0 { return 3 }

    // 完全一致とずれた重なり（添字順のループと同じ意味）
    fix s = new #[1, 2, 3, 4]
    s#[] = s * 2
    if s.[0] != 2 || s.[1] != 4 || s.[2] != 6 || s.[3] != 8 { return 4 }
    fix shift = new #[1, 2, 3, 4]
    shift#[1:4] = shift.[0:3]
    if shift.[0] != 1 || shift.[1] != 1 || shift.[2] != 1 || shift.[3] != 1 { return 5 }

    // 要素ごとの or 代替値
    fix num = new #[10, 20, 30]
    fix den = new #[2, 0, 5]
    fix quo = new #[0; 3]
    quo#[] = num / den or -1
    if quo.[0] != 5 || quo.[1] != -1 || quo.[2] != 6 { return 6 }
    return 0
}
```

### array-082 列挙型添字配列の一括書き込み・複合代入・マスク付き書き込み
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.enum-type]`・`[array.enum-index]`
- 結果：正常終了

```suz
enum Wave: u8 { Saw, Square, Triangle }

fn mix_waves(out: #[f32; Wave], a: .[f32; Wave], b: .[f32; Wave]) -> void {
    out#[] = a * 0.5 + b
    out#[a > 2.0] += 10.0
}

fn main() -> u8 {
    fix out: #[f32; Wave] = new #[0.0; Wave]
    fix a: .[f32; Wave] = .[Wave.Saw: 2.0, Wave.Square: 4.0, Wave.Triangle: 6.0]
    fix b: .[f32; Wave] = .[Wave.Saw: 1.0, Wave.Square: 1.0, Wave.Triangle: 1.0]
    mix_waves(out, a, b)
    if out.[Wave.Saw] != 2.0 || out.[Wave.Square] != 13.0 || out.[Wave.Triangle] != 14.0 { return 1 }
    return 0
}
```

### array-083 列挙型添字配列と整数添字配列を一括書き込みに混在できない
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.enum-type]`
- 結果：コンパイルエラー `[array.bulk]` 6:14

```suz
enum Wave: u8 { Saw, Square }

fn main() -> u8 {
    fix out: #[f32; Wave] = new #[0.0; Wave]
    fix xs: .[f32] = .[1.0, 2.0]
    out#[] = xs
    return 0
}
```

### array-086 異なる列挙型添字配列同士を一括書き込みに混在できない
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.enum-type]`
- 結果：コンパイルエラー `[array.bulk]` 7:14

```suz
enum Wave: u8 { Saw, Square }
enum Mode: u8 { Fast, Slow }

fn main() -> u8 {
    fix out: #[f32; Wave] = new #[0.0; Wave]
    fix src: .[f32; Mode] = .[Mode.Fast: 1.0, Mode.Slow: 2.0]
    out#[] = src
    return 0
}
```

### array-087 列挙型添字配列への切り出し一括書き込みは拒否される
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.enum-index]`
- 結果：コンパイルエラー `[array.enum-index]` 5:15

```suz
enum Wave: u8 { Saw, Square }

fn main() -> u8 {
    fix out: #[f32; Wave] = new #[0.0; Wave]
    out#[0:1] = 1.0
    return 0
}
```

### array-088 nil許容の配列参照への一括書き込みは拒否される
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.value]`
- 結果：コンパイルエラー `[array.value]` 3:5

```suz
fn main() -> u8 {
    fix out: #[i32]? = new #[0; 2]
    out#[] = 1
    return 0
}
```

### array-089 一括書き込みで追記演算子<-は使えない
- 主根拠：`[array.bulk]`
- 補助根拠：`[array.write]`
- 結果：コンパイルエラー `[array.bulk]` 3:12

```suz
fn main() -> u8 {
    fix out = new #[0; 2]
    out#[] <- 1
    return 0
}
```

### array-079 構造体要素の配列への演算子メソッド適用と添字順の呼び出し順序
- 主根拠：`[array.bulk-ops]`
- 補助根拠：`[operator.decl]`・`[operator.arith-boundary]`
- 結果：正常終了

```suz
struct Vec2 { x: f32, y: f32 }

fn Vec2.+(other: Vec2) -> Vec2 {
    log info "add ${self.x}"
    return Vec2 { x: self.x + other.x, y: self.y + other.y }
}

fn Vec2.*(scale: f32) -> Vec2 {
    return Vec2 { x: self.x * scale, y: self.y * scale }
}

fn main() -> u8 {
    fix ps = new #[Vec2 { x: 1.0, y: 2.0 }, Vec2 { x: 3.0, y: 4.0 }]
    fix vs = new #[Vec2 { x: 10.0, y: 20.0 }, Vec2 { x: 30.0, y: 40.0 }]
    ps#[] = ps + vs * 0.5
    if ps.[0].x != 6.0 || ps.[0].y != 12.0 || ps.[1].x != 18.0 || ps.[1].y != 24.0 { return 1 }
    ps#[] += vs
    if ps.[0].x != 16.0 || ps.[1].x != 48.0 { return 2 }
    return 0
}
```

**標準エラー：**

```text
info array-079.suz:4: add 1.0
info array-079.suz:4: add 3.0
info array-079.suz:4: add 6.0
info array-079.suz:4: add 18.0
```

### array-081 演算子メソッドの定義がない構造体要素の配列の一括演算は拒否される
- 主根拠：`[array.bulk-ops]`
- 補助根拠：`[operator.overload]`
- 結果：コンパイルエラー `[operator.overload]` 7:16

```suz
struct Plain { v: i32 }

fn main() -> u8 {
    fix a = new #[Plain { v: 1 }; 2]
    fix b = new #[Plain { v: 2 }; 2]
    fix out = new #[Plain { v: 0 }; 2]
    out#[] = a + b
    return 0
}
```

### array-091 or付きリテラル式と飽和演算を組み合わせた一括書き込み
- 主根拠：`[array.bulk-ops]`
- 補助根拠：`[array.bulk-handler]`・`[expr.sat-arith]`
- 結果：正常終了

```suz
fn apply(out: #[i32; n], c: .[i32; n]) -> void {
    out#[] = (3 * 5 or 1) +| c
    out#[] = c +| (3 * 2147483647 or 1)
}

fn main() -> u8 {
    fix out: #[i32] = new #[0; 2]
    fix c: .[i32] = .[10, 20]
    apply(out, c)
    if out.[0] != 11 || out.[1] != 21 { return 1 }
    return 0
}
```

### array-076 マスク付き一括書き込みとマスク付き複合代入は偽の位置を評価・更新しない
- 主根拠：`[array.bulk-mask]`
- 補助根拠：`[array.slice]`・`[expr.arithmetic-handler]`
- 結果：正常終了

```suz
fn fallback(calls: mut i64) -> i64 {
    #calls = .calls + 1
    return -1
}

fn clear_positive(out: #[f32], a: .[f32], s: len_t, e: len_t) -> void {
    fix part = out.[s:e] or return
    fix input = a.[s:e] or return
    part#[input > 0.0] = 0.0
}

fn main() -> u8 {
    // 偽の位置では除算を実行せず out を保持する
    fix a = new #[10, 20, 30, 40]
    fix b = new #[2, 0, 5, 0]
    fix out = new #[99, 99, 99, 99]
    out#[b != 0] = a / b
    if out.[0] != 5 || out.[1] != 99 || out.[2] != 6 || out.[3] != 99 { return 1 }

    // 偽の位置では or 代替値を評価しない
    fix calls = new 0
    fix c = new #[1, 0, 1]
    fix d = new #[0, 0, 2]
    fix r = new #[7, 7, 7]
    r#[c != 0] = a.[0:3] / d or fallback(calls)
    if r.[0] != -1 || r.[1] != 7 || r.[2] != 15 || .calls != 1 { return 2 }

    // 比較連鎖と論理演算 & | ^ ~ とマスク付き複合代入
    fix x: #[f32] = new #[0.5, 1.5, -0.5, 0.25]
    fix y: .[f32] = .[0.2, 0.2, 0.2, 0.8]
    x#[0.0 < x < 1.0] += 10.0
    if x.[0] != 10.5 || x.[1] != 1.5 || x.[2] != -0.5 || x.[3] != 10.25 { return 3 }
    x#[(x > 5.0) & ~(y > 0.5)] = 1.0
    if x.[0] != 1.0 || x.[3] != 10.25 { return 4 }

    // 切り出しを束縛してからのマスク付き書き込み
    fix buf: #[f32] = new #[1.0, -2.0, 3.0, 4.0]
    fix ref_in: .[f32] = .[1.0, -2.0, 3.0, 4.0]
    clear_positive(buf, ref_in, 1, 4)
    if buf.[0] != 1.0 || buf.[1] != -2.0 || buf.[2] != 0.0 || buf.[3] != 0.0 { return 5 }
    return 0
}
```

### array-077 マスク条件の位置以外では配列参照の比較はコンパイルエラーになる
- 主根拠：`[array.bulk-mask]`
- 補助根拠：`[array.value]`・`[expr.equality]`
- 結果：コンパイルエラー `[array.value]` 4:16

```suz
fn main() -> u8 {
    fix a = new #[1, 2]
    fix b = new #[1, 2]
    fix eq = a == b
    return 0
}
```

### array-109 マスク条件にnotを含む配列比較はarray.valueで拒否される
- 主根拠：`[array.bulk-mask]`
- 補助根拠：`[array.value]`
- 結果：コンパイルエラー `[array.value]` 4:17

```suz
fn main() -> u8 {
    fix a = new #[1, 2]
    fix out = new #[0; 2]
    out#[not (a > 1)] = 1
    return 0
}
```

### array-078 切り出しとマスクを直接連結した書き込みはコンパイルエラーになる
- 主根拠：`[array.bulk-mask]`
- 結果：コンパイルエラー `[array.bulk-mask]` 4:8

```suz
fn main() -> u8 {
    fix out = new #[1, 2, 3, 4]
    fix a = new #[1, 0, 1]
    out.[0:3]#[a > 0] = 0
    return 0
}
```

### array-080 演算子メソッドが途中で失敗すると失敗より前の呼び出し順だけが観測される
- 主根拠：`[array.bulk-eval]`
- 補助根拠：`[operator.arith-boundary]`・`[rt.div-zero]`
- 結果：パニック `[rt.div-zero]` 5:29

```suz
struct Item { v: i64 }

fn Item./(other: Item) -> Item {
    log info "div ${self.v}/${other.v}"
    return Item { v: self.v / other.v }
}

fn main() -> u8 {
    fix a = new #[Item { v: 10 }, Item { v: 20 }, Item { v: 30 }]
    fix b = new #[Item { v: 2 }, Item { v: 0 }, Item { v: 5 }]
    fix out = new #[Item { v: 0 }; 3]
    out#[] = a / b
    return 0
}
```

**パニック前の標準エラー：**

```text
info array-080.suz:4: div 10/2
info array-080.suz:4: div 20/0
```

### array-084 ブロードキャストスカラーの1回評価とマスク内の比較連鎖の短絡
- 主根拠：`[array.bulk-eval]`
- 補助根拠：`[array.bulk-mask]`
- 結果：正常終了

```suz
fn count(n: mut i64) -> i64 {
    #n = .n + 1
    return 10
}

fn main() -> u8 {
    // 長さ0の配列でもブロードキャストスカラーは1回評価される
    fix c1 = new 0
    fix empty = new #[0; 0]
    empty#[] = count(c1)
    if .c1 != 1 { return 1 }

    // 全位置でマスクが偽でもブロードキャストスカラーは1回評価される
    fix c2 = new 0
    fix out = new #[1, 2]
    fix m = new #[0, 0]
    out#[m > 0] = count(c2)
    if .c2 != 1 || out.[0] != 1 || out.[1] != 2 { return 2 }

    // マスク内の比較連鎖は左が偽なら右の比較を短絡する
    fix a = new #[0, 5]
    fix b = new #[0, 2]
    fix dst = new #[0, 0]
    dst#[0 < a < (10 / b)] = 1
    if dst.[0] != 0 || dst.[1] != 0 { return 3 }
    return 0
}
```

### array-085 一括書き込みの複合代入はスカラー評価後に各位置の現在値を読む
- 主根拠：`[array.bulk-eval]`
- 補助根拠：`[assign.eval-order]`・`[array.write]`
- 結果：正常終了

```suz
fn bump(x: #[i64]) -> i64 {
    x#[0] = 100
    return 5
}

fn main() -> u8 {
    fix out = new #[1, 2, 3]
    fix alias = out
    out#[] += bump(alias)
    if out.[0] != 105 || out.[1] != 7 || out.[2] != 8 { return 1 }
    return 0
}
```

### array-072 一括書き込みの長さ不一致は要素へ書き込む前にパニックする
- 主根拠：`[array.bulk-bounds]`
- 補助根拠：`[rt.array-bounds]`
- 結果：パニック `[rt.array-bounds]` 4:14

```suz
fn main() -> u8 {
    fix out = new #[0; 3]
    fix src = new #[1, 2]
    out#[] = src
    return 0
}
```

### array-073 一括書き込みの左辺切り出しの範囲外は要素へ書き込む前にパニックする
- 主根拠：`[array.bulk-bounds]`
- 補助根拠：`[rt.array-bounds]`
- 結果：パニック `[rt.array-bounds]` 4:15

```suz
fn main() -> u8 {
    fix out = new #[0; 3]
    fix src = new #[1, 2, 3, 4]
    out#[1:5] = src
    return 0
}
```

### array-074 一括書き込みの要素式で失敗しうるasにorを省略できない
- 主根拠：`[array.bulk-handler]`
- 補助根拠：`[expr.cast-handler]`
- 結果：コンパイルエラー `[expr.cast-handler]` 4:18

```suz
fn main() -> u8 {
    fix out = new #[0 as u8; 2]
    fix src = new #[10, 20]
    out#[] = src as u8
    return 0
}
```

### array-075 一括書き込みの要素式でor returnは使えない
- 主根拠：`[array.bulk-handler]`
- 結果：コンパイルエラー `[array.bulk-handler]` 5:23

```suz
fn main() -> u8 {
    fix out = new #[0; 2]
    fix a = new #[10, 20]
    fix b = new #[2, 0]
    out#[] = a / b or return 1
    return 0
}
```

### array-090 一括書き込みの要素算術に対する診断設定の比較
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[array.bulk-handler]`
- 結果（通常）：正常終了
- 結果（警告）：正常終了 / 警告：`[diagnostic.arithmetic]` 4:16
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 4:16

```suz
fn main() -> u8 {
    fix a = new #[1, 2]
    fix out = new #[0; 2]
    out#[] = a + 1
    if out.[0] != 2 || out.[1] != 3 { return 1 }
    return 0
}
```
