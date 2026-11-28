# 字句

[目次と書式](../spec-cases.md)

### lex-001 コメント内に不正なUTF-8のバイトを含む
- 主根拠：`[src.encoding]`
- 結果：コンパイルエラー `[src.encoding]` 2:3

**入力：** 2行目のコメント記号//の直後に、文字ではなく生の1バイトffを入れる。文字列「FF」を書く例ではない。
**扱い：** ffはUTF-8として解釈できない。コメントとして読み飛ばす場所にあってもファイル全体がUTF-8であるという条件に反するため拒否する。mainは実行しない。

**入力図：**

```text
fn main() -> u8 { return 0 }<LF>
//<FF><LF><EOF>
```

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 20 72 65 74 75 72 6e 20 30 20 7d 0a 2f 2f ff
0a
```

### lex-002 CRLFの改行を使う
- 主根拠：`[src.newline]`
- 結果：正常終了

**入力：** 3行のプログラムを、各行末にCRとLFを1つずつ付けて保存する。
**扱い：** 各CRLFをLFと同じ改行として扱う。文の区切りは通常どおり認識され、mainが0を返す。

**入力図：**

```text
fn main() -> u8 {<CR><LF>
    return 0<CR><LF>
}<CR><LF><EOF>
```

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 0d 0a 20 20 20 20 72 65 74 75 72 6e 20 30 0d
0a 7d 0d 0a
```

### lex-003 CRLFではない単独のCRを含む
- 主根拠：`[src.newline]`
- 結果：コンパイルエラー `[src.newline]` 1:29

**入力：** lex-011と同じソースの最後にCRだけを追加する。CRの直後にLFはなく、そこでファイルが終わる。
**扱い：** このCRはCRLFの一部ではないため、不正な改行として拒否する。mainは実行しない。

**入力図：**

```text
fn main() -> u8 { return 0 }<CR><EOF>
```

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 20 72 65 74 75 72 6e 20 30 20 7d 0d
```

### lex-004 演算子の後のコメントを挟んで式を継続する
- 主根拠：`[src.comment]`
- 補助根拠：`[src.line-continuation]`
- 結果：正常終了 3

コメントを読み飛ばした後も、+の直後の改行として式を継続する。

```suz
fn main() -> u8 {
    fix value: u8 = 1 + // 次の行も加算の右辺
        2
    return value
}
```

### lex-005 キーワードを変数名にする
- 主根拠：`[src.ident]`
- 補助根拠：`[src.keywords]`、`[lit.float-special]`
- 結果：コンパイルエラー `[src.ident]` 2:9

nanは予約された浮動小数点リテラルであり、変数名に使えない。

```suz
fn main() -> u8 {
    fix nan: u8 = 1
    return 0
}
```

### lex-006 組み込み型名を変数名にする
- 主根拠：`[src.ident]`
- 補助根拠：`[src.type-names]`
- 結果：コンパイルエラー `[src.ident]` 2:9

u8は組み込み型名として予約されている。

```suz
fn main() -> u8 {
    fix u8 = 1
    return 0
}
```

### lex-007 as%の途中に空白を入れる
- 主根拠：`[src.whitespace]`
- 補助根拠：`[expr.as-trunc]`
- 結果：コンパイルエラー `[syntax.grammar]` 2:24

字句規則は付録A.2。
as%は隣接した1つのトークン。asと%に分けた形は変換の構文にならない。

```suz
fn main() -> u8 {
    fix value = 257 as % u8
    return 0
}
```

### lex-008 +%=を1つの複合代入演算子として読む
- 主根拠：`[src.longest-match]`
- 補助根拠：`[assign.compound]`、`[expr.wrap]`
- 結果：正常終了

255に1を加えるラップ演算の結果0を代入する。

```suz
fn main() -> u8 {
    var count: u8 = 255
    count +%= 1
    return count
}
```

### lex-039 負数との比較は小なりとマイナスを空白で分ける
- 主根拠：`[src.longest-match]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix x: i32 = -2
    if x < -1 { return 0 }
    return 1
}
```

### lex-009 infoを通常の識別子とログレベルに使う
- 主根拠：`[src.log-level-words]`
- 補助根拠：`[log.output]`
- 結果：正常終了

logの直後のinfoはログレベル、埋め込み式のinfoはローカル変数を表す。

```suz
fn main() -> u8 {
    fix info: u8 = 3
    log info "${info}"
    return 0
}
```

**標準エラー：**

```text
info lex-009.suz:3: 3
```

### lex-010 同じ行に初期化文とreturnを書く
- 主根拠：`[src.statement-end]`
- 結果：コンパイルエラー `[src.statement-end]` 2:23

省略できるのは閉じる波括弧の直前の改行。初期化文とreturnの間には改行が必要。

```suz
fn main() -> u8 {
    fix value: u8 = 1 return value
}
```

### lex-011 ファイル末尾の改行を省略する
- 主根拠：`[src.final-newline]`
- 補助根拠：`[src.statement-end]`
- 結果：正常終了

**入力：** 閉じる波括弧の直後でファイルを終える。最後にEnterを押して改行を追加しない形。
**扱い：** 最後の改行は必須ではないため、このままプログラムとして受け付ける。mainが0を返す。

**入力図：**

```text
fn main() -> u8 { return 0 }<EOF>
```

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 20 72 65 74 75 72 6e 20 30 20 7d
```

### lex-012 orと代入演算子の直後の改行
- 主根拠：`[src.line-continuation]`
- 結果：正常終了 7

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count =
        raw as u8 or
            7
    return count
}
```

### lex-013 丸括弧内では次の行のorも同じ式に含まれる
- 主根拠：`[src.line-continuation]`
- 補助根拠：`[src.or-line]`、`[expr.cast-fallback]`
- 結果：正常終了 7

最も内側の開き括弧が丸括弧なので、orの前の改行で初期化文は終わらない。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count = (
        raw as u8
        or 7
    )
    return count
}
```

### lex-014 呼び出しの丸括弧内でもexprのブロックは改行で文を区切る
- 主根拠：`[src.line-continuation]`
- 補助根拠：`[src.trailing-comma]`、`[block.value]`
- 結果：正常終了 3

exprの中では最も内側の開き括弧が波括弧になる。初期化文と末尾の式を改行で区切る。

```suz
fn identity(value: u8) -> u8 {
    return value
}

fn main() -> u8 {
    return identity(expr {
        fix base: u8 = 2
        base + 1
    })
}
```

### lex-015 orを次の行の先頭に置く
- 主根拠：`[src.or-line]`
- 結果：コンパイルエラー `[src.or-line]` 4:5

改行で初期化文が終わり、orは継続にならない。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix value = raw as u8
    or 0
    return 0
}
```

### lex-016 関数の実引数の間のカンマを省略する
- 主根拠：`[src.list-comma]`
- 結果：コンパイルエラー `[src.list-comma]` 8:9

丸括弧内の改行は引数の区切りにならない。1と2の間にカンマが必要。

```suz
fn sum(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    return sum(
        1
        2,
    )
}
```

### lex-017 多行の引数と末尾カンマ
- 主根拠：`[src.trailing-comma]`
- 補助根拠：`[src.line-continuation]`
- 結果：正常終了 3

```suz
fn first(
    left: u8,
    right: u8,
) -> u8 {
    return left
}

fn main() -> u8 {
    return first(
        3,
        4,
    )
}
```

### lex-018 1行の引数リストの末尾カンマ
- 主根拠：`[src.trailing-comma]`
- 結果：コンパイルエラー `[src.trailing-comma]` 3:11

1行のリストの末尾カンマは禁止。

```suz
fn use(value: u8) -> void {}
fn main() -> u8 {
    use(1,)
    return 0
}
```

### lex-019 複数行の構造体フィールドの末尾カンマ漏れ
- 主根拠：`[src.trailing-comma]`
- 結果：コンパイルエラー `[src.trailing-comma]` 3:1

複数行のフィールドリストには末尾カンマが必要。

```suz
struct Point {
    x: i32
}
fn main() -> u8 { return 0 }
```

### lex-020 最後の実引数と別の行で閉じる多行リストのカンマ漏れ
- 主根拠：`[src.trailing-comma]`
- 結果：コンパイルエラー `[src.trailing-comma]` 9:5

閉じる丸括弧が最後の実引数と別の行にあるので、2の後に末尾カンマが必要。

```suz
fn sum(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    return sum(
        1,
        2
    )
}
```

### lex-021 最後の実引数と同じ行で閉じる多行リスト
- 主根拠：`[src.trailing-comma]`
- 結果：正常終了 3

閉じる丸括弧が最後の実引数と同じ行にあるので、末尾カンマを省略できる。

```suz
fn sum(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    return sum(
        1,
        2)
}
```

### lex-022 整数リテラルの桁区切りを連続させる
- 主根拠：`[lit.integer]`
- 結果：コンパイルエラー `[lit.integer]` 2:17

数字の間に置ける_は1つだけ。

```suz
fn main() -> u8 {
    fix value = 1__000
    return 0
}
```

### lex-023 整数リテラルの末尾に桁区切りを書く
- 主根拠：`[lit.integer]`
- 結果：コンパイルエラー `[lit.integer]` 2:17

_は数字の間に置く必要がある。

```suz
fn main() -> u8 {
    fix value = 1000_
    return 0
}
```

### lex-024 小数点を含むリテラルに指数と桁区切りを付ける
- 主根拠：`[lit.float]`
- 結果：正常終了

指数の符号は省略でき、+と-も書ける。整数部・小数部・指数部の数字間にある桁区切りは、値に影響しない。

```suz
fn main() -> u8 {
    fix implicit = 1_2.5e1
    fix positive = 1.2_5e+0_2
    fix negative = 1.2_5e-0_2
    return ifexpr implicit == 125.0 && positive == 125.0 && negative == 0.0125 { 0 } else { 1 }
}
```

### lex-025 小数点の後の数字を省略する
- 主根拠：`[lit.float]`
- 結果：コンパイルエラー `[lit.float]` 2:17

小数点の両側に数字が必要。

```suz
fn main() -> u8 {
    fix value = 1.
    return 0
}
```

### lex-026 小数点なしの数値に指数を付ける
- 主根拠：`[lit.float]`
- 結果：コンパイルエラー `[lit.float]` 2:17

指数を付ける場合も小数点が必要。1.0e3と書く。

```suz
fn main() -> u8 {
    fix value = 1e3
    return 0
}
```

### lex-027 符号付き整数の最小値と桁区切り
- 主根拠：`[lit.negative]`
- 補助根拠：`[lit.integer]`、`[lit.range]`
- 結果：正常終了

整数リテラルに直接付けたマイナスを含めて範囲検査するため、i8の-128を書ける。桁区切りの_は数値を変えない。

```suz
fn main() -> u8 {
    fix minimum: i8 = -128
    fix separated: u16 = 1_234
    return ifexpr minimum == -128 && separated == 1234 { 0 } else { 1 }
}
```

### lex-028 符号付き整数の最小値より小さいリテラル
- 主根拠：`[lit.negative]`
- 補助根拠：`[lit.range]`
- 結果：コンパイルエラー `[lit.range]` 2:21

符号を含めて検査しても、-129はi8の範囲に収まらない。

```suz
fn main() -> u8 {
    fix value: i8 = -129
    return 0
}
```

### lex-029 整数リテラルの範囲外
- 主根拠：`[lit.range]`
- 結果：コンパイルエラー `[lit.range]` 2:21

256はu8に収まらない。

```suz
fn main() -> u8 {
    fix value: u8 = 256
    return 0
}
```

### lex-030 浮動小数点の丸めと特殊値
- 主根拠：`[lit.float-rounding]`
- 補助根拠：`[lit.float-negative-zero]`、`[expr.nan-comparison]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix rounded: f32 = 16_777_217.0
    fix tiny: f32 = -1.0e-50
    fix negative_inf = -inf
    fix not_number = nan
    fix infinity = inf
    log info "${rounded} ${tiny} ${not_number} ${infinity} ${negative_inf}"
    return ifexpr nan != nan && not (nan == nan) { 0 } else { 1 }
}
```

**標準エラー：**

```text
info lex-030.suz:7: 16777216.0 -0.0 nan inf -inf
```

### lex-031 浮動小数点リテラルの範囲外
- 主根拠：`[lit.float-range]`
- 結果：コンパイルエラー `[lit.float-range]` 2:22

f32への丸めの結果が無限大になる。

```suz
fn main() -> u8 {
    fix value: f32 = 1.0e40
    return 0
}
```

### lex-032 f32の最大値付近でリテラルとasを揃える
- 主根拠：`[lit.float-range]`
- 補助根拠：`[const.cast-check]`、`[rt.as-float]`
- 結果：正常終了

```suz
fix NEAR_MAX: f64 = 3.4028235e38

fn main() -> u8 {
    fix literal: f32 = 3.4028235e38
    fix converted = NEAR_MAX as f32
    return ifexpr literal == converted { 0 } else { 1 }
}
```

### lex-033 文字リテラルとUnicodeエスケープを同じ値として扱う
- 主根拠：`[char.literal]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix same = \é == \u{E9}
    fix kana = \あ
    fix emoji = \u{1f600}
    log info "${same} ${kana} ${emoji}"
    fix brace = \}
    fix quote = \'
    fix slash = \\
    log info "${brace} ${quote} ${slash}"
    return 0
}
```

**標準エラー：**

```text
info lex-033.suz:5: true あ 😀
info lex-033.suz:9: } ' \
```

### lex-034 見た目が1文字でも複数のスカラー値なら拒否する
- 主根拠：`[char.literal]`
- 結果：コンパイルエラー `[char.literal]` 2:14

```suz
fn main() -> u8 {
    fix ch = \é
    return 0
}
```

### lex-035 文字リテラルのサロゲート値を拒否する
- 主根拠：`[char.literal]`
- 結果：コンパイルエラー `[char.literal]` 2:14

```suz
fn main() -> u8 {
    fix ch = \u{D800}
    return 0
}
```

### lex-041 名前つき文字リテラル・\u単独・単独結合文字と直後のトークン
- 主根拠：`[char.literal]`
- 補助根拠：`[src.comment]`
- 結果：正常終了

```suz
fn main() -> u8 {
    fix a = (\a)
    if a!=\u{61} || not (a==\a) { return 1 }
    if (\nul as u32) != 0 || (\tab as u32) != 9 { return 2 }
    if (\newline as u32) != 10 || (\return as u32) != 13 || (\space as u32) != 32 { return 3 }
    if \n != \u{6e} || \u != \u{75} { return 4 }
    if (\́ as u32) != 769 { return 5 }
    fix slash = \/// comment
    if slash != \u{2f} { return 6 }
    return 0
}
```

### lex-042 定義されていない名前の文字リテラル
- 主根拠：`[char.literal]`
- 結果：コンパイルエラー `[char.literal]` 2:14

```suz
fn main() -> u8 {
    fix ch = \ab
    return 0
}
```

### lex-044 バックスラッシュ直後の空白は文字リテラルとして拒否する
- 主根拠：`[char.literal]`
- 結果：コンパイルエラー `[char.literal]` 2:15

```suz
fn main() -> u8 {
    fix ch = (\ )
    return 0
}
```

### lex-046 入力の末尾のバックスラッシュは文字リテラルとして拒否する
- 主根拠：`[char.literal]`
- 結果：コンパイルエラー `[char.literal]` 2:14

**入力図：**

```text
fn main() -> u8 {<LF>
    fix ch = \<EOF>
```

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20 7b 0a
20 20 20 20 66 69 78 20 63 68 20 3d 20 5c
```

### lex-036 整数リテラルの正確な表現と明示的な丸め
- 主根拠：`[lit.integer-as-float]`
- 補助根拠：`[arith.cast]`、`[arith.compare]`
- 結果：正常終了

2^24を超えても、f32で正確に表せる整数は受理する。丸めたい場合は小数点かasを明記する。比較では整数リテラルを丸めない。

```suz
fn main() -> u8 {
    fix base: f32 = 16777216
    fix next: f32 = 16_777_218
    fix wide: f64 = 9007199254740992
    fix rounded: f32 = 16777217.0
    fix converted = 16777217 as f32
    if base != rounded || base != converted { return 1 }
    if not (base < 16777217 < next) { return 2 }
    if not (wide < 9007199254740993) { return 3 }
    return 0
}
```

### lex-037 浮動小数点型で正確に表せない整数リテラル
- 主根拠：`[lit.integer-as-float]`
- 結果：コンパイルエラー `[lit.integer-as-float]` 2:22

```suz
fn main() -> u8 {
    fix value: f32 = 16777217
    return 0
}
```

### lex-038 型引数の推論でも整数リテラルを黙って丸めない
- 主根拠：`[lit.integer-as-float]`
- 補助根拠：`[generic.inference]`
- 結果：コンパイルエラー `[lit.integer-as-float]` 6:19

```suz
fn first[T](a: T, b: T) -> T {
    return a
}
fn main() -> u8 {
    fix x: f32 = 2.0
    fix y = first(16777217, x)
    return 0
}
```

### lex-040 テンプレートの外にバッククォートを書く
- 主根拠：`[syntax.grammar]`
- 結果：コンパイルエラー `[syntax.grammar]` 2:13

```suz
fn main() -> u8 {
    fix x = `a`
    return 0
}
```

### lex-047 テンプレート外のシングルクォートはコンパイルエラーになる
- 主根拠：`[syntax.grammar]`
- 補助根拠：`[char.literal]`
- 結果：コンパイルエラー `[syntax.grammar]` 2:14

```suz
fn main() -> u8 {
    fix ch = 'a'
    return 0
}
```
