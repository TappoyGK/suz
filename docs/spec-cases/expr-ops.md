# 仕様例：式（演算子・比較連鎖・値ブロック・制御）

### expr-003 ifexprの分岐の型不一致
- 主根拠：`[branch.expression]`
- 結果：コンパイルエラー `[type.expected]` 2:31

整数とboolは同じ結果型にならない。

```suz
fn main() -> u8 {
    fix value = ifexpr true { 1 } else { false }
    return 0
}
```

### expr-004 ifexprの両分岐がvoidになる
- 主根拠：`[branch.expression]`
- 補助根拠：`[block.value]`
- 結果：コンパイルエラー `[branch.expression]` 4:5

両分岐の型が一致していても、結果がvoidのifexprは使えない。結果を捨てる位置でも同じ。

```suz
fn action() -> void {}

fn main() -> u8 {
    ifexpr true { action() } else { action() }
    return 0
}
```

### expr-005 matchexprの全腕がvoidになる
- 主根拠：`[branch.expression]`
- 補助根拠：`[match.value]`
- 結果：コンパイルエラー `[branch.expression]` 4:5

全値を網羅し、全腕の型が一致していても、matchexprの結果はvoidにできない。

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

### expr-006 expr内部のor return
- 主根拠：`[expr.jump-prohibited]`
- 結果：コンパイルエラー `[expr.jump-prohibited]` 4:38

初期値全体であってもexpr内部のreturnは禁止。

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

### expr-007 ifexpr内部のreturn
- 主根拠：`[expr.jump-prohibited]`
- 結果：コンパイルエラー `[expr.jump-prohibited]` 3:9

末尾に値があっても、途中のreturnは禁止。

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

### expr-008 matchexpr内部のreturn
- 主根拠：`[expr.jump-prohibited]`
- 結果：コンパイルエラー `[expr.jump-prohibited]` 4:13

matchexprの腕の内部のreturnは禁止。

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

### expr-009 expr系からreturnを持つ関数を呼び出す
- 主根拠：`[expr.jump-prohibited]`
- 補助根拠：`[fn.call]`、`[fn.return]`
- 結果：正常終了 3

returnを書いているのは呼び出し先のchooseの本体。exprやifexprの内部にreturnを記述しているわけではなく、chooseから戻った後も値の評価を続ける。

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

### expr-010 expr内部のループのbreak
- 主根拠：`[expr.jump-prohibited]`
- 結果：コンパイルエラー `[expr.jump-prohibited]` 4:13

expr内部に書かれたループにも禁止を適用する。

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

### expr-011 expr内部のループのcontinue
- 主根拠：`[expr.jump-prohibited]`
- 結果：コンパイルエラー `[expr.jump-prohibited]` 4:13

内側のforでもcontinueは禁止。

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

### expr-070 値を作るブロック内のrevからcontinueする
- 主根拠：`[expr.jump-prohibited]`
- 結果：コンパイルエラー `[expr.jump-prohibited]` 3:26

```suz
fn main() -> u8 {
    fix value = expr {
        rev i in 0..<1 { continue }
        0
    }
    return value
}
```

### expr-012 ifexprで選ばれない分岐の演算は実行しない
- 主根拠：`[expr.if-value]`
- 補助根拠：`[if.condition]`
- 結果：正常終了 7

elseの分岐を評価しないので、整数のゼロ除算によるパニックは起きない。

```suz
fn main() -> u8 {
    fix divisor: u8 = 0
    fix result: u8 = ifexpr true { 7 } else { 1 / divisor }
    return result
}
```

### expr-013 if文を初期値にする
- 主根拠：`[expr.if-value]`
- 補助根拠：`[branch.statement]`
- 結果：コンパイルエラー `[expr.if-value]` 2:17

値を返す分岐はifexprで書く。

```suz
fn main() -> u8 {
    fix value = if true { 1 } else { 2 }
    return 0
}
```

### expr-014 ifexprのelse漏れ
- 主根拠：`[expr.if-value]`
- 結果：コンパイルエラー `[expr.if-value]` 2:34

ifexprには最後のelseが必要。

```suz
fn main() -> u8 {
    fix value = ifexpr true { 1 }
    return 0
}
```

### expr-015 exprとifexpr、elif
- 主根拠：`[expr.block]`
- 補助根拠：`[expr.if-value]`、`[block.value]`、`[if.elif]`
- 結果：正常終了 5

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

### expr-016 単独ブロックを初期値にする
- 主根拠：`[expr.block]`
- 補助根拠：`[block.statement]`
- 結果：コンパイルエラー `[expr.block]` 2:17

値を作るブロックはexprで書く。

```suz
fn main() -> u8 {
    fix value = { 1 }
    return 0
}
```

### expr-017 exprの中でwhileを使って値を計算する
- 主根拠：`[expr.block]`
- 補助根拠：`[expr.jump-prohibited]`、`[block.value]`、`[while.loop]`
- 結果：正常終了 3

breakやcontinueを使わず、条件がfalseになることでwhileを終了する。exprの末尾で計算した値を返す。

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

### expr-018 条件内の構造体リテラルを括弧で囲む
- 主根拠：`[expr.struct-in-cond]`
- 補助根拠：`[expr.equality]`
- 結果：正常終了

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

### expr-019 条件内の関数呼び出しに構造体リテラルを渡す
- 主根拠：`[expr.struct-in-cond]`
- 結果：正常終了

関数呼び出しの実引数を囲む丸括弧が、構造体リテラルを囲む括弧として有効になる。

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

### expr-020 notは比較より先に結合する
- 主根拠：`[expr.not-precedence]`
- 補助根拠：`[expr.not]`
- 結果：コンパイルエラー `[expr.not]` 3:21

not value == 0は(not value) == 0と読む。valueは整数なのでnotを適用できない。比較結果を否定する場合はnot (value == 0)と書く。

```suz
fn main() -> u8 {
    fix value: i32 = 1
    fix different = not value == 0
    return ifexpr different { 0 } else { 1 }
}
```

### expr-021 bool同士を不等号で比較する
- 主根拠：`[expr.operator-types]`
- 結果：コンパイルエラー `[expr.operator-types]` 2:23

boolに大小比較は定義されていない。

```suz
fn main() -> u8 {
    fix result = true < false
    return 0
}
```

### expr-022 短絡で評価されない論理演算の右辺が整数
- 主根拠：`[expr.operator-types]`
- 補助根拠：`[expr.short-circuit]`
- 結果：コンパイルエラー `[expr.operator-types]` 2:24

左辺がfalseなので実行時に右辺は評価されないが、右辺にもboolを要求する。整数の1を真偽値として扱わない。

```suz
fn main() -> u8 {
    fix result = false && 1
    return 0
}
```

### expr-084 符号なし整数への単項飽和符号反転は拒否する
- 主根拠：`[expr.operator-types]`
- 補助根拠：`[expr.sat]`
- 結果：コンパイルエラー `[expr.operator-types]` 2:17

```suz
fn main() -> u8 {
    fix a: u8 = -| (1 as u8)
    return a
}
```

### expr-087 len_t の飽和算術は拒否する
- 主根拠：`[expr.operator-types]`
- 補助根拠：`[len.type-operations]`
- 結果：コンパイルエラー `[expr.operator-types]` 3:22

```suz
fn main() -> u8 {
    fix a: len_t = 1
    fix b: len_t = a +| a
    return 0
}
```

### expr-024 not・論理積・論理和の優先順位
- 主根拠：`[expr.precedence]`
- 結果：正常終了 7

not、論理積、論理和の順に強く結合する。firstはtrue、secondはfalse、thirdはfalseとなり、最後に7を返す。

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

### expr-025 算術・シフト・ビット演算と比較の優先順位
- 主根拠：`[expr.precedence]`
- 結果：正常終了

2 + 3 * 4は14、1 << 1 + 2は8。6 & 3 == 2は、ビットANDの結果2と比較する。

```suz
fn main() -> u8 {
    fix arithmetic = 2 + 3 * 4
    fix shifted = 1 << 1 + 2
    fix matched = 6 & 3 == 2
    return ifexpr arithmetic == 14 && shifted == 8 && matched { 0 } else { 1 }
}
```

### expr-027 関数の実引数を左から評価
- 主根拠：`[expr.eval-order]`
- 結果：正常終了

```suz
fn mark(n: i32) -> i32 {
    log info "${n}"
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

**標準エラー：**

```text
info expr-027.suz:2: 1
info expr-027.suz:2: 2
```

### expr-028 論理演算の短絡評価
- 主根拠：`[expr.short-circuit]`
- 結果：正常終了

probeは呼ばれず、ログは出ない。

```suz
fn probe() -> bool {
    log info "probe"
    return true
}

fn main() -> u8 {
    fix first = false && probe()
    fix second = true || probe()
    return ifexpr not first && second { 0 } else { 1 }
}
```

### expr-030 同じ向きの不等号を連鎖させる
- 主根拠：`[expr.comparison-chain]`
- 補助根拠：`[expr.comparison-chain-evaluation]`
- 結果：正常終了

厳密な不等号と等号付きの不等号を同じ向きで組み合わせられる。後ろのオペランドのu8型を、前のリテラルにも適用する。連鎖の末尾がu64型の場合は、i64の上限を超える先行リテラルにもu64型を適用する。

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

### expr-031 比較の連鎖で不等号の向きを変える
- 主根拠：`[expr.comparison-chain]`
- 結果：コンパイルエラー `[expr.comparison-chain]` 2:25

1 < 2 > 3は途中で不等号の向きが変わる。

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 > 3
    return ifexpr ordered { 0 } else { 1 }
}
```

### expr-032 等値比較を含む同じ向きの連鎖
- 主根拠：`[expr.comparison-chain]`
- 補助根拠：`[expr.comparison-chain-evaluation]`
- 結果：正常終了

==は先頭にも途中にも置ける。不等号の向きは連鎖全体でそろえる。

```suz
fn main() -> u8 {
    fix ascending = 1 == 1 < 2 == 2 <= 3
    fix descending = 3 == 3 >= 2 == 2 > 1
    fix equal = 2 == 2 == 2
    return ifexpr ascending && descending && equal { 0 } else { 1 }
}
```

### expr-033 比較の連鎖に!=を混ぜる
- 主根拠：`[expr.comparison-chain]`
- 結果：コンパイルエラー `[expr.comparison-chain]` 2:25

不等号の連鎖に!=を混ぜることはできない。

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 != 3
    return ifexpr ordered { 0 } else { 1 }
}
```

### expr-034 等値比較を挟んでも不等号の向きは変えられない
- 主根拠：`[expr.comparison-chain]`
- 結果：コンパイルエラー `[expr.comparison-chain]` 2:30

==を挟んだ前後で、不等号の向きが変わっている。

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 == 2 > 1
    return ifexpr ordered { 0 } else { 1 }
}
```

### expr-035 !=だけの比較も連鎖できない
- 主根拠：`[expr.comparison-chain]`
- 結果：コンパイルエラー `[expr.comparison-chain]` 2:28

!=は2つのオペランドの単独の比較で使う。

```suz
fn main() -> u8 {
    fix different = 1 != 2 != 1
    return ifexpr different { 0 } else { 1 }
}
```

### expr-036 等値比較だけの連鎖でbool・enum・構造体を比較する
- 主根拠：`[expr.comparison-chain]`
- 補助根拠：`[expr.equality]`
- 結果：正常終了

数値以外でも、同じ型の値同士に==を適用できれば連鎖できる。

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

### expr-037 短絡で評価しない比較対象の型不一致
- 主根拠：`[expr.comparison-chain]`
- 補助根拠：`[expr.comparison-chain-evaluation]`
- 結果：コンパイルエラー `[expr.operator-types]` 5:25

最初の比較が不成立でも、連鎖全体を型検査する。整数とboolは比較できない。

```suz
fn main() -> u8 {
    fix first: u8 = 3
    fix second: u8 = 2
    fix third: bool = true
    fix ordered = first < second < third
    return ifexpr ordered { 1 } else { 0 }
}
```

### expr-038 比較の連鎖は中間の式を1回だけ評価する
- 主根拠：`[expr.comparison-chain-evaluation]`
- 結果：正常終了

mark(2)の結果を両側の比較で共有する。

```suz
fn mark(value: i32) -> i32 {
    log info "${value}"
    return value
}

fn main() -> u8 {
    fix ordered = mark(1) < mark(2) < mark(3)
    return ifexpr ordered { 0 } else { 1 }
}
```

**標準エラー：**

```text
info expr-038.suz:2: 1
info expr-038.suz:2: 2
info expr-038.suz:2: 3
```

### expr-039 比較の連鎖は不成立で評価を打ち切る
- 主根拠：`[expr.comparison-chain-evaluation]`
- 結果：正常終了

3 < 2が不成立なので、probeは呼ばれずログも出ない。

```suz
fn probe() -> i32 {
    log info "probe"
    return 4
}

fn main() -> u8 {
    fix ordered = 3 < 2 < probe()
    return ifexpr ordered { 1 } else { 0 }
}
```

### expr-040 NaNを含む構造体は自分自身とも等しくない
- 主根拠：`[expr.equality]`
- 補助根拠：`[expr.nan-comparison]`
- 結果：正常終了

構造体の等値比較にも各フィールドの==を使う。NaNのフィールドがあるため、同じ変数同士を比較しても==はfalse、!=はtrueになる。

```suz
struct Sample { value: f64 }

fn main() -> u8 {
    fix sample = Sample { value: nan }
    return ifexpr not (sample == sample) && sample != sample { 0 } else { 1 }
}
```

### expr-041 NaNで比較の連鎖が不成立になれば残りを評価しない
- 主根拠：`[expr.nan-comparison]`
- 補助根拠：`[expr.comparison-chain-evaluation]`
- 結果：正常終了

0.0 < nanがfalseなので、probeは呼ばれずログも出ない。

```suz
fn probe() -> f64 {
    log info "probe"
    return 1.0
}

fn main() -> u8 {
    fix ordered = 0.0 < nan < probe()
    return ifexpr ordered { 1 } else { 0 }
}
```

### expr-053 or returnを含む右辺全体の丸括弧
- 主根拠：`[expr.or-return-position]`
- 結果：正常終了 7

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count = (raw as u8 or return 7)
    return count
}
```

### expr-054 加算の途中にor returnを埋め込む
- 主根拠：`[expr.or-return-position]`
- 結果：コンパイルエラー `[expr.or-return-position]` 3:32

or returnを付けた変換が初期値全体ではない。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix result = (raw as u8 or return 1) + 2
    return result
}
```

### expr-055 関数の引数にor returnを埋め込む
- 主根拠：`[expr.or-return-position]`
- 結果：コンパイルエラー `[expr.or-return-position]` 5:22

実引数の内部のor returnは禁止。

```suz
fn use(value: u8) -> void {}

fn main() -> u8 {
    fix raw: i64 = 300
    use(raw as u8 or return 1)
    return 0
}
```

### expr-056 returnに渡す式の中にor returnを書く
- 主根拠：`[expr.or-return-position]`
- 結果：コンパイルエラー `[expr.or-return-position]` 3:25

returnの値を作る式にはor returnを置けない。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    return raw as u8 or return 7
}
```

### expr-057 構造体のフィールド初期値にor returnを書く
- 主根拠：`[expr.or-return-position]`
- 結果：コンパイルエラー `[expr.or-return-position]` 5:49

変換はフィールド初期値の全体でも、fixの初期値全体ではない。

```suz
struct Counter { value: u8 }

fn main() -> u8 {
    fix raw: i64 = 300
    fix counter = Counter { value: raw as u8 or return 7 }
    return counter.value
}
```

### expr-058 orの代替値の中にor returnを書く
- 主根拠：`[expr.or-return-position]`
- 結果：コンパイルエラー `[expr.or-return-position]` 4:49

内側の変換は外側のorの代替値に埋め込まれているため、or returnを付けられない。

```suz
fn main() -> u8 {
    fix first: i64 = 300
    fix second: i64 = 400
    fix value = first as u8 or (second as u8 or return 7)
    return value
}
```

### expr-069 orの代替値にあるorは右から結合する
- 主根拠：`[expr.or-binding]`
- 補助根拠：`[expr.arithmetic-fallback]`
- 結果：正常終了 7

最初の加算が失敗すると、代替値の加算とその失敗処理を評価する。両方の加算が失敗し、最後の7になる。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = top + 1 or top + 1 or 7
    return result
}
```

### expr-078 ポインタの読み出しと更新対象の指定
- 主根拠：`[arena.ptr-operations]`
- 結果：正常終了

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
