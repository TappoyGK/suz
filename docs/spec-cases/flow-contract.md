# 仕様例：関数と制御（契約）

### flow-123 関数の事前条件・事後条件と範囲推定の連携
- 主根拠：`[contract.decl]`
- 補助根拠：`[contract.expects]`、`[contract.ensures]`、`[arith.width]`、`[expr.cast-risk]`
- 結果：正常終了

`@expects(1 <= n <= 100)` と `@ensures(0 <= return <= 200)` を宣言すると、本体内の `(n * 2) as u8` で `or` を省略でき、呼び出し元でも戻り値の範囲を引き継いで `u8` への `as` の `or` を省略できる。

```suz
@expects(1 <= n <= 100)
@ensures(0 <= return <= 200)
fn scale(n: i32) -> i32 {
    return (n * 2) as u8
}

fn main() -> u8 {
    fix r = scale(50)
    fix out: u8 = r as u8
    if out == 100 {
        return 0
    }
    return 1
}
```

### flow-124 mainに契約を付けると拒否する
- 主根拠：`[contract.decl]`
- 結果：コンパイルエラー `[contract.decl]` 1:1

`main` 関数には `@expects`・`@ensures` を付けられない。

```suz
@ensures(return == 0)
fn main() -> u8 {
    return 0
}
```

### flow-125 構造体に契約を付けると拒否する
- 主根拠：`[contract.decl]`
- 結果：コンパイルエラー `[contract.decl]` 2:1

構造体宣言には `@expects`・`@ensures` を付けられない。

```suz
@expects(true)
struct Box {
    value: i32,
}

fn main() -> u8 {
    return 0
}
```

### flow-126 fnshapeに契約を付けると拒否する
- 主根拠：`[contract.decl]`
- 結果：コンパイルエラー `[contract.decl]` 2:1

`fnshape` 宣言には `@expects`・`@ensures` を付けられない。

```suz
@expects(true)
fnshape Callback(i32) -> i32

fn main() -> u8 {
    return 0
}
```

### flow-127 externに契約を付けると拒否する
- 主根拠：`[contract.decl]`
- 結果：コンパイルエラー `[contract.decl]` 2:1

`extern` ブロックには `@expects`・`@ensures` を付けられない。

```suz
@expects(true)
extern "stdlib.h" {
    fn abs(x: _int) -> _int
}

fn main() -> u8 {
    return 0
}
```

### flow-128 契約と関数宣言の間の空行を拒否する
- 主根拠：`[contract.decl]`
- 補助根拠：`[generic.needs-adjacent]`
- 結果：コンパイルエラー `[contract.decl]` 2:1

契約の行と対象関数の間に空行を置くとコンパイルエラーとなる。

```suz
@expects(n > 0)

fn pos(n: i32) -> i32 {
    return n
}

fn main() -> u8 {
    return 0
}
```

### flow-129 契約式に関数呼び出しを書くと拒否する
- 主根拠：`[contract.expr]`
- 結果：コンパイルエラー `[contract.expr]` 5:10

契約式には通常の関数・メソッド呼び出しを書けない。

```suz
fn ok(n: i32) -> bool {
    return n > 0
}

@expects(ok(n))
fn use_n(n: i32) -> i32 {
    return n
}

fn main() -> u8 {
    return 0
}
```

### flow-130 契約式で非零保証のない除算を拒否する
- 主根拠：`[contract.expr]`
- 結果：コンパイルエラー `[contract.expr]` 1:16

除数が `0` になり得る除算は契約式に書けない。

```suz
@expects(total / n < 10)
fn ratio(total: i32, n: i32) -> i32 {
    return 0
}

fn main() -> u8 {
    return 0
}
```

### flow-131 契約式で浮動小数点算術を書くと拒否する
- 主根拠：`[contract.expr]`
- 結果：コンパイルエラー `[contract.expr]` 1:1

契約式に浮動小数点算術は書けない。

```suz
@expects(x + 1.0 > 0.0)
fn f(x: f64) -> u8 {
    return 0
}

fn main() -> u8 {
    return 0
}
```

### flow-133 void関数の@ensuresでreturnを参照すると拒否する
- 主根拠：`[contract.return]`
- 補助根拠：`[contract.expr]`
- 結果：コンパイルエラー `[contract.return]` 1:10

`void` を返す関数の `@ensures` では `return` を参照できない。

```suz
@ensures(return == 0)
fn noop() -> void {
    return
}

fn main() -> u8 {
    noop()
    return 0
}
```

### flow-149 @expects内でreturnを参照すると拒否する
- 主根拠：`[contract.return]`
- 結果：コンパイルエラー `[contract.return]` 1:10

`return` は `@ensures` 内でのみ参照でき、`@expects` 内で参照するとコンパイルエラーとなる。

```suz
@expects(return > 0)
fn f(n: i32) -> i32 {
    return n
}

fn main() -> u8 {
    return 0
}
```

### flow-132 契約式の&&左辺で非零を保証すれば右辺の除算を許可する
- 主根拠：`[contract.expr-order]`
- 補助根拠：`[contract.expr]`、`[contract.expects]`、`[contract.proof]`
- 結果：正常終了

`@expects(n != 0 && total / n < 10)` のように短絡評価の左辺で非零を保証すれば右辺の除算を許可し、呼び出し側で同じ条件をガードすれば受理する。

```suz
@expects(n != 0 && total / n < 10)
fn ratio(total: i32, n: i32) -> i32 {
    return 1
}

fn check(total: i32, n: i32) -> u8 {
    if n != 0 && total / n < 10 {
        if ratio(total, n) == 1 {
            return 0
        }
    }
    return 1
}

fn main() -> u8 {
    return check(20, 4)
}
```

### flow-134 @expectsの配列境界条件で本体内の添字orを省略する
- 主根拠：`[contract.expects]`
- 補助根拠：`[array.bounds-proof]`
- 結果：正常終了

`@expects(0 <= idx < len buf)` や `@expects(len dst == len src)` により、関数本体内の配列読み書きで `or` を省略できる。

```suz
@expects(0 <= idx < len buf)
fn read_at(buf: .[u8], idx: i32) -> u8 {
    return buf.[idx]
}

@expects(len dst == len src)
fn copy_all(dst: #[u8], src: .[u8]) -> void {
    for i in 0..<len src {
        dst#[i] = src.[i]
    }
}

fn main() -> u8 {
    fix src: .[u8] = .[10, 20, 30]
    fix dst: #[u8] = new #[0; 3]
    if len dst == len src {
        copy_all(dst, src)
    }
    fix idx: i32 = 2
    if 0 <= idx < len dst {
        if read_at(dst, idx) == 30 {
            return 0
        }
    }
    return 1
}
```

### flow-135 ガードなしで動的な値を@expects付き関数へ渡すと拒否する
- 主根拠：`[contract.expects]`
- 結果：コンパイルエラー `[contract.expects]` 7:12

動的な値 `n` を `if` ガードなしで `@expects(1 <= n <= 100)` の関数へ渡すとコンパイルエラーとなる。

```suz
@expects(1 <= n <= 100)
fn scale(n: i32) -> u8 {
    return (n * 2) as u8
}

fn run(n: i32) -> u8 {
    return scale(n)
}

fn main() -> u8 {
    return run(10)
}
```

### flow-136 実引数の評価中に参照先が変わり得る場合は先のガードで@expectsを証明しない
- 主根拠：`[contract.expects]`
- 補助根拠：`[contract.proof]`、`[array.bounds-proof]`
- 結果：コンパイルエラー `[contract.expects]` 18:16

`if v.active` の後に `use_voice(v, disable(v))` を呼ぶと、第 2 引数 `disable(v)` の評価で共有参照先の条件が無効化されるため、コンパイルエラーとなる。

```suz
struct Voice {
    active: bool,
}

fn disable(v: mut Voice) -> i32 {
    v#active = false
    return 0
}

@expects(v.active)
fn use_voice(v: ref Voice, tag: i32) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = new Voice { active: true }
    if v.active {
        return use_voice(v, disable(v))
    }
    return 0
}
```

### flow-137 return前のifガードと代替値で@ensuresを満たす
- 主根拠：`[contract.ensures]`
- 結果：正常終了

動的な計算結果を返す場合でも、`return` 前の `if` ガードと範囲内の代替値によって `@ensures` を静的に満たせる。

```suz
@ensures(0 <= return <= 200)
fn clamp_even(x: i32) -> i32 {
    if 0 <= x <= 200 {
        return x
    }
    return 0
}

fn main() -> u8 {
    fix a = clamp_even(42)
    fix b = clamp_even(999)
    fix ua: u8 = a as u8
    fix ub: u8 = b as u8
    if ua == 42 && ub == 0 {
        return 0
    }
    return 1
}
```

### flow-138 失敗する関数のfail経路では@ensuresを検査しない
- 主根拠：`[contract.ensures]`
- 結果：正常終了

失敗する関数（`-> i32 or Fail`）では正常 `return` 経路のみ `@ensures` を満たせばよく、`fail` 経路は検査しない。

```suz
error Fail { Bad }

@ensures(return > 0)
fn positive(x: i32) -> i32 or Fail {
    if x > 0 {
        return x
    }
    fail Fail.Bad
}

fn main() -> u8 {
    fix v = positive(-1) or 9
    if v == 9 {
        return 0
    }
    return 1
}
```

### flow-139 @ensuresを証明できないreturnを拒否する
- 主根拠：`[contract.ensures]`
- 結果：コンパイルエラー `[contract.ensures]` 3:12

`@ensures(0 <= return <= 200)` を満たすことを証明できない `return` はコンパイルエラーとなる。

```suz
@ensures(0 <= return <= 200)
fn bad(x: i32) -> i32 {
    return x
}

fn main() -> u8 {
    return 0
}
```

### flow-140 or returnで束縛した呼び出し結果へ@ensuresを持ち越す
- 主根拠：`[contract.ensures-propagate]`
- 補助根拠：`[contract.expects]`
- 結果：正常終了

失敗時に脱出する `or return` で束縛した場合、後続の経路は成功確定のため `@ensures` の条件を呼び出し元へ持ち越せる。

```suz
error Fail { Bad }

@ensures(return > 0)
fn make_pos(x: i32) -> i32 or Fail {
    if x > 0 {
        return x
    }
    fail Fail.Bad
}

@expects(n > 0)
fn need_pos(n: i32) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = make_pos(5) or return 1
    return need_pos(v)
}
```

### flow-141 失敗する関数のor代替値合流後は@ensuresを持ち越さない
- 主根拠：`[contract.ensures-propagate]`
- 補助根拠：`[contract.expects]`
- 結果：コンパイルエラー `[contract.expects]` 18:12

`make_pos(5) or 0` のように代替値へ合流すると `@ensures(return > 0)` は持ち越さず、後続の `@expects(n > 0)` はコンパイルエラーとなる。

```suz
error Fail { Bad }

@ensures(return > 0)
fn make_pos(x: i32) -> i32 or Fail {
    if x > 0 {
        return x
    }
    fail Fail.Bad
}

@expects(n > 0)
fn need_pos(n: i32) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = make_pos(5) or 0
    return need_pos(v)
}
```

### flow-142 一般のbool状態条件・比較の反転・notと&&の分解で@expectsを証明する
- 主根拠：`[contract.proof]`
- 補助根拠：`[contract.expects]`
- 結果：正常終了

`if v.active`、比較の反転（`100 >= n`）、早期退出の `not`（`if not (n >= 1) { return 1 }`）、`&&` の分解によって `@expects` を証明できる。

```suz
struct Voice {
    active: bool,
}

@expects(v.active)
fn play(v: ref Voice) -> u8 {
    return 0
}

@expects(1 <= n && n <= 100)
fn use_range(n: i32) -> u8 {
    return 0
}

fn check(n: i32) -> u8 {
    if not (n >= 1) {
        return 1
    }
    if 100 >= n {
        return use_range(n)
    }
    return 1
}

fn main() -> u8 {
    fix v = new Voice { active: true }
    if v.active {
        if play(v) == 0 && check(50) == 0 {
            return 0
        }
    }
    return 1
}
```

### flow-143 配列要素を更新しても配列長lenの保証は失われない
- 主根拠：`[contract.proof]`
- 補助根拠：`[array.bounds-proof]`、`[contract.expects]`
- 結果：正常終了

配列の要素へ書き込んでも配列参照の `len` に関する条件は保持されるため、後続の `@expects(len dst == len src)` 呼び出しが受理される。

```suz
@expects(len dst == len src)
fn copy_all(dst: #[u8], src: .[u8]) -> void {
    for i in 0..<len src {
        dst#[i] = src.[i]
    }
}

fn main() -> u8 {
    fix src: .[u8] = .[1, 2, 3]
    fix dst: #[u8] = new #[0; 3]
    if len dst == len src && len dst > 0 {
        dst#[0] = 9
        copy_all(dst, src)
        if dst.[0] == 1 {
            return 0
        }
    }
    return 1
}
```

### flow-144 共有参照先に依存する条件は関数呼び出しをまたぐと忘れる
- 主根拠：`[contract.proof]`
- 補助根拠：`[array.bounds-proof]`、`[contract.expects]`
- 結果：コンパイルエラー `[contract.expects]` 18:16

共有参照先 `v.active` の条件は、別の関数呼び出しをまたぐと忘れられるため、後続の `@expects(v.active)` 呼び出しはコンパイルエラーとなる。

```suz
struct Voice {
    active: bool,
}

fn touch() -> void {
    return
}

@expects(v.active)
fn play(v: ref Voice) -> u8 {
    return 0
}

fn main() -> u8 {
    fix v = new Voice { active: true }
    if v.active {
        touch()
        return play(v)
    }
    return 0
}
```

### flow-145 浮動小数点比較の否定で反対向きの比較を証明済みとみなさない
- 主根拠：`[contract.proof]`
- 補助根拠：`[expr.nan-comparison]`、`[contract.expects]`
- 結果：コンパイルエラー `[contract.expects]` 11:12

浮動小数点数の `x < 1.0` は `NaN` でも偽となるため、`if x < 1.0 { return 1 }` の後ろでも `x >= 1.0` は証明されずコンパイルエラーとなる。

```suz
@expects(x >= 1.0)
fn h(x: f64) -> u8 {
    return 0
}

fn main() -> u8 {
    fix x: f64 = nan
    if x < 1.0 {
        return 1
    }
    return h(x)
}
```

### flow-150 構造体リテラルを返す位置で@ensuresのreturn.フィールドを証明し呼び出し元へ引き継ぐ
- 主根拠：`[contract.proof]`
- 補助根拠：`[contract.ensures]`、`[contract.ensures-propagate]`
- 結果：正常終了

構造体リテラルを返す位置では `return.x` をそのフィールドの初期値の式として扱うため `@ensures(return.x >= 1)` を静的に証明でき、結果を束縛した変数のフィールド条件として呼び出し元へ引き継げる。

```suz
struct Pt {
    x: u8,
}

@ensures(return.x >= 1)
fn mk() -> Pt {
    return Pt { x: 1 }
}

@expects(p.x >= 1)
fn use_pt(p: Pt) -> u8 {
    return p.x - 1
}

fn main() -> u8 {
    fix p = mk()
    return use_pt(p)
}
```

### flow-151 整数比較のnotを同じ形のガードと反対向きの比較のガードで証明する
- 主根拠：`[contract.proof]`
- 補助根拠：`[contract.expects]`、`[contract.ensures]`
- 結果：正常終了

整数の比較では `not (n < 1)` と `n >= 1` が反転で一致するため、`@expects(not (n < 1))` と `@ensures(not (return < 1))` を同じ形のガードでも反対向きの比較のガードでも証明できる。

```suz
@expects(not (n < 1))
@ensures(not (return < 1))
fn keep_pos(n: i32) -> i32 {
    return n
}

fn main() -> u8 {
    fix a: i32 = 5
    fix b: i32 = 7
    if not (a < 1) {
        fix r1 = keep_pos(a)
        if b >= 1 {
            fix r2 = keep_pos(b)
            if r1 == 5 && r2 == 7 {
                return 0
            }
        }
    }
    return 1
}
```

### flow-152 比較連鎖や&&のnotを同じ形のガードで証明する
- 主根拠：`[contract.proof]`
- 補助根拠：`[contract.expects]`
- 結果：正常終了

比較連鎖や `&&` の否定は反対向きの比較へ反転せず、`not (...)` の形のまま一般 `bool` 条件として記憶・一致させるため、同じ形のガードで `@expects` を証明できる。

```suz
@expects(not (0 < n < 10))
fn outside_chain(n: i32) -> u8 {
    return 0
}

@expects(not (n > 0 && n < 10))
fn outside_and(n: i32) -> u8 {
    return 0
}

fn main() -> u8 {
    fix a: i32 = 15
    fix b: i32 = -1
    if not (0 < a < 10) {
        if outside_chain(a) == 0 && not (b > 0 && b < 10) {
            return outside_and(b)
        }
    }
    return 1
}
```

### flow-146 契約を持つ関数を関数値にすると拒否する
- 主根拠：`[contract.fn-value]`
- 結果：コンパイルエラー `[contract.fn-value]` 7:13

契約を持つ関数は `fn 名前` で関数値にできない。

```suz
@expects(n > 0)
fn pos(n: i32) -> i32 {
    return n
}

fn main() -> u8 {
    fix f = fn pos
    return 0
}
```

### flow-147 契約を持つメソッドは@needsの要求を満たさない
- 主根拠：`[contract.needs]`
- 補助根拠：`[generic.needs-check]`
- 結果：コンパイルエラー `[contract.needs]` 17:5

契約を持つメソッドは `@needs` を経由した無検査呼び出しを防ぐため、`@needs` の要求を満たさない。

```suz
struct Boxed {
    value: i32,
}

@expects(x > 0)
fn Boxed#set(x: i32) -> void {
    self#value = x
}

@needs T: #set(i32) -> void
fn apply[T](target: mut T) -> void {
    target#set(10)
}

fn main() -> u8 {
    fix b = new Boxed { value: 0 }
    apply(b)
    return 0
}
```

### flow-148 契約を持つ演算子メソッドは@needsの要求を満たさない
- 主根拠：`[contract.needs]`
- 補助根拠：`[generic.needs-check]`
- 結果：コンパイルエラー `[contract.needs]` 18:13

契約を持つ演算子メソッドも `@needs` の要求を満たさない。

```suz
struct Num {
    v: i32,
}

@expects(other.v != 0)
fn Num./(other: Num) -> Num {
    return Num { v: self.v }
}

@needs T: /(T) -> T
fn div_pair[T](a: T, b: T) -> T {
    return a / b
}

fn main() -> u8 {
    fix a = Num { v: 10 }
    fix b = Num { v: 2 }
    fix c = div_pair(a, b)
    return 0
}
```
