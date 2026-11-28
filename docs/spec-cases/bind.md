# 定数・変数・代入

[目次と書式](../spec-cases.md)

### bind-001 定数の構造体フィールドへ代入する
- 主根拠：`[const.decl]`
- 補助根拠：`[assign.target]`
- 結果：コンパイルエラー `[const.decl]` 5:5

構造体を定数として宣言した場合、そのフィールドも変更できない。

```suz
struct Point { x: i32 }
fix ORIGIN = Point { x: 1 }

fn main() -> u8 {
    ORIGIN#x = 2
    return 0
}
```

### bind-002 定数が互いを循環参照する
- 主根拠：`[const.initializer]`
- 結果：コンパイルエラー `[const.initializer]` 2:18

前方参照は可能だが、firstとsecondの初期値は循環している。

```suz
fix FIRST: u8 = SECOND
fix SECOND: u8 = FIRST

fn main() -> u8 {
    return FIRST
}
```

### bind-003 構造体の定数とフィールド参照を定数式に使う
- 主根拠：`[const.expression]`
- 補助根拠：`[const.cast-check]`
- 結果：正常終了 3

originのフィールドを参照した加算も定数式になる。結果がu8に収まるので、変換のorを省略できる。

```suz
struct Point { x: i32, y: i32 }
fix ORIGIN = Point { x: 1, y: 2 }
fix TOTAL = ORIGIN.x + ORIGIN.y

fn main() -> u8 {
    return TOTAL as u8
}
```

### bind-004 短絡で評価しない関数呼び出しも定数式には使えない
- 主根拠：`[const.expression]`
- 補助根拠：`[const.evaluation]`
- 結果：コンパイルエラー `[const.expression]` 5:22

短絡で右辺を評価しなくても、定数式の構成を検査する。関数呼び出しは定数式に含まれない。

```suz
fn ready() -> bool {
    return true
}

fix RESULT = true || ready()

fn main() -> u8 {
    return ifexpr RESULT { 0 } else { 1 }
}
```

### bind-005 exprブロックを定数の初期値にする
- 主根拠：`[const.expression]`
- 補助根拠：`[const.initializer]`
- 結果：コンパイルエラー `[const.expression]` 1:18

内容がリテラルだけでも、exprブロックは定数式の構成要素ではない。

```suz
fix RESULT: u8 = expr { 3 }

fn main() -> u8 {
    return RESULT
}
```

### bind-006 定数式の成功時は代替値を評価しない
- 主根拠：`[const.evaluation]`
- 結果：正常終了 3

代替値の型は検査するが、ゼロ除算は評価しない。除数はu8型の値として範囲を見積もるため、必ずゼロで割る式とは判定しない。

```suz
fix RESULT: u8 = (1 + 2) or (1 / (0 as u8))

fn main() -> u8 {
    return RESULT
}
```

### bind-007 定数式の演算失敗時は代替値を評価
- 主根拠：`[const.evaluation]`
- 補助根拠：`[const.expression]`
- 結果：正常終了 7

```suz
fix RESULT: u8 = (255 + 1) or 7

fn main() -> u8 {
    return RESULT
}
```

### bind-008 定数のオーバーフロー
- 主根拠：`[const.evaluation]`
- 結果：コンパイルエラー `[const.evaluation]` 1:21

コンパイル時の加算が範囲外で、orがない。

```suz
fix VALUE: u8 = 255 + 1
fn main() -> u8 { return 0 }
```

### bind-009 実行しない代替値にも型検査を適用
- 主根拠：`[const.evaluation]`
- 補助根拠：`[expr.arithmetic-fallback]`
- 結果：コンパイルエラー `[type.expected]` 1:28

加算は成功するが、代替値のboolはu8と一致しない。

```suz
fix VALUE: u8 = (1 + 2) or false
fn main() -> u8 { return 0 }
```

### bind-010 定数変換の失敗
- 主根拠：`[const.cast-check]`
- 結果：コンパイルエラー `[const.cast-check]` 2:21

変換失敗がコンパイル時に分かり、orがない。

```suz
fn main() -> u8 {
    fix value = 300 as u8
    return 0
}
```

### bind-011 短絡で評価しない定数変換にも失敗処理を書く
- 主根拠：`[const.cast-check]`
- 補助根拠：`[const.evaluation]`
- 結果：正常終了

右辺のキャストは`or`の必須検査を満たす。論理和の左辺がtrueなので、右辺は評価しない。

```suz
fix RESULT = true || ((300 as u8 or 0) == 0)

fn main() -> u8 {
    return ifexpr RESULT { 0 } else { 1 }
}
```

### bind-012 短絡で評価しない失敗する定数変換
- 主根拠：`[const.cast-check]`
- 補助根拠：`[const.evaluation]`
- 結果：コンパイルエラー `[const.cast-check]` 1:28

右辺を評価しなくても、失敗する定数キャストには`or`が必要。

```suz
fix RESULT = true || ((300 as u8) == 0)

fn main() -> u8 {
    return ifexpr RESULT { 0 } else { 1 }
}
```

### bind-013 varの初期値を省略する
- 主根拠：`[var.initializer]`
- 結果：コンパイルエラー `[var.initializer]` 2:18

後続に代入があっても、宣言時の初期値は必須。

```suz
fn main() -> u8 {
    var count: u8
    count = 1
    return count
}
```

### bind-014 fixのフィールドへの代入
- 主根拠：`[var.fix-assign]`
- 結果：コンパイルエラー `[var.fix-assign]` 5:5

fixのフィールドは変更できない。

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix point = Point { x: 0 }
    point#x = 1
    return 0
}
```

### bind-015 fixの入れ子のフィールドを変更する
- 主根拠：`[var.fix-assign]`
- 結果：コンパイルエラー `[var.fix-assign]` 6:5

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn main() -> u8 {
    fix value = Box { point: Point { x: 1 } }
    value.point#x = 9
    return 0
}
```

### bind-016 実行されない分岐の代入もvarの再代入として数える
- 主根拠：`[var.never-reassigned]`
- 結果：正常終了 3

実行時には変更されなくても、スコープ内に代入が構文上存在するのでvarを受理する。

```suz
fn main() -> u8 {
    var count: u8 = 3
    if false {
        count = 4
    }
    return count
}
```

### bind-017 再代入のないvar
- 主根拠：`[var.never-reassigned]`
- 結果：コンパイルエラー `[var.never-reassigned]` 2:9

初期化以外にcountへの代入がない。

```suz
fn main() -> u8 {
    var count = 0
    return 0
}
```

### bind-018 通常の代入でキャストの代替値を使う
- 主根拠：`[assign.type]`
- 補助根拠：`[expr.cast-fallback]`
- 結果：正常終了 7

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix raw: i64 = 300
    count = raw as u8 or 7
    return count
}
```

### bind-019 代入先と異なる型の変数を代入する
- 主根拠：`[assign.type]`
- 補助根拠：`[type.numeric-conversion]`
- 結果：コンパイルエラー `[type.expected]` 4:13

u16の値をu8の代入先へ暗黙変換しない。

```suz
fn main() -> u8 {
    var count: u8 = 0
    fix source: u16 = 1
    count = source
    return count
}
```

### bind-020 複合代入の右辺で代替値を使う
- 主根拠：`[assign.compound]`
- 補助根拠：`[expr.arithmetic-scope]`
- 結果：正常終了 13

右辺の除算が失敗して3になり、その値を現在のcountに加える。

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix divisor: u8 = 0
    count += (6 / divisor) or 3
    return count
}
```

### bind-021 右辺のorは複合代入自身の加算を処理しない
- 主根拠：`[assign.compound]`
- 補助根拠：`[expr.arithmetic-scope]`、`[rt.panic]`、`[rt.int-overflow]`
- 結果：パニック `[rt.int-overflow]` 4:11

右辺の除算は成功して1になる。その後の255への加算がオーバーフローする。標準エラーに複合代入の位置と理由を出す。

```suz
fn main() -> u8 {
    var count: u8 = 255
    fix divisor: u8 = 1
    count += (1 / divisor) or 0
    return count
}
```

### bind-022 複合代入の右辺のor returnは代入前に戻る
- 主根拠：`[assign.eval-order]`
- 補助根拠：`[expr.cast-return]`、`[expr.or-return-position]`
- 結果：正常終了 10

キャストが失敗した時点で現在のcountを返す。加算と代入は実行しない。

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix raw: i64 = 300
    count += raw as u8 or return count
    return 99
}
```

### bind-023 右辺で代入先を読み取り、別のフィールドへ代入する
- 主根拠：`[assign.rhs-write]`
- 補助根拠：`[assign.target]`、`[expr.block]`
- 結果：正常終了

point.xへの代入の右辺で、point.xの読み取りとpoint.yへの代入を行える。

```suz
struct Point { x: u8, y: u8 }

fn main() -> u8 {
    var point = Point { x: 10, y: 0 }
    point#x = expr {
        point#y = 20
        point.x + 1
    }
    return ifexpr point.x == 11 && point.y == 20 { 0 } else { 1 }
}
```

### bind-024 複合代入の右辺で同じ代入先を書き換える
- 主根拠：`[assign.rhs-write]`
- 結果：コンパイルエラー `[assign.rhs-write]` 4:9

countへの複合代入の右辺に、countへの代入がある。

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

### bind-025 通常の代入の右辺で同じ代入先を書き換える
- 主根拠：`[assign.rhs-write]`
- 結果：コンパイルエラー `[assign.rhs-write]` 5:13

通常の代入にも同じ制約を適用する。実行されない分岐内の複合代入も検査対象になる。

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

### bind-026 フィールドへの代入の右辺で親の構造体を書き換える
- 主根拠：`[assign.rhs-write]`
- 結果：コンパイルエラー `[assign.rhs-write]` 6:9

point全体への代入は、外側の代入先point.xと重なる。

```suz
struct Point { x: u8, y: u8 }

fn main() -> u8 {
    var point = Point { x: 10, y: 0 }
    point#x = expr {
        point = Point { x: 20, y: 30 }
        1
    }
    return 0
}
```

### bind-027 構造体への代入の右辺でそのフィールドを書き換える
- 主根拠：`[assign.rhs-write]`
- 結果：コンパイルエラー `[assign.rhs-write]` 6:9

point.xへの代入は、外側の代入先point全体と重なる。

```suz
struct Point { x: u8, y: u8 }

fn main() -> u8 {
    var point = Point { x: 10, y: 0 }
    point = expr {
        point#x = 20
        Point { x: 1, y: 2 }
    }
    return 0
}
```

### bind-028 複合代入を丸括弧で囲んでorの対象にする
- 主根拠：`[assign.statement]`
- 補助根拠：`[expr.arithmetic-handler]`
- 結果：コンパイルエラー `[assign.statement]` 4:12

複合代入は文なので、丸括弧で囲んでも式にはならず、外側のorの対象にはできない。除算と加算をそれぞれ処理する書き方はexpr-066を参照。

```suz
fn main() -> u8 {
    var count: u8 = 255
    fix divisor: u8 = 1
    (count += (1 / divisor) or 0) or 1
    return count
}
```

### bind-029 通常の代入を初期値の式として使う
- 主根拠：`[assign.statement]`
- 結果：コンパイルエラー `[assign.statement]` 3:25

通常の代入も文であり、丸括弧で囲んで値を得ることはできない。

```suz
fn main() -> u8 {
    var count: u8 = 0
    fix result = (count = 1)
    return count
}
```

### bind-030 arenaの状態を関数へ渡し、コピーした値は保持する
- 主根拠：`[arena.ptr-operations]`
- 補助根拠：`[const.decl]`、`[value.copy-equivalent]`
- 結果：正常終了

```suz
fix INITIAL: u8 = 1
struct State { count: u8 }
fn update(state: mut State) -> void { state#count += 1 }
fn main() -> u8 {
    arena {
        fix state = new State { count: INITIAL }
        fix before = state.count
        update(state)
        if before != 1 || state.count != 2 { return 1 }
    }
    return 0
}
```

### bind-031 トップレベルのvarは代入の有無によらず拒否する
- 主根拠：`[decl.top-level]`
- 補助根拠：`[decl.location]`
- 結果：コンパイルエラー `[decl.top-level]` 1:1

```suz
var COUNT: u8 = 1
fn main() -> u8 {
    COUNT = 2
    return 0
}
```

### bind-032 トップレベルでは列挙による配列の確保を拒否する
- 主根拠：`[const.expression]`
- 結果：コンパイルエラー `[const.expression]` 1:23

```suz
fix PALETTE: #[i32] = new #[1, 2, 3]
fn main() -> u8 { return 0 }
```

### bind-033 トップレベルでは反復初期化による配列の確保を拒否する
- 主根拠：`[const.expression]`
- 結果：コンパイルエラー `[const.expression]` 1:9

```suz
fix B = new #[0; 3]
fn main() -> u8 { return 0 }
```

### bind-034 トップレベルではnewによる実体の確保を拒否する
- 主根拠：`[const.expression]`
- 結果：コンパイルエラー `[const.expression]` 1:9

```suz
fix A = new (1 as i32)
fn main() -> u8 { return 0 }
```
