# 6. 定数・変数・代入

[目次と書式](../spec-cases.md)

### bind-001 定数の構造体フィールドへ代入する

主根拠：`[const.decl]`。

補助根拠：`[assign.target]`。

**ソース：**

```suz
struct Point { x: i32 }
const ORIGIN = Point { x: 1 }

fn main() -> u8 {
    ORIGIN.x = 2
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

構造体を定数として宣言した場合、そのフィールドも変更できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.decl]` | 5 | 5 |

### bind-002 定数が互いを循環参照する

主根拠：`[const.initializer]`。

**ソース：**

```suz
const FIRST: u8 = SECOND
const SECOND: u8 = FIRST

fn main() -> u8 {
    return FIRST
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

前方参照は可能だが、firstとsecondの初期値は循環している。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.initializer]` | 2 | 20 |

### bind-003 構造体の定数とフィールド参照を定数式に使う

主根拠：`[const.expression]`。

補助根拠：`[const.cast-check]`。

**ソース：**

```suz
struct Point { x: i32, y: i32 }
const ORIGIN = Point { x: 1, y: 2 }
const TOTAL = ORIGIN.x + ORIGIN.y

fn main() -> u8 {
    return TOTAL as u8
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

originのフィールドを参照した加算も定数式になる。結果がu8に収まるので、変換のorを省略できる。

### bind-004 短絡で評価しない関数呼び出しも定数式には使えない

主根拠：`[const.expression]`。

補助根拠：`[const.evaluation]`。

**ソース：**

```suz
fn ready() -> bool {
    return true
}

const RESULT = true || ready()

fn main() -> u8 {
    return ifexpr RESULT { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

短絡で右辺を評価しなくても、定数式の構成を検査する。関数呼び出しは定数式に含まれない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.expression]` | 5 | 24 |

### bind-005 exprブロックを定数の初期値にする

主根拠：`[const.expression]`。

補助根拠：`[const.initializer]`。

**ソース：**

```suz
const RESULT: u8 = expr { 3 }

fn main() -> u8 {
    return RESULT
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

内容がリテラルだけでも、exprブロックは定数式の構成要素ではない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.expression]` | 1 | 20 |

### bind-006 定数式の成功時は代替値を評価しない

主根拠：`[const.evaluation]`。

**ソース：**

```suz
const RESULT: u8 = (1 + 2) or (1 / (0 as u8))

fn main() -> u8 {
    return RESULT
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

代替値の型は検査するが、ゼロ除算は評価しない。除数はu8型の値として範囲を見積もるため、必ずゼロで割る式とは判定しない。

### bind-007 定数式の演算失敗時は代替値を評価

主根拠：`[const.evaluation]`。

補助根拠：`[const.expression]`。

**ソース：**

```suz
const RESULT: u8 = (255 + 1) or 7

fn main() -> u8 {
    return RESULT
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

### bind-008 constのオーバーフロー

主根拠：`[const.evaluation]`。

**ソース：**

```suz
const VALUE: u8 = 255 + 1
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

コンパイル時の加算が範囲外で、orがない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.evaluation]` | 1 | 23 |

### bind-009 実行しない代替値にも型検査を適用

主根拠：`[const.evaluation]`。

補助根拠：`[expr.arithmetic-fallback]`。

**ソース：**

```suz
const VALUE: u8 = (1 + 2) or false
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

加算は成功するが、代替値のboolはu8と一致しない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 1 | 30 |

### bind-010 定数変換の失敗

主根拠：`[const.cast-check]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = 300 as u8
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

変換失敗がコンパイル時に分かり、orがない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.cast-check]` | 2 | 21 |

### bind-011 短絡で評価しない定数変換にも失敗処理を書く

主根拠：`[const.cast-check]`。

補助根拠：`[const.evaluation]`。

**ソース：**

```suz
const RESULT = true || ((300 as u8 or 0) == 0)

fn main() -> u8 {
    return ifexpr RESULT { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

右辺のキャストは`or`の必須検査を満たす。論理和の左辺がtrueなので、右辺は評価しない。

### bind-012 短絡で評価しない失敗する定数変換

主根拠：`[const.cast-check]`。

補助根拠：`[const.evaluation]`。

**ソース：**

```suz
const RESULT = true || ((300 as u8) == 0)

fn main() -> u8 {
    return ifexpr RESULT { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

右辺を評価しなくても、失敗する定数キャストには`or`が必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[const.cast-check]` | 1 | 30 |

### bind-013 varの初期値を省略する

主根拠：`[var.initializer]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8
    count = 1
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

後続に代入があっても、宣言時の初期値は必須。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[var.initializer]` | 2 | 18 |

### bind-014 fixのフィールドへの代入

主根拠：`[var.fix-assign]`。

**ソース：**

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix point = Point { x: 0 }
    point.x = 1
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

fixのフィールドは変更できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[var.fix-assign]` | 5 | 5 |

### bind-015 fixの入れ子のフィールドを変更する

主根拠：`[var.fix-assign]`。

**ソース：**

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn main() -> u8 {
    fix value = Box { point: Point { x: 1 } }
    value.point.x = 9
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
| 通常 | `[var.fix-assign]` | 6 | 5 |

### bind-016 実行されない分岐の代入もvarの再代入として数える

主根拠：`[var.never-reassigned]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 3
    if false {
        count = 4
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

実行時には変更されなくても、スコープ内に代入が構文上存在するのでvarを受理する。

### bind-017 再代入のないvar

主根拠：`[var.never-reassigned]`。

**ソース：**

```suz
fn main() -> u8 {
    var count = 0
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

初期化以外にcountへの代入がない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[var.never-reassigned]` | 2 | 9 |

### bind-018 通常の代入でキャストの代替値を使う

主根拠：`[assign.type]`。

補助根拠：`[expr.cast-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix raw: i64 = 300
    count = raw as u8 or 7
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

### bind-019 代入先と異なる型の変数を代入する

主根拠：`[assign.type]`。

補助根拠：`[type.numeric-conversion]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 0
    fix source: u16 = 1
    count = source
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

u16の値をu8の代入先へ暗黙変換しない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 4 | 13 |

### bind-020 複合代入の右辺で代替値を使う

主根拠：`[assign.compound]`。

補助根拠：`[expr.arithmetic-scope]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix divisor: u8 = 0
    count += (6 / divisor) or 3
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 13 | 0 |

右辺の除算が失敗して3になり、その値を現在のcountに加える。

### bind-021 右辺のorは複合代入自身の加算を処理しない

主根拠：`[assign.compound]`。

補助根拠：`[expr.arithmetic-scope]`、`[rt.panic]`、`[rt.int-overflow]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 255
    fix divisor: u8 = 1
    count += (1 / divisor) or 0
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

右辺の除算は成功して1になる。その後の255への加算がオーバーフローする。標準エラーに複合代入の位置と理由を出す。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 4 | 11 |

### bind-022 複合代入の右辺のor returnは代入前に戻る

主根拠：`[assign.eval-order]`。

補助根拠：`[expr.cast-return]`、`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix raw: i64 = 300
    count += raw as u8 or return count
    return 99
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 10 | 0 |

キャストが失敗した時点で現在のcountを返す。加算と代入は実行しない。

### bind-023 右辺で代入先を読み取り、別のフィールドへ代入する

主根拠：`[assign.rhs-write]`。

補助根拠：`[assign.target]`、`[expr.block]`。

**ソース：**

```suz
struct Point { x: u8, y: u8 }

fn main() -> u8 {
    var point = Point { x: 10, y: 0 }
    point.x = expr {
        point.y = 20
        point.x + 1
    }
    return ifexpr point.x == 11 && point.y == 20 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

point.xへの代入の右辺で、point.xの読み取りとpoint.yへの代入を行える。

### bind-024 複合代入の右辺で同じ代入先を書き換える

主根拠：`[assign.rhs-write]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 10
    count += expr {
        count = 20
        1
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

countへの複合代入の右辺に、countへの代入がある。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[assign.rhs-write]` | 4 | 9 |

### bind-025 通常の代入の右辺で同じ代入先を書き換える

主根拠：`[assign.rhs-write]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 10
    count = expr {
        if false {
            count += 1
        }
        7
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

通常の代入にも同じ制約を適用する。実行されない分岐内の複合代入も検査対象になる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[assign.rhs-write]` | 5 | 13 |

### bind-026 フィールドへの代入の右辺で親の構造体を書き換える

主根拠：`[assign.rhs-write]`。

**ソース：**

```suz
struct Point { x: u8, y: u8 }

fn main() -> u8 {
    var point = Point { x: 10, y: 0 }
    point.x = expr {
        point = Point { x: 20, y: 30 }
        1
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

point全体への代入は、外側の代入先point.xと重なる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[assign.rhs-write]` | 6 | 9 |

### bind-027 構造体への代入の右辺でそのフィールドを書き換える

主根拠：`[assign.rhs-write]`。

**ソース：**

```suz
struct Point { x: u8, y: u8 }

fn main() -> u8 {
    var point = Point { x: 10, y: 0 }
    point = expr {
        point.x = 20
        Point { x: 1, y: 2 }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

point.xへの代入は、外側の代入先point全体と重なる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[assign.rhs-write]` | 6 | 9 |

### bind-028 複合代入を丸括弧で囲んでorの対象にする

主根拠：`[assign.statement]`。

補助根拠：`[expr.arithmetic-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 255
    fix divisor: u8 = 1
    (count += (1 / divisor) or 0) or 1
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

複合代入は文なので、丸括弧で囲んでも式にはならず、外側のorの対象にはできない。除算と加算をそれぞれ処理する書き方はexpr-066を参照。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[assign.statement]` | 4 | 12 |

### bind-029 通常の代入を初期値の式として使う

主根拠：`[assign.statement]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 0
    fix result = (count = 1)
    return count
}
```


**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

通常の代入も文であり、丸括弧で囲んで値を得ることはできない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[assign.statement]` | 3 | 25 |
