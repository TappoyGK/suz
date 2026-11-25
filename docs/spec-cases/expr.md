# 式

[目次と書式](../spec-cases.md)

### expr-001 ラップする算術・シフトと切り詰め

主根拠：`[expr.wrap]`。

補助根拠：`[expr.as-trunc]`、`[expr.shift]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### expr-002 符号付き整数のラップとas%の符号の扱い

主根拠：`[expr.wrap]`。

補助根拠：`[expr.as-trunc]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

i8の127 +% 1は-128。255 as% i8は-1。-1 as% u16は65535になる。

### expr-003 ifexprの分岐の型不一致

主根拠：`[branch.expression]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = ifexpr true { 1 } else { false }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

整数とboolは同じ結果型にならない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 2 | 31 |

### expr-004 ifexprの両分岐がvoidになる

主根拠：`[branch.expression]`。

補助根拠：`[block.value]`。

**ソース：**

```suz
fn action() -> void {}

fn main() -> u8 {
    ifexpr true { action() } else { action() }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

両分岐の型が一致していても、結果がvoidのifexprは使えない。結果を捨てる位置でも同じ。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[branch.expression]` | 4 | 5 |

### expr-005 matchexprの全腕がvoidになる

主根拠：`[branch.expression]`。

補助根拠：`[match.value]`。

**ソース：**

```suz
fn action() -> void {}

fn main() -> u8 {
    matchexpr true {
        true => action()
        false => action()
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

全値を網羅し、全腕の型が一致していても、matchexprの結果はvoidにできない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[branch.expression]` | 4 | 5 |

### expr-006 expr内部のor return

主根拠：`[expr.jump-prohibited]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix result: u8 = expr {
        fix converted = raw as u8 or return 1
        converted
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

初期値全体であってもexpr内部のreturnは禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.jump-prohibited]` | 4 | 38 |

### expr-007 ifexpr内部のreturn

主根拠：`[expr.jump-prohibited]`。

**ソース：**

```suz
fn main() -> u8 {
    fix result: u8 = ifexpr true {
        return 1
        2
    } else {
        3
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

末尾に値があっても、途中のreturnは禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.jump-prohibited]` | 3 | 9 |

### expr-008 matchexpr内部のreturn

主根拠：`[expr.jump-prohibited]`。

**ソース：**

```suz
fn main() -> u8 {
    fix result: u8 = matchexpr true {
        true => {
            return 1
            2
        }
        false => 3
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

matchexprの腕の内部のreturnは禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.jump-prohibited]` | 4 | 13 |

### expr-009 expr系からreturnを持つ関数を呼び出す

主根拠：`[expr.jump-prohibited]`。

補助根拠：`[fn.call]`、`[fn.return]`。

**ソース：**

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return 2
    }
    return 3
}

fn main() -> u8 {
    fix result: u8 = expr {
        fix base = ifexpr true { choose(true) } else { choose(false) }
        base + 1
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

returnを書いているのは呼び出し先のchooseの本体。exprやifexprの内部にreturnを記述しているわけではなく、chooseから戻った後も値の評価を続ける。

### expr-010 expr内部のループのbreak

主根拠：`[expr.jump-prohibited]`。

**ソース：**

```suz
fn main() -> u8 {
    fix result: u8 = expr {
        loop {
            break
        }
        1
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

expr内部に書かれたループにも禁止を適用する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.jump-prohibited]` | 4 | 13 |

### expr-011 expr内部のループのcontinue

主根拠：`[expr.jump-prohibited]`。

**ソース：**

```suz
fn main() -> u8 {
    fix result: u8 = expr {
        for i in 0..<1 {
            continue
        }
        1
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

内側のforでもcontinueは禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.jump-prohibited]` | 4 | 13 |

### expr-070 値を作るブロック内のrevからcontinueする

主根拠：`[expr.jump-prohibited]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = expr {
        rev i in 0..<1 { continue }
        0
    }
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.jump-prohibited]` | 3 | 26 |

### expr-012 ifexprで選ばれない分岐の演算は実行しない

主根拠：`[expr.if-value]`。

補助根拠：`[if.condition]`。

**ソース：**

```suz
fn main() -> u8 {
    fix divisor: u8 = 0
    fix result: u8 = ifexpr true { 7 } else { 1 / divisor }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

elseの分岐を評価しないので、整数のゼロ除算によるパニックは起きない。

### expr-013 if文を初期値にする

主根拠：`[expr.if-value]`。

補助根拠：`[branch.statement]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = if true { 1 } else { 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

値を返す分岐はifexprで書く。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.if-value]` | 2 | 17 |

### expr-014 ifexprのelse漏れ

主根拠：`[expr.if-value]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = ifexpr true { 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

ifexprには最後のelseが必要。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.if-value]` | 2 | 34 |

### expr-015 exprとifexpr、elif

主根拠：`[expr.block]`。

補助根拠：`[expr.if-value]`、`[block.value]`、`[if.elif]`。

**ソース：**

```suz
fn main() -> u8 {
    fix input: i32 = 0
    fix value: u8 = expr {
        fix base: u8 = 4
        ifexpr input < 0 {
            1
        } elif input == 0 {
            base + 1
        } else {
            9
        }
    }
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 5 | 0 |

### expr-016 単独ブロックを初期値にする

主根拠：`[expr.block]`。

補助根拠：`[block.statement]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = { 1 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

値を作るブロックはexprで書く。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.block]` | 2 | 17 |

### expr-017 exprの中でwhileを使って値を計算する

主根拠：`[expr.block]`。

補助根拠：`[expr.jump-prohibited]`、`[block.value]`、`[while.loop]`。

**ソース：**

```suz
fn main() -> u8 {
    fix result: u8 = expr {
        var count: u8 = 0
        while count < 3 {
            count += 1
        }
        count
    }
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

breakやcontinueを使わず、条件がfalseになることでwhileを終了する。exprの末尾で計算した値を返す。

### expr-018 条件内の構造体リテラルを括弧で囲む

主根拠：`[expr.struct-in-cond]`。

補助根拠：`[expr.equality]`。

**ソース：**

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix p = Point { x: 0 }
    if p == (Point { x: 0 }) {
        return 0
    } else {
        return 1
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### expr-019 条件内の関数呼び出しに構造体リテラルを渡す

主根拠：`[expr.struct-in-cond]`。

**ソース：**

```suz
struct Point { x: i32 }

fn is_zero(point: Point) -> bool {
    return point.x == 0
}

fn main() -> u8 {
    if is_zero(Point { x: 0 }) {
        return 0
    } else {
        return 1
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

関数呼び出しの実引数を囲む丸括弧が、構造体リテラルを囲む括弧として有効になる。

### expr-020 notは比較より先に結合する

主根拠：`[expr.not-precedence]`。

補助根拠：`[expr.not]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: i32 = 1
    fix different = not value == 0
    return ifexpr different { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

not value == 0は(not value) == 0と読む。valueは整数なのでnotを適用できない。比較結果を否定する場合はnot (value == 0)と書く。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.not]` | 3 | 21 |

### expr-021 bool同士を不等号で比較する

主根拠：`[expr.operator-types]`。

boolに大小比較は定義されていない。

**ソース：**

```suz
fn main() -> u8 {
    fix result = true < false
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
| 通常 | `[expr.operator-types]` | 2 | 23 |

### expr-022 短絡で評価されない論理演算の右辺が整数

主根拠：`[expr.operator-types]`。

補助根拠：`[expr.short-circuit]`。

左辺がfalseなので実行時に右辺は評価されないが、右辺にもboolを要求する。整数の1を真偽値として扱わない。

**ソース：**

```suz
fn main() -> u8 {
    fix result = false && 1
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
| 通常 | `[expr.operator-types]` | 2 | 24 |

### expr-023 比較相手から整数リテラルの型を決める

主根拠：`[expr.literal-type]`。

左右どちらにリテラルを書いても、比較相手のtopからu64を要求する。比較結果に要求するboolは比較対象へ伝えない。

**ソース：**

```suz
fn main() -> u8 {
    fix top: u64 = 18446744073709551615
    fix same: bool = top == 18446744073709551615
    if not same { return 1 }
    if not (18446744073709551615 == (top)) { return 2 }
    return 7
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

### expr-024 not・論理積・論理和の優先順位

主根拠：`[expr.precedence]`。

not、論理積、論理和の順に強く結合する。firstはtrue、secondはfalse、thirdはfalseとなり、最後に7を返す。

**ソース：**

```suz
fn main() -> u8 {
    fix first = true || false && false
    fix second = (true || false) && false
    fix third = not true && false
    if not first { return 1 }
    if second { return 2 }
    if third { return 3 }
    return 7
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

### expr-025 算術・シフト・ビット演算と比較の優先順位

主根拠：`[expr.precedence]`。

**ソース：**

```suz
fn main() -> u8 {
    fix arithmetic = 2 + 3 * 4
    fix shifted = 1 << 1 + 2
    fix matched = 6 & 3 == 2
    return ifexpr arithmetic == 14 && shifted == 8 && matched { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

2 + 3 * 4は14、1 << 1 + 2は8。6 & 3 == 2は、ビットANDの結果2と比較する。

### expr-026 同じ優先順位の二項演算は左から結合する

主根拠：`[expr.associativity]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

10 - 3 - 2は5、20 / 2 / 2は5になる。右側を括弧でまとめると、それぞれ9と20になる。

### expr-027 関数の実引数を左から評価

主根拠：`[expr.eval-order]`。

**ソース：**

```suz
fn mark(n: i32) -> i32 {
    log info `${n}`
    return n
}

fn add(left: i32, right: i32) -> i32 {
    return left + right
}

fn main() -> u8 {
    fix total = add(mark(1), mark(2))
    return ifexpr total == 3 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info expr-027.suz:2: 1
info expr-027.suz:2: 2
```

### expr-028 論理演算の短絡評価

主根拠：`[expr.short-circuit]`。

**ソース：**

```suz
fn probe() -> bool {
    log info `probe`
    return true
}

fn main() -> u8 {
    fix first = false && probe()
    fix second = true || probe()
    return ifexpr not first && second { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

probeは呼ばれず、ログは出ない。

### expr-029 負数の除算・剰余・非負剰余・右シフト

主根拠：`[expr.int-division]`。

補助根拠：`[expr.shift]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

-7 / 3は-2、-7 % 3は-1、-7 %% 3は2。浮動小数点の-7.5 % 2.0は-1.5、-0.25 %% 1.0は0.75。符号付きの-7 >> 1は符号ビットで埋めるため-4、-7 >>> 1は0で埋めるため124になる。符号なしの249 >> 1は124になる。

### expr-030 同じ向きの不等号を連鎖させる

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**ソース：**

```suz
fn main() -> u8 {
    fix upper: u8 = 3
    fix ascending = 1 < 2 <= upper
    fix descending = upper >= 3 > 1
    fix top: u64 = 18446744073709551615
    fix boundary = 0 < 18446744073709551615 == top
    return ifexpr ascending && descending && boundary { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

厳密な不等号と等号付きの不等号を同じ向きで組み合わせられる。後ろのオペランドのu8型を、前のリテラルにも適用する。連鎖の末尾がu64型の場合は、i64の上限を超える先行リテラルにもu64型を適用する。

### expr-031 比較の連鎖で不等号の向きを変える

主根拠：`[expr.comparison-chain]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 > 3
    return ifexpr ordered { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

1 < 2 > 3は途中で不等号の向きが変わる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.comparison-chain]` | 2 | 25 |

### expr-032 等値比較を含む同じ向きの連鎖

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ascending = 1 == 1 < 2 == 2 <= 3
    fix descending = 3 == 3 >= 2 == 2 > 1
    fix equal = 2 == 2 == 2
    return ifexpr ascending && descending && equal { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

==は先頭にも途中にも置ける。不等号の向きは連鎖全体でそろえる。

### expr-033 比較の連鎖に!=を混ぜる

主根拠：`[expr.comparison-chain]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 != 3
    return ifexpr ordered { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

不等号の連鎖に!=を混ぜることはできない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.comparison-chain]` | 2 | 25 |

### expr-034 等値比較を挟んでも不等号の向きは変えられない

主根拠：`[expr.comparison-chain]`。

**ソース：**

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 == 2 > 1
    return ifexpr ordered { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

==を挟んだ前後で、不等号の向きが変わっている。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.comparison-chain]` | 2 | 30 |

### expr-035 !=だけの比較も連鎖できない

主根拠：`[expr.comparison-chain]`。

**ソース：**

```suz
fn main() -> u8 {
    fix different = 1 != 2 != 1
    return ifexpr different { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

!=は2つのオペランドの単独の比較で使う。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.comparison-chain]` | 2 | 28 |

### expr-036 等値比較だけの連鎖でbool・enum・構造体を比較する

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.equality]`。

**ソース：**

```suz
enum Wave: u8 { Sine, Saw }
struct Point { x: i32 }

fn main() -> u8 {
    fix first = Point { x: 1 }
    fix second = Point { x: 1 }
    fix third = Point { x: 1 }
    fix flags = true == true == true
    fix waves = Wave.Sine == Wave.Sine == Wave.Sine
    fix points = first == second == third
    return ifexpr flags && waves && points { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

数値以外でも、同じ型の値同士に==を適用できれば連鎖できる。

### expr-037 短絡で評価しない比較対象の型不一致

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**ソース：**

```suz
fn main() -> u8 {
    fix first: u8 = 3
    fix second: u8 = 2
    fix third: bool = true
    fix ordered = first < second < third
    return ifexpr ordered { 1 } else { 0 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

最初の比較が不成立でも、連鎖全体を型検査する。整数とboolは比較できない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.operator-types]` | 5 | 25 |

### expr-038 比較の連鎖は中間の式を1回だけ評価する

主根拠：`[expr.comparison-chain-evaluation]`。

**ソース：**

```suz
fn mark(value: i32) -> i32 {
    log info `${value}`
    return value
}

fn main() -> u8 {
    fix ordered = mark(1) < mark(2) < mark(3)
    return ifexpr ordered { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

mark(2)の結果を両側の比較で共有する。

**標準エラー：**

```text
info expr-038.suz:2: 1
info expr-038.suz:2: 2
info expr-038.suz:2: 3
```

### expr-039 比較の連鎖は不成立で評価を打ち切る

主根拠：`[expr.comparison-chain-evaluation]`。

**ソース：**

```suz
fn probe() -> i32 {
    log info `probe`
    return 4
}

fn main() -> u8 {
    fix ordered = 3 < 2 < probe()
    return ifexpr ordered { 1 } else { 0 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

3 < 2が不成立なので、probeは呼ばれずログも出ない。

### expr-040 NaNを含む構造体は自分自身とも等しくない

主根拠：`[expr.equality]`。

補助根拠：`[expr.nan-comparison]`。

**ソース：**

```suz
struct Sample { value: f64 }

fn main() -> u8 {
    fix sample = Sample { value: nan }
    return ifexpr not (sample == sample) && sample != sample { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

構造体の等値比較にも各フィールドの==を使う。NaNのフィールドがあるため、同じ変数同士を比較しても==はfalse、!=はtrueになる。

### expr-041 NaNで比較の連鎖が不成立になれば残りを評価しない

主根拠：`[expr.nan-comparison]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**ソース：**

```suz
fn probe() -> f64 {
    log info `probe`
    return 1.0
}

fn main() -> u8 {
    fix ordered = 0.0 < nan < probe()
    return ifexpr ordered { 1 } else { 0 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

0.0 < nanがfalseなので、probeは呼ばれずログも出ない。

### expr-042 f32からf64への変換で有限値と特殊値を保つ

主根拠：`[expr.as]`。

補助根拠：`[expr.nan-comparison]`、`[expr.cast-risk]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

f32からf64への変換は常に成功し、orを必要としない。有限値は正確に表し、無限大とNaNも保つ。

### expr-043 boolから整数へのasは使えない

主根拠：`[expr.as]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = true as u8
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

boolから整数への変換はasが許可する型の組み合わせに含まれない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.as]` | 2 | 22 |

### expr-044 異なるenumへ直接asで変換する

主根拠：`[expr.as]`。

**ソース：**

```suz
enum First: u8 { A, B }
enum Second: u8 { A, B }

fn main() -> u8 {
    fix source = First.A
    fix value = source as Second or Second.A
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

enum同士の直接変換は許可されない。orを付けても許可される型の組み合わせは変わらない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.as]` | 6 | 24 |

### expr-071 整数とcharの変換で成功範囲と代替値を扱う

主根拠：`[char.cast]`。

**ソース：**

```suz
fn scalar(n: i32) -> char { return n as char or '?' }
fn main() -> u8 {
    fix byte: u8 = 65
    fix ch = byte as char
    fix narrow: u8 = 'あ' as u8 or 7
    fix code = ch as u32
    log info `${ch} ${code} ${narrow}`
    fix surrogate = scalar(55296)
    fix negative = scalar(-1)
    fix above = scalar(1114112)
    log info `${surrogate} ${negative} ${above}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info expr-071.suz:7: A 65 7
info expr-071.suz:11: ? ? ?
```

### expr-045 必ず成功する変換と定数式の変換

主根拠：`[expr.cast-risk]`。

補助根拠：`[const.cast-check]`。

**ソース：**

```suz
fn main() -> u8 {
    fix small: u8 = 7
    fix wide = small as i64
    fix literal = 255 as u8
    return ifexpr wide == 7 && literal == 255 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### expr-046 値が小さくてもfixからの縮小変換にはorが必要

主根拠：`[expr.cast-risk]`。

補助根拠：`[const.expression]`、`[expr.cast-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 1
    fix value = raw as u8
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

rawはconstではなく、変換は型で判定すると失敗しうる。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.cast-handler]` | 3 | 21 |

### expr-047 変換元を1回だけ評価し、成功時の代替値を省略

主根拠：`[expr.cast-fallback]`。

補助根拠：`[log.output]`。

**ソース：**

```suz
fn source() -> i64 {
    log info `source`
    return 7
}

fn fallback() -> u8 {
    log info `fallback`
    return 9
}

fn main() -> u8 {
    fix value = source() as u8 or fallback()
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

**標準エラー：**

```text
info expr-047.suz:2: source
```

### expr-048 変換失敗時だけ代替値を評価

主根拠：`[expr.cast-fallback]`。

**ソース：**

```suz
fn source() -> i64 {
    log info `source`
    return 300
}

fn fallback() -> u8 {
    log info `fallback`
    return 9
}

fn main() -> u8 {
    fix value = source() as u8 or fallback()
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 9 | 0 |

**標準エラー：**

```text
info expr-048.suz:2: source
info expr-048.suz:7: fallback
```

### expr-049 orの代替値の型不一致

主根拠：`[expr.cast-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix value = raw as u8 or false
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

代替値にはu8を要求する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[type.expected]` | 3 | 30 |

### expr-050 or return exprで関数を抜ける

主根拠：`[expr.cast-return]`。

補助根拠：`[expr.block]`、`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count = raw as u8 or return expr {
        log info `conversion failed`
        7
    }
    log info `count=${count}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

**標準エラー：**

```text
info expr-050.suz:4: conversion failed
```

### expr-051 or return exprの成功経路

主根拠：`[expr.cast-return]`。

補助根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 3
    fix count = raw as u8 or return expr {
        log info `conversion failed`
        7
    }
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 3 | 0 |

### expr-052 戻り値voidのor return

主根拠：`[expr.cast-return]`。

補助根拠：`[fn.return]`。

**ソース：**

```suz
fn report(raw: i64) -> void {
    fix count = raw as u8 or return
    log info `count=${count}`
}

fn main() -> u8 {
    report(300)
    log info `done`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info expr-052.suz:8: done
```

### expr-053 or returnを含む右辺全体の丸括弧

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count = (raw as u8 or return 7)
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

### expr-054 加算の途中にor returnを埋め込む

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix result = (raw as u8 or return 1) + 2
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

or returnを付けた変換が初期値全体ではない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 3 | 32 |

### expr-055 関数の引数にor returnを埋め込む

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn use(value: u8) -> void {}

fn main() -> u8 {
    fix raw: i64 = 300
    use(raw as u8 or return 1)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

実引数の内部のor returnは禁止。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 5 | 22 |

### expr-056 returnに渡す式の中にor returnを書く

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    return raw as u8 or return 7
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

returnの値を作る式にはor returnを置けない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 3 | 25 |

### expr-057 構造体のフィールド初期値にor returnを書く

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
struct Counter { value: u8 }

fn main() -> u8 {
    fix raw: i64 = 300
    fix counter = Counter { value: raw as u8 or return 7 }
    return counter.value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

変換はフィールド初期値の全体でも、fixの初期値全体ではない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 5 | 49 |

### expr-058 orの代替値の中にor returnを書く

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix first: i64 = 300
    fix second: i64 = 400
    fix value = first as u8 or (second as u8 or return 7)
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

内側の変換は外側のorの代替値に埋め込まれているため、or returnを付けられない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 4 | 49 |

### expr-059 キャストのorは右側の式全体を代替値にする

主根拠：`[expr.cast-binding]`。

補助根拠：`[expr.cast-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    fix raw: i64 = 7
    fix first = raw as u8 or 1 + 2
    fix second = (raw as u8 or 1) + 2
    return ifexpr first == 7 && second == 9 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

変換は成功するので、firstは7。secondは変換結果に2を加えて9になる。

### expr-060 enumと整数の相互変換

主根拠：`[expr.enum-conversion]`。

補助根拠：`[enum.variant]`、`[expr.cast-risk]`、`[rt.as-int-enum]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### expr-061 同じ加算の失敗をorで処理

主根拠：`[expr.arithmetic-handler]`。

補助根拠：`[expr.arithmetic-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = (top + 1) or 7
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

### expr-062 浮動小数点の除算に演算失敗のorを付ける

主根拠：`[expr.arithmetic-handler]`。

補助根拠：`[rt.float]`。

**ソース：**

```suz
fn main() -> u8 {
    fix zero = 0.0
    fix result = (1.0 / zero) or 0.0
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

演算失敗を処理するorの対象は整数演算。浮動小数点の除算には付けられない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.arithmetic-handler]` | 3 | 31 |

### expr-063 算術のorは対象の演算を1回だけ評価

主根拠：`[expr.arithmetic-fallback]`。

補助根拠：`[expr.arithmetic-scope]`。

**ソース：**

```suz
fn left_value() -> u8 {
    log info `left`
    return 250
}

fn right_value() -> u8 {
    log info `right`
    return 10
}

fn alternative() -> u8 {
    log info `alternative`
    return 7
}

fn main() -> u8 {
    fix total = (left_value() + right_value()) or alternative()
    return total
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

**標準エラー：**

```text
info expr-063.suz:2: left
info expr-063.suz:7: right
info expr-063.suz:12: alternative
```

### expr-064 加算全体を処理してから代入する

主根拠：`[expr.arithmetic-fallback]`。

補助根拠：`[assign.target]`。

**ソース：**

```suz
fn main() -> u8 {
    var count: u8 = 255
    count = (count + 1) or 7
    return count
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

countへの加算が失敗した場合の代替値を、通常の代入で保存する。

### expr-065 整数演算の失敗時に初期化を中断して戻る

主根拠：`[expr.arithmetic-fallback]`。

補助根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = (top + 1) or return 7
    return result
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

加算全体がfixの初期値なのでor returnを付けられる。オーバーフローしたら7を返し、後続のreturnには進まない。

### expr-066 通常の代入で除算と加算をそれぞれ処理する

主根拠：`[expr.arithmetic-scope]`。

補助根拠：`[assign.target]`、`[expr.arithmetic-fallback]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

bind-028の複合代入を通常の代入に書き換えた形。divisorが0なら除算の代替値0を加えて255になる。1なら除算は成功するが加算がオーバーフローし、外側の代替値1を代入する。

### expr-067 orは算術式全体の格納失敗を処理する

主根拠：`[expr.arithmetic-scope]`。

補助根拠：`[rt.int-overflow]`。

**ソース：**

```suz
fn operation(value: u8) -> u8 {
    fix result = ((value + 1) * 1) or 0
    return result
}

fn main() -> u8 {
    return operation(255)
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

途中の加算では失敗せず、結果256をu8へ格納するときに失敗する。orの代替値0を返す。

### expr-068 呼び出し先のパニックをorで捕まえない

主根拠：`[expr.arithmetic-scope]`。

補助根拠：`[rt.panic]`、`[rt.div-zero]`。

**ソース：**

```suz
fn divide(divisor: i32) -> i32 {
    return 1 / divisor
}

fn main() -> u8 {
    fix result = (divide(0) + 1) or 0
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |

divide内の除算が失敗する。加算に付いたorは実行しない。標準エラーに除算の位置と理由を出す。

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.div-zero]` | 2 | 14 |

### expr-069 orの代替値にあるorは右から結合する

主根拠：`[expr.or-binding]`。

補助根拠：`[expr.arithmetic-fallback]`。

**ソース：**

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = top + 1 or top + 1 or 7
    return result
}
```


**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 7 | 0 |

最初の加算が失敗すると、代替値の加算とその失敗処理を評価する。両方の加算が失敗し、最後の7になる。
### expr-072 text全体を1つのスカラー値に変換する

主根拠：`[char.cast-symbol]`。

**ソース：**

```suz
fn parse(input: symbol) -> char { return input as char or '?' }
fn main() -> u8 {
    fix emoji = parse(`😀`)
    fix empty = parse(``)
    fix multiple = parse(`ab`)
    fix quoted = parse(`'a'`)
    log info `${emoji} ${empty} ${multiple} ${quoted}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info expr-072.suz:7: 😀 ? ? ?
```

### expr-073 構造体のcharフィールドを文字リテラルとして読む

主根拠：`[symbol.cast-struct-field]`。

**ソース：**

```suz
struct Letter { ch: char }
fn read(input: symbol) -> Letter { return input as Letter or Letter { ch: '?' } }
fn main() -> u8 {
    fix good = read(`Letter { ch: '\\u{1F600}' }`)
    fix bad = read(`Letter { ch: 'ab' }`)
    log info `${good.ch} ${bad.ch}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info expr-073.suz:6: 😀 ?
```

### expr-074 途中の値を保持して格納時だけ範囲を検査する

主根拠：`[arith.exact]`。

**ソース：**

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

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

和300と積20000を途中で切り詰めない。外側からu16を要求しても、or付きの内側の算術式はu8への格納に失敗し255を返す。単一の整数16777218はf32で正確に表せる。

### expr-075 ゼロ除算より前の128ビット超過を拒否する

主根拠：`[arith.width]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: u64 = 1
    fix result: u64 = (value * value * value / 0) or 0
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

最初の積は符号なし128ビットで表せるが、次の積の見積もりは収まらない。実際の変数の値と後続のゼロ除算は、この検査を省略する理由にならない。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arith.width]` | 3 | 38 |

### expr-076 型の範囲から必ずゼロで割る式を拒否する

主根拠：`[arith.div-zero-always]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value: u8 = 7
    fix result = value / (value % 1)
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

valueの実際の値を使わなくても、value % 1の範囲は0だけになる。失敗処理がないため拒否する。

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[arith.div-zero-always]` | 3 | 24 |

### expr-077 整数と浮動小数点数を丸めずに比較する

主根拠：`[conv.compare]`。

**ソース：**

```suz
fn main() -> u8 {
    fix integer: u64 = 9_007_199_254_740_993
    fix rounded: f64 = 9_007_199_254_740_992.0
    fix negative: i8 = -1
    return ifexpr integer > rounded && rounded < integer && negative < integer && integer != nan && not (integer <= nan) && 0 == -0.0 { 0 } else { 1 }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

整数をf64へ丸めず、1の差を保つ。符号の異なる整数、NaN、負のゼロもそれぞれの値の規則で比較する。

### expr-078 ポインタの読み出しと更新対象の指定

主根拠：`[arena.ptr-operations]`。

**ソース：**

```suz
struct Point { x: i32 }
struct Holder { value: mut i32 }
fn select(holder: mut Holder, calls: mut i32) -> mut i32 {
    #calls += 1
    return holder.value
}
fn select_point(point: mut Point, calls: mut i32) -> mut Point {
    #calls += 1
    return point
}
fn main() -> u8 {
    arena {
        fix first = new (7 as i32)
        fix second = new (20 as i32)
        fix holder = new Holder { value: first }
        fix calls = new (0 as i32)
        // 対象は1回だけ評価し、右辺がポインタを付け替えても元の実体を更新する。
        #select(holder, calls) = expr { holder#value = second
            9 }
        if .calls != 1 || .first != 9 || .second != 20 { return 1 }
        // 複合代入の元の値も、右辺の実行より先に読む。
        #select(holder, calls) += expr { #second = 40
            holder#value = first
            2 }
        if .calls != 2 || .second != 22 { return 2 }
        if .select(holder, calls) != 9 || .calls != 3 { return 3 }
        fix pp = new first
        #.pp += 1
        if ..pp != 10 || .(.pp) != 10 { return 4 }
        if .(holder.value) != 10 { return 5 }
        fix point = new Point { x: 1 }
        fix copy = .point
        (.point)#x = 2
        point#x += 1
        if copy.x != 1 || (.point).x != 3 { return 6 }
        #point = Point { x: 4 }
        if point.x != 4 { return 7 }
        select_point(point, calls)#x += expr { point#x = 40
            2 }
        if point.x != 6 || .calls != 4 { return 10 }
        var value = Point { x: 1 }
        value#x = 2
        if value.x != 2 { return 8 }
        // 乗算の * は変わらない。
        if .first * 2 != 20 { return 9 }
    }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |
