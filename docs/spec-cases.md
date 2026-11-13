# Suz v0.1 仕様例集

[言語仕様](spec.md)に対する受理・拒否・実行結果の確認例。期待結果は仕様から導いたものであり、Suzコンパイラでの実行結果ではない。

## 分類とID

各例には、何を確かめる例なのかを示す主根拠を1つ付け、その規則が属する仕様の章に配置する。関連するほかの規則は補助根拠として記載する。

章は仕様の順に並べ、章内は主根拠の掲載順を基本とする。同じ主根拠の例は、基本的な動作、境界条件、ほかの機能との組み合わせの順にまとめ、対応する受理例とエラー例を近くに置く。

IDは分野の接頭辞と番号で構成し、期待結果の種類を含めない。今回の整理後はIDを固定する。例を追加するときは、その分野の未使用の次番号を採番して関連例の近くへ置き、既存例は改番しない。掲載順と番号順が異なってもよい。

## 読み方と保存方法

通常のソースブロックは、それぞれ独立した1ファイルである。IDをファイル名にし、たとえば `prog-001` は `prog-001.suz` として、コードブロックの先頭を1行目に保存する。改行はLFとする。バイト列を指定した例は、そのバイト列をそのまま保存する。

通常設定での結果を基本とし、診断設定を比較する例には別途指定する。通常終了する例の標準出力は空。標準エラーは記載したログだけで、ログがない例では空。コンパイルエラーの終了コードや診断文言、パニックの診断文言は仕様で固定されていないため、理由と位置を確認する。

バイト列の入力図では、`<CR>`は復帰文字（0d）、`<LF>`は改行文字（0a）、`<FF>`は不正なUTF-8の1バイト（ff）を表す。`<EOF>`はファイルの終端であり、文字やバイトではない。図中の改行は表示のためで、ファイルに含める改行は記号で示したものだけ。

保存用の16進数は、空白と改行を取り除き、2桁を1バイトに変換して各IDの `.suz` ファイルへ保存する。

## 目次と件数

| 分野 | ID接頭辞 | 対応する仕様 | 件数 |
|---|---|---|---:|
| [字句](#3-字句) | `lex` | 第3章 | 32 |
| [プログラムの構造](#4-プログラムの構造) | `prog` | 第4章 | 13 |
| [型](#5-型) | `type` | 第5章 | 29 |
| [定数・変数・代入](#6-定数変数代入) | `bind` | 第6章 | 29 |
| [式](#7-式) | `expr` | 第7章 | 65 |
| [関数と制御](#8-関数と制御) | `flow` | 第8章 | 56 |
| [実行時の検査](#9-実行時の検査) | `runtime` | 第9章 | 18 |
| [ログ](#10-ログ) | `log` | 第10章 | 10 |
| [整数演算の診断設定](#11-整数演算の診断設定) | `diag` | 第11章 | 8 |
| 合計 | | | 260 |

結果別：受理102例、コンパイルエラー135例、実行時パニック15例、診断設定の比較8例。バイト列による4例も、それぞれの期待結果に含めている。

## 3. 字句

### lex-001 コメント内に不正なUTF-8のバイトを含む

主根拠：`[src.encoding]`。

**入力：** 2行目のコメント記号//の直後に、文字ではなく生の1バイトffを入れる。文字列「FF」を書く例ではない。

```text
fn main() -> u8 { return 0 }<LF>
//<FF><LF><EOF>
```

**扱い：** ffはUTF-8として解釈できない。コメントとして読み飛ばす場所にあってもファイル全体がUTF-8であるという条件に反するため拒否する。mainは実行しない。

**期待：** コンパイルエラー。

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 20 72 65 74 75 72 6e 20 30 20 7d 0a 2f 2f ff
0a
```

### lex-002 CRLFの改行を使う

主根拠：`[src.newline]`。

**入力：** 3行のプログラムを、各行末にCRとLFを1つずつ付けて保存する。

```text
fn main() -> u8 {<CR><LF>
    return 0<CR><LF>
}<CR><LF><EOF>
```

**扱い：** 各CRLFをLFと同じ改行として扱う。文の区切りは通常どおり認識され、mainが0を返す。

**期待：** 受理。終了コード 0。

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 0d 0a 20 20 20 20 72 65 74 75 72 6e 20 30 0d
0a 7d 0d 0a
```

### lex-003 CRLFではない単独のCRを含む

主根拠：`[src.newline]`。

**入力：** lex-011と同じソースの最後にCRだけを追加する。CRの直後にLFはなく、そこでファイルが終わる。

```text
fn main() -> u8 { return 0 }<CR><EOF>
```

**扱い：** このCRはCRLFの一部ではないため、不正な改行として拒否する。mainは実行しない。

**期待：** コンパイルエラー。

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 20 72 65 74 75 72 6e 20 30 20 7d 0d
```

### lex-004 演算子の後のコメントを挟んで式を継続する

主根拠：`[src.comment]`。

補助根拠：`[src.line-continuation]`。

**期待：** 受理。終了コード 3。

コメントを読み飛ばした後も、+の直後の改行として式を継続する。

```suz
fn main() -> u8 {
    fix value: u8 = 1 + // 次の行も加算の右辺
        2
    return value
}
```

### lex-005 キーワードを変数名にする

主根拠：`[src.ident]`。

補助根拠：`[src.keywords]`、`[lit.float-special]`。

**期待：** コンパイルエラー。

nanは予約された浮動小数点リテラルであり、変数名に使えない。

```suz
fn main() -> u8 {
    fix nan: u8 = 1
    return 0
}
```

### lex-006 組み込み型名を変数名にする

主根拠：`[src.ident]`。

補助根拠：`[src.type-names]`。

**期待：** コンパイルエラー。

u8は組み込み型名として予約されている。

```suz
fn main() -> u8 {
    fix u8 = 1
    return 0
}
```

### lex-007 as%の途中に空白を入れる

主根拠：`[src.whitespace]`。

補助根拠：`[expr.as-trunc]`。

字句規則は付録A.2。

**期待：** コンパイルエラー。

as%は隣接した1つのトークン。asと%に分けた形は変換の構文にならない。

```suz
fn main() -> u8 {
    fix value = 257 as % u8
    return 0
}
```

### lex-008 +%=を1つの複合代入演算子として読む

主根拠：`[src.longest-match]`。

補助根拠：`[assign.compound]`、`[expr.wrap]`。

**期待：** 受理。終了コード 0。

255に1を加えるラップ演算の結果0を代入する。

```suz
fn main() -> u8 {
    var count: u8 = 255
    count +%= 1
    return count
}
```

### lex-009 infoを通常の識別子とログレベルに使う

主根拠：`[src.info]`。

補助根拠：`[log.output]`。

**期待：** 受理。終了コード 0。

logの直後のinfoはログレベル、埋め込み式のinfoはローカル変数を表す。

```suz
fn main() -> u8 {
    fix info: u8 = 3
    log info `${info}`
    return 0
}
```

標準エラー：

```text
info lex-009.suz:3: 3
```

### lex-010 同じ行に初期化文とreturnを書く

主根拠：`[src.statement-end]`。

**期待：** コンパイルエラー。

省略できるのは閉じる波括弧の直前の改行。初期化文とreturnの間には改行が必要。

```suz
fn main() -> u8 {
    fix value: u8 = 1 return value
}
```

### lex-011 ファイル末尾の改行を省略する

主根拠：`[src.final-newline]`。

補助根拠：`[src.statement-end]`。

**入力：** 閉じる波括弧の直後でファイルを終える。最後にEnterを押して改行を追加しない形。

```text
fn main() -> u8 { return 0 }<EOF>
```

**扱い：** 最後の改行は必須ではないため、このままプログラムとして受け付ける。mainが0を返す。

**期待：** 受理。終了コード 0。

**保存用のバイト列（16進数）：**

```text
66 6e 20 6d 61 69 6e 28 29 20 2d 3e 20 75 38 20
7b 20 72 65 74 75 72 6e 20 30 20 7d
```

### lex-012 orと代入演算子の直後の改行

主根拠：`[src.line-continuation]`。

**期待：** 受理。終了コード 7。

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

主根拠：`[src.line-continuation]`。

補助根拠：`[src.or-line]`、`[expr.cast-fallback]`。

**期待：** 受理。終了コード 7。

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

主根拠：`[src.line-continuation]`。

補助根拠：`[src.trailing-comma]`、`[block.value]`。

**期待：** 受理。終了コード 3。

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

主根拠：`[src.or-line]`。

**期待：** コンパイルエラー。

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

主根拠：`[src.list-comma]`。

**期待：** コンパイルエラー。

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

主根拠：`[src.trailing-comma]`。

補助根拠：`[src.line-continuation]`。

**期待：** 受理。終了コード 3。

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

主根拠：`[src.trailing-comma]`。

**期待：** コンパイルエラー。

1行のリストの末尾カンマは禁止。

```suz
fn use(value: u8) -> void {}
fn main() -> u8 {
    use(1,)
    return 0
}
```

### lex-019 複数行の構造体フィールドの末尾カンマ漏れ

主根拠：`[src.trailing-comma]`。

**期待：** コンパイルエラー。

複数行のフィールドリストには末尾カンマが必要。

```suz
struct Point {
    x: i32
}
fn main() -> u8 { return 0 }
```

### lex-020 最後の実引数と別の行で閉じる多行リストのカンマ漏れ

主根拠：`[src.trailing-comma]`。

**期待：** コンパイルエラー。

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

主根拠：`[src.trailing-comma]`。

**期待：** 受理。終了コード 3。

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

主根拠：`[lit.integer]`。

**期待：** コンパイルエラー。

数字の間に置ける_は1つだけ。

```suz
fn main() -> u8 {
    fix value = 1__000
    return 0
}
```

### lex-023 整数リテラルの末尾に桁区切りを書く

主根拠：`[lit.integer]`。

**期待：** コンパイルエラー。

_は数字の間に置く必要がある。

```suz
fn main() -> u8 {
    fix value = 1000_
    return 0
}
```

### lex-024 小数点を含むリテラルに指数を付ける

主根拠：`[lit.float]`。

**期待：** 受理。終了コード 0。

指数の符号は省略でき、+と-も書ける。

```suz
fn main() -> u8 {
    fix implicit = 1.25e2
    fix positive = 1.25e+2
    fix negative = 1.25e-2
    return ifexpr implicit == 125.0 && positive == 125.0 && negative == 0.0125 { 0 } else { 1 }
}
```

### lex-025 小数点の後の数字を省略する

主根拠：`[lit.float]`。

**期待：** コンパイルエラー。

小数点の両側に数字が必要。

```suz
fn main() -> u8 {
    fix value = 1.
    return 0
}
```

### lex-026 小数点なしの数値に指数を付ける

主根拠：`[lit.float]`。

**期待：** コンパイルエラー。

指数を付ける場合も小数点が必要。1.0e3と書く。

```suz
fn main() -> u8 {
    fix value = 1e3
    return 0
}
```

### lex-027 符号付き整数の最小値と桁区切り

主根拠：`[lit.negative]`。

補助根拠：`[lit.integer]`、`[lit.range]`。

**期待：** 受理。終了コード 0。

整数リテラルに直接付けたマイナスを含めて範囲検査するため、i8の-128を書ける。桁区切りの_は数値を変えない。

```suz
fn main() -> u8 {
    fix minimum: i8 = -128
    fix separated: u16 = 1_234
    return ifexpr minimum == -128 && separated == 1234 { 0 } else { 1 }
}
```

### lex-028 符号付き整数の最小値より小さいリテラル

主根拠：`[lit.negative]`。

補助根拠：`[lit.range]`。

**期待：** コンパイルエラー。

符号を含めて検査しても、-129はi8の範囲に収まらない。

```suz
fn main() -> u8 {
    fix value: i8 = -129
    return 0
}
```

### lex-029 整数リテラルの範囲外

主根拠：`[lit.range]`。

**期待：** コンパイルエラー。

256はu8に収まらない。

```suz
fn main() -> u8 {
    fix value: u8 = 256
    return 0
}
```

### lex-030 浮動小数点の丸めと特殊値

主根拠：`[lit.float-rounding]`。

補助根拠：`[lit.integer-as-float]`、`[lit.float-negative-zero]`、`[expr.nan-comparison]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    fix rounded: f32 = 16_777_217
    fix tiny: f32 = -1.0e-50
    log info `${rounded} ${tiny} ${nan} ${inf} ${-inf}`
    return ifexpr nan != nan && not (nan == nan) { 0 } else { 1 }
}
```

標準エラー：

```text
info lex-030.suz:4: 16777216.0 -0.0 nan inf -inf
```

### lex-031 浮動小数点リテラルの範囲外

主根拠：`[lit.float-range]`。

**期待：** コンパイルエラー。

f32への丸めの結果が無限大になる。

```suz
fn main() -> u8 {
    fix value: f32 = 1.0e40
    return 0
}
```

### lex-032 f32の最大値付近でリテラルとasを揃える

主根拠：`[lit.float-range]`。

補助根拠：`[const.cast-check]`、`[rt.as-float]`。

**期待：** 受理。終了コード 0。

```suz
const near_max: f64 = 3.4028235e38

fn main() -> u8 {
    fix literal: f32 = 3.4028235e38
    fix converted = near_max as f32
    return ifexpr literal == converted { 0 } else { 1 }
}
```


## 4. プログラムの構造

### prog-001 最小のプログラム

主根拠：`[prog.main]`。

補助根拠：`[prog.exit]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    return 0
}
```

### prog-002 mainの戻り値型

主根拠：`[prog.main]`。

**期待：** コンパイルエラー。

mainの戻り値型はu8である。

```suz
fn main() -> i32 { return 0 }
```

### prog-003 mainが存在しない

主根拠：`[prog.main]`。

**期待：** コンパイルエラー。

入口となるmainが必要。このソース全体にはhelperだけがある。

```suz
fn helper() -> u8 {
    return 0
}
```

### prog-004 mainを2つ宣言する

主根拠：`[prog.main]`。

補助根拠：`[decl.duplicate]`。

**期待：** コンパイルエラー。

```suz
fn main() -> u8 { return 0 }
fn main() -> u8 { return 1 }
```

### prog-005 後で宣言する関数と定数を参照する

主根拠：`[decl.top-level]`。

補助根拠：`[const.initializer]`。

**期待：** 受理。終了コード 3。

mainは後にある関数を、定数answerは後にある定数baseを参照できる。

```suz
fn main() -> u8 {
    return calculate()
}

const answer: u8 = base + 1
const base: u8 = 2

fn calculate() -> u8 {
    return answer
}
```

### prog-006 トップレベルにfixを宣言する

主根拠：`[decl.location]`。

補助根拠：`[decl.top-level]`。

**期待：** コンパイルエラー。

fixはブロック内で宣言する。

```suz
fix value: u8 = 1

fn main() -> u8 { return 0 }
```

### prog-007 関数内にconstを宣言する

主根拠：`[decl.location]`。

**期待：** コンパイルエラー。

constはトップレベルで宣言する。

```suz
fn main() -> u8 {
    const value: u8 = 1
    return 0
}
```

### prog-008 定数と関数に同じ名前を付ける

主根拠：`[decl.duplicate]`。

**期待：** コンパイルエラー。

宣言の種類が違っても、同じスコープの名前は重複できない。

```suz
const value: u8 = 1

fn value() -> u8 {
    return 2
}

fn main() -> u8 { return 0 }
```

### prog-009 独立したブロックで同じ変数名を使う

主根拠：`[decl.shadowing]`。

補助根拠：`[decl.local-scope]`。

**期待：** 受理。終了コード 3。

2つのvalueのスコープは重ならない。

```suz
fn main() -> u8 {
    var total: u8 = 0
    {
        fix value: u8 = 1
        total += value
    }
    {
        fix value: u8 = 2
        total += value
    }
    return total
}
```

### prog-010 ローカル変数のシャドーイング

主根拠：`[decl.shadowing]`。

**期待：** コンパイルエラー。

内側のcountが外側の名前を隠蔽する。

```suz
fn main() -> u8 {
    fix count = 1
    {
        fix count = 2
    }
    return 0
}
```

### prog-011 ローカル変数を宣言前に参照する

主根拠：`[decl.local-scope]`。

**期待：** コンパイルエラー。

firstの初期化時には、後に宣言するsecondを参照できない。

```suz
fn main() -> u8 {
    fix first: u8 = second
    fix second: u8 = 1
    return first
}
```

### prog-012 自分自身を初期値で参照する

主根拠：`[decl.local-scope]`。

**期待：** コンパイルエラー。

valueは自分自身の初期値の式では参照できない。

```suz
fn main() -> u8 {
    fix value: u8 = value
    return value
}
```

### prog-013 ブロック終了後にローカル変数を参照する

主根拠：`[decl.local-scope]`。

**期待：** コンパイルエラー。

valueのスコープは内側のブロックの終わりまで。

```suz
fn main() -> u8 {
    {
        fix value: u8 = 1
    }
    return value
}
```


## 5. 型

### type-001 voidを返す呼び出しで変数を初期化する

主根拠：`[type.void]`。

**期待：** コンパイルエラー。

型注釈を省略しても、voidを変数の型にはできない。

```suz
fn action() -> void {}

fn main() -> u8 {
    fix value = action()
    return 0
}
```

### type-002 仮引数の型にvoidを使う

主根拠：`[type.void]`。

**期待：** コンパイルエラー。

引数を取らない関数は空の丸括弧で宣言する。void型の仮引数は宣言できない。

```suz
fn action(value: void) -> void {}

fn main() -> u8 { return 0 }
```

### type-003 構造体のフィールドの型にvoidを使う

主根拠：`[type.void]`。

**期待：** コンパイルエラー。

```suz
struct Item { value: void }

fn main() -> u8 { return 0 }
```

### type-004 型の決まった整数同士の暗黙変換

主根拠：`[type.numeric-conversion]`。

補助根拠：`[expr.operator-types]`。

**期待：** コンパイルエラー。

加算の両辺の型が異なる。

```suz
fn main() -> u8 {
    fix left: u8 = 1
    fix right: i32 = 2
    fix total = left + right
    return 0
}
```

### type-005 同型の推論とビット演算

主根拠：`[type.propagation]`。

補助根拠：`[type.mixed-literals]`、`[expr.bitwise]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    fix sum: u8 = 1 + 2
    fix bits: u8 = ~0
    fix mixed = 1 + 0.5
    return ifexpr sum == 3 && bits == 255 && mixed == 1.5 { 0 } else { 1 }
}
```

### type-006 シフトの左右に異なる整数型を使う

主根拠：`[type.propagation-boundary]`。

補助根拠：`[expr.operator-types]`。

**期待：** 受理。終了コード 8。

左辺と結果はu8、シフト数はu64。シフト数に左辺と同じ型は要求しない。

```suz
fn main() -> u8 {
    fix count: u64 = 3
    fix result: u8 = 1 << count
    return result
}
```

### type-007 キャスト先の型は変換前の加算へ伝わらない

主根拠：`[type.propagation-boundary]`。

補助根拠：`[type.literal-default]`、`[const.cast-check]`。

**期待：** 受理。終了コード 7。

255 + 1は既定のi64で計算して256になる。その後のu8への変換が失敗し、代替値7を使う。

```suz
fn main() -> u8 {
    fix result = (255 + 1) as u8 or 7
    return result
}
```

### type-008 分岐の順序によらず既知の型をリテラルへ伝える

主根拠：`[type.branch-inference]`。

補助根拠：`[type.inference-order]`。

**期待：** 受理。終了コード 3。

型注釈のないfirstとsecondもu8になる。型の決まった値が後ろの分岐にあっても同じ。

```suz
fn add(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    fix known: u8 = 2
    fix first = ifexpr true { 1 } else { known }
    fix second = ifexpr true { known } else { 1 }
    return add(first, second)
}
```

### type-009 分岐の型が決まっている整数同士は統一しない

主根拠：`[type.branch-inference]`。

補助根拠：`[type.inference-order]`。

**期待：** コンパイルエラー。

u8とu16の値は、数値が同じでも分岐の結果型を統一できない。選ばれない分岐も型検査する。

```suz
fn main() -> u8 {
    fix small: u8 = 1
    fix large: u16 = 1
    fix result = ifexpr true { small } else { large }
    return 0
}
```

### type-010 既定型で宣言した変数を別の型として返す

主根拠：`[type.literal-default]`。

補助根拠：`[type.numeric-conversion]`、`[type.expected]`。

**期待：** コンパイルエラー。

valueは初期化時にi64と決まる。returnの要求するu8へ暗黙変換しない。

```suz
fn main() -> u8 {
    fix value = 1
    return value
}
```

### type-011 構造体を宣言して値をコピーする

主根拠：`[type.value-copy]`。

補助根拠：`[struct.literal]`。

**期待：** 受理。終了コード 0。

structで宣言した型はコピー可能。コピー先のフィールドを変更しても元の値は変わらない。

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix original = Point { x: 1 }
    var copied = original
    copied.x = 2
    return ifexpr original.x == 1 && copied.x == 2 { 0 } else { 1 }
}
```

### type-012 列挙型を宣言して値をコピーする

主根拠：`[type.value-copy]`。

補助根拠：`[enum.base]`。

**期待：** 受理。終了コード 0。

enumで宣言した型はコピー可能。コピー先に別の列挙子を代入しても元の値は変わらない。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix original = Wave.Sine
    var copied = original
    copied = Wave.Saw
    return ifexpr original == Wave.Sine && copied == Wave.Saw { 0 } else { 1 }
}
```

### type-013 入れ子の構造体をコピーしてフィールドを変更する

主根拠：`[type.value-copy]`。

補助根拠：`[var.never-reassigned]`、`[assign.target]`、`[fn.call]`。

**期待：** 受理。終了コード 0。

引数をvarへコピーして、その入れ子のフィールドを変更する。呼び出し元の値は変わらない。フィールドへの代入はvarの再代入として数える。

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn changed(original: Box) -> Box {
    var copy = original
    copy.point.x = 9
    return copy
}

fn main() -> u8 {
    fix first = Box { point: Point { x: 1 } }
    fix second = changed(first)
    return ifexpr first.point.x == 1 && second.point.x == 9 { 0 } else { 1 }
}
```

### type-014 宣言が違う構造体

主根拠：`[type.identity]`。

補助根拠：`[type.expected]`。

**期待：** コンパイルエラー。

PositionとVelocityは異なる型。

```suz
struct Position { x: i32 }
struct Velocity { x: i32 }

fn main() -> u8 {
    fix p = Position { x: 1 }
    fix v: Velocity = p
    return 0
}
```

### type-015 構成が同じでも異なるenumは代入できない

主根拠：`[type.identity]`。

補助根拠：`[type.expected]`。

**期待：** コンパイルエラー。

基底型と列挙子名が同じでも、別の宣言によるenumは異なる型。

```suz
enum First: u8 { A, B }
enum Second: u8 { A, B }

fn main() -> u8 {
    fix value: Second = First.A
    return 0
}
```

### type-016 フィールド名は外側の名前を隠蔽しない

主根拠：`[struct.field]`。

**期待：** 受理。終了コード 3。

定数xと構造体のフィールドxは共存できる。初期値のxは定数を参照する。

```suz
const x: u8 = 3
struct Point { x: u8 }

fn main() -> u8 {
    fix point = Point { x: x }
    return point.x
}
```

### type-017 空の構造体

主根拠：`[struct.nonempty]`。

**期待：** コンパイルエラー。

フィールドが1つ以上必要。

```suz
struct Empty {}
fn main() -> u8 { return 0 }
```

### type-018 構造体の初期化漏れ

主根拠：`[struct.literal]`。

**期待：** コンパイルエラー。

yの初期値がない。

```suz
struct Pair { x: i32, y: i32 }

fn main() -> u8 {
    fix value = Pair { x: 1 }
    return 0
}
```

### type-019 構造体リテラルで同じフィールドを2回指定する

主根拠：`[struct.literal]`。

**期待：** コンパイルエラー。

xを重複して指定している。

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix value = Point { x: 1, x: 2 }
    return 0
}
```

### type-020 構造体は記述順に初期化し、コピーで渡す

主根拠：`[struct.eval-order]`。

補助根拠：`[struct.access]`、`[fn.call]`。

**期待：** 受理。終了コード 0。

```suz
struct Pair { x: i32, y: i32 }

fn mark(n: i32) -> i32 {
    log info `${n}`
    return n
}

fn change(original: Pair) -> Pair {
    var copy = original
    copy.x = 9
    return copy
}

fn main() -> u8 {
    fix first = Pair { y: mark(2), x: mark(1) }
    fix second = change(first)
    return ifexpr first.x == 1 && second.x == 9 { 0 } else { 1 }
}
```

標準エラー：

```text
info type-020.suz:4: 2
info type-020.suz:4: 1
```

### type-021 構造体が互いをフィールドに持つ

主根拠：`[struct.recursion]`。

**期待：** コンパイルエラー。

FirstはSecondを介して自分自身を値として含むため、間接的な再帰になる。

```suz
struct First { second: Second }
struct Second { first: First }

fn main() -> u8 { return 0 }
```

### type-022 enumの基底型を省略する

主根拠：`[enum.base]`。

**期待：** コンパイルエラー。

```suz
enum Wave { Sine, Saw }

fn main() -> u8 { return 0 }
```

### type-023 enumの基底型に浮動小数点型を指定する

主根拠：`[enum.base]`。

**期待：** コンパイルエラー。

基底型には整数型を指定する。

```suz
enum Wave: f64 { Sine, Saw }

fn main() -> u8 { return 0 }
```

### type-024 空の列挙型

主根拠：`[enum.nonempty]`。

**期待：** コンパイルエラー。

列挙子が1つ以上必要。

```suz
enum Empty: u8 {}
fn main() -> u8 { return 0 }
```

### type-025 同じenum内で列挙子名を重複させる

主根拠：`[enum.variant]`。

**期待：** コンパイルエラー。

```suz
enum Wave: u8 { Sine, Sine }

fn main() -> u8 { return 0 }
```

### type-026 列挙子の整数値を明示的に指定する

主根拠：`[enum.variant]`。

**期待：** コンパイルエラー。

列挙子の整数値は宣言順に付く。値を指定する構文はない。

```suz
enum Wave: u8 { Sine = 1, Saw = 2 }

fn main() -> u8 { return 0 }
```

### type-027 enumの自動採番が基底型の最大値に一致する

主根拠：`[enum.variant]`。

補助根拠：`[expr.enum-conversion]`。

**期待：** 受理。終了コード 0。

128個の列挙子に0から127までを割り当てる。最後のV127はi8の最大値に収まる。

```suz
enum Code: i8 {
    V000, V001, V002, V003, V004, V005, V006, V007,
    V008, V009, V010, V011, V012, V013, V014, V015,
    V016, V017, V018, V019, V020, V021, V022, V023,
    V024, V025, V026, V027, V028, V029, V030, V031,
    V032, V033, V034, V035, V036, V037, V038, V039,
    V040, V041, V042, V043, V044, V045, V046, V047,
    V048, V049, V050, V051, V052, V053, V054, V055,
    V056, V057, V058, V059, V060, V061, V062, V063,
    V064, V065, V066, V067, V068, V069, V070, V071,
    V072, V073, V074, V075, V076, V077, V078, V079,
    V080, V081, V082, V083, V084, V085, V086, V087,
    V088, V089, V090, V091, V092, V093, V094, V095,
    V096, V097, V098, V099, V100, V101, V102, V103,
    V104, V105, V106, V107, V108, V109, V110, V111,
    V112, V113, V114, V115, V116, V117, V118, V119,
    V120, V121, V122, V123, V124, V125, V126, V127,
}

fn main() -> u8 {
    fix last = Code.V127 as i8
    return ifexpr last == 127 { 0 } else { 1 }
}
```

### type-028 enumの自動採番が基底型の最大値を超える

主根拠：`[enum.variant]`。

**期待：** コンパイルエラー。

129個目の列挙子V128に割り当てる128はi8に収まらない。列挙子を使用するかどうかにかかわらず、宣言をエラーにする。

```suz
enum Code: i8 {
    V000, V001, V002, V003, V004, V005, V006, V007,
    V008, V009, V010, V011, V012, V013, V014, V015,
    V016, V017, V018, V019, V020, V021, V022, V023,
    V024, V025, V026, V027, V028, V029, V030, V031,
    V032, V033, V034, V035, V036, V037, V038, V039,
    V040, V041, V042, V043, V044, V045, V046, V047,
    V048, V049, V050, V051, V052, V053, V054, V055,
    V056, V057, V058, V059, V060, V061, V062, V063,
    V064, V065, V066, V067, V068, V069, V070, V071,
    V072, V073, V074, V075, V076, V077, V078, V079,
    V080, V081, V082, V083, V084, V085, V086, V087,
    V088, V089, V090, V091, V092, V093, V094, V095,
    V096, V097, V098, V099, V100, V101, V102, V103,
    V104, V105, V106, V107, V108, V109, V110, V111,
    V112, V113, V114, V115, V116, V117, V118, V119,
    V120, V121, V122, V123, V124, V125, V126, V127,
    V128,
}

fn main() -> u8 { return 0 }
```

### type-029 列挙子名のスコープと宣言順の整数値

主根拠：`[enum.access]`。

補助根拠：`[enum.variant]`。

**期待：** 受理。終了コード 0。

列挙子名は各enumの中で区別する。外側の定数名や別のenumの列挙子名と同じでもよい。整数値は各enumで0から始まる。

```suz
const First: u8 = 9
enum Left: u8 { First, Second, Third }
enum Right: u8 { Third, Second, First }

fn main() -> u8 {
    fix a = Left.First as u8
    fix b = Left.Second as u8
    fix c = Left.Third as u8
    fix d = Right.First as u8
    return ifexpr First == 9 && a == 0 && b == 1 && c == 2 && d == 2 { 0 } else { 1 }
}
```


## 6. 定数・変数・代入

### bind-001 定数の構造体フィールドへ代入する

主根拠：`[const.decl]`。

補助根拠：`[assign.target]`。

**期待：** コンパイルエラー。

構造体を定数として宣言した場合、そのフィールドも変更できない。

```suz
struct Point { x: i32 }
const origin = Point { x: 1 }

fn main() -> u8 {
    origin.x = 2
    return 0
}
```

### bind-002 定数が互いを循環参照する

主根拠：`[const.initializer]`。

**期待：** コンパイルエラー。

前方参照は可能だが、firstとsecondの初期値は循環している。

```suz
const first: u8 = second
const second: u8 = first

fn main() -> u8 {
    return first
}
```

### bind-003 構造体の定数とフィールド参照を定数式に使う

主根拠：`[const.expression]`。

補助根拠：`[const.cast-check]`。

**期待：** 受理。終了コード 3。

originのフィールドを参照した加算も定数式になる。結果がu8に収まるので、変換のorを省略できる。

```suz
struct Point { x: i32, y: i32 }
const origin = Point { x: 1, y: 2 }
const total = origin.x + origin.y

fn main() -> u8 {
    return total as u8
}
```

### bind-004 短絡で評価しない関数呼び出しも定数式には使えない

主根拠：`[const.expression]`。

補助根拠：`[const.evaluation]`。

**期待：** コンパイルエラー。

短絡で右辺を評価しなくても、定数式の構成を検査する。関数呼び出しは定数式に含まれない。

```suz
fn ready() -> bool {
    return true
}

const result = true || ready()

fn main() -> u8 {
    return ifexpr result { 0 } else { 1 }
}
```

### bind-005 exprブロックを定数の初期値にする

主根拠：`[const.expression]`。

補助根拠：`[const.initializer]`。

**期待：** コンパイルエラー。

内容がリテラルだけでも、exprブロックは定数式の構成要素ではない。

```suz
const result: u8 = expr { 3 }

fn main() -> u8 {
    return result
}
```

### bind-006 定数式の成功時は代替値を評価しない

主根拠：`[const.evaluation]`。

**期待：** 受理。終了コード 3。

代替値の型は検査するが、ゼロ除算は評価しない。

```suz
const result: u8 = (1 + 2) or (1 / 0)

fn main() -> u8 {
    return result
}
```

### bind-007 定数式の演算失敗時は代替値を評価

主根拠：`[const.evaluation]`。

補助根拠：`[const.expression]`。

**期待：** 受理。終了コード 7。

```suz
const result: u8 = (255 + 1) or 7

fn main() -> u8 {
    return result
}
```

### bind-008 constのオーバーフロー

主根拠：`[const.evaluation]`。

**期待：** コンパイルエラー。

コンパイル時の加算が範囲外で、orがない。

```suz
const value: u8 = 255 + 1
fn main() -> u8 { return 0 }
```

### bind-009 実行しない代替値にも型検査を適用

主根拠：`[const.evaluation]`。

補助根拠：`[expr.arithmetic-fallback]`。

**期待：** コンパイルエラー。

加算は成功するが、代替値のboolはu8と一致しない。

```suz
const value: u8 = (1 + 2) or false
fn main() -> u8 { return 0 }
```

### bind-010 定数変換の失敗

主根拠：`[const.cast-check]`。

**期待：** コンパイルエラー。

変換失敗がコンパイル時に分かり、orがない。

```suz
fn main() -> u8 {
    fix value = 300 as u8
    return 0
}
```

### bind-011 短絡で評価しない定数変換にも失敗処理を書く

主根拠：`[const.cast-check]`。

補助根拠：`[const.evaluation]`。

**期待：** 受理。終了コード 0。

右辺のキャストは`or`の必須検査を満たす。論理和の左辺がtrueなので、右辺は評価しない。

```suz
const result = true || ((300 as u8 or 0) == 0)

fn main() -> u8 {
    return ifexpr result { 0 } else { 1 }
}
```

### bind-012 短絡で評価しない失敗する定数変換

主根拠：`[const.cast-check]`。

補助根拠：`[const.evaluation]`。

**期待：** コンパイルエラー。

右辺を評価しなくても、失敗する定数キャストには`or`が必要。

```suz
const result = true || ((300 as u8) == 0)

fn main() -> u8 {
    return ifexpr result { 0 } else { 1 }
}
```

### bind-013 varの初期値を省略する

主根拠：`[var.initializer]`。

**期待：** コンパイルエラー。

後続に代入があっても、宣言時の初期値は必須。

```suz
fn main() -> u8 {
    var count: u8
    count = 1
    return count
}
```

### bind-014 fixのフィールドへの代入

主根拠：`[var.fix-assign]`。

**期待：** コンパイルエラー。

fixのフィールドは変更できない。

```suz
struct Point { x: i32 }

fn main() -> u8 {
    fix point = Point { x: 0 }
    point.x = 1
    return 0
}
```

### bind-015 fixの入れ子のフィールドを変更する

主根拠：`[var.fix-assign]`。

**期待：** コンパイルエラー。

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn main() -> u8 {
    fix value = Box { point: Point { x: 1 } }
    value.point.x = 9
    return 0
}
```

### bind-016 実行されない分岐の代入もvarの再代入として数える

主根拠：`[var.never-reassigned]`。

**期待：** 受理。終了コード 3。

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

主根拠：`[var.never-reassigned]`。

**期待：** コンパイルエラー。

初期化以外にcountへの代入がない。

```suz
fn main() -> u8 {
    var count = 0
    return 0
}
```

### bind-018 通常の代入でキャストの代替値を使う

主根拠：`[assign.type]`。

補助根拠：`[expr.cast-fallback]`。

**期待：** 受理。終了コード 7。

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix raw: i64 = 300
    count = raw as u8 or 7
    return count
}
```

### bind-019 代入先と異なる型の変数を代入する

主根拠：`[assign.type]`。

補助根拠：`[type.numeric-conversion]`。

**期待：** コンパイルエラー。

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

主根拠：`[assign.compound]`。

補助根拠：`[expr.arithmetic-scope]`。

**期待：** 受理。終了コード 13。

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

主根拠：`[assign.compound]`。

補助根拠：`[expr.arithmetic-scope]`、`[rt.panic]`、`[rt.int-overflow]`。

**期待：** 受理。実行時パニック。終了コード 101。

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

主根拠：`[assign.eval-order]`。

補助根拠：`[expr.cast-return]`、`[expr.or-return-position]`。

**期待：** 受理。終了コード 10。

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

主根拠：`[assign.rhs-write]`。

補助根拠：`[assign.target]`、`[expr.block]`。

**期待：** 受理。終了コード 0。

point.xへの代入の右辺で、point.xの読み取りとpoint.yへの代入を行える。

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

### bind-024 複合代入の右辺で同じ代入先を書き換える

主根拠：`[assign.rhs-write]`。

**期待：** コンパイルエラー。

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

主根拠：`[assign.rhs-write]`。

**期待：** コンパイルエラー。

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

主根拠：`[assign.rhs-write]`。

**期待：** コンパイルエラー。

point全体への代入は、外側の代入先point.xと重なる。

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

### bind-027 構造体への代入の右辺でそのフィールドを書き換える

主根拠：`[assign.rhs-write]`。

**期待：** コンパイルエラー。

point.xへの代入は、外側の代入先point全体と重なる。

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

### bind-028 複合代入を丸括弧で囲んでorの対象にする

主根拠：`[assign.statement]`。

補助根拠：`[expr.arithmetic-handler]`。

**期待：** コンパイルエラー。

複合代入は文なので、丸括弧で囲んでも式にはならず、外側のorの対象にはできない。除算と加算をそれぞれ処理する書き方はexpr-062を参照。

```suz
fn main() -> u8 {
    var count: u8 = 255
    fix divisor: u8 = 1
    (count += (1 / divisor) or 0) or 1
    return count
}
```

### bind-029 通常の代入を初期値の式として使う

主根拠：`[assign.statement]`。

**期待：** コンパイルエラー。

通常の代入も文であり、丸括弧で囲んで値を得ることはできない。

```suz
fn main() -> u8 {
    var count: u8 = 0
    fix result = (count = 1)
    return count
}
```


## 7. 式

### expr-001 ラップする算術と切り詰め

主根拠：`[expr.wrap]`。

補助根拠：`[expr.as-trunc]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix wrapped = top +% 1
    fix down: u8 = 0 -% 1
    fix product: u8 = 128 *% 2
    fix truncated = 257 as% u8
    return ifexpr wrapped == 0 && down == 255 && product == 0 && truncated == 1 { 0 } else { 1 }
}
```

### expr-002 符号付き整数のラップとas%の符号の扱い

主根拠：`[expr.wrap]`。

補助根拠：`[expr.as-trunc]`。

**期待：** 受理。終了コード 0。

i8の127 +% 1は-128。255 as% i8は-1。-1 as% u16は65535になる。

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

### expr-003 ifexprの分岐の型不一致

主根拠：`[branch.expression]`。

**期待：** コンパイルエラー。

整数とboolは同じ結果型にならない。

```suz
fn main() -> u8 {
    fix value = ifexpr true { 1 } else { false }
    return 0
}
```

### expr-004 ifexprの両分岐がvoidになる

主根拠：`[branch.expression]`。

補助根拠：`[block.value]`。

**期待：** コンパイルエラー。

両分岐の型が一致していても、結果がvoidのifexprは使えない。結果を捨てる位置でも同じ。

```suz
fn action() -> void {}

fn main() -> u8 {
    ifexpr true { action() } else { action() }
    return 0
}
```

### expr-005 matchexprの全腕がvoidになる

主根拠：`[branch.expression]`。

補助根拠：`[match.value]`。

**期待：** コンパイルエラー。

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

主根拠：`[expr.jump-prohibited]`。

**期待：** コンパイルエラー。

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

主根拠：`[expr.jump-prohibited]`。

**期待：** コンパイルエラー。

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

主根拠：`[expr.jump-prohibited]`。

**期待：** コンパイルエラー。

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

主根拠：`[expr.jump-prohibited]`。

補助根拠：`[fn.call]`、`[fn.return]`。

**期待：** 受理。終了コード 3。

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

主根拠：`[expr.jump-prohibited]`。

**期待：** コンパイルエラー。

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

主根拠：`[expr.jump-prohibited]`。

**期待：** コンパイルエラー。

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

### expr-012 ifexprで選ばれない分岐の演算は実行しない

主根拠：`[expr.if-value]`。

補助根拠：`[if.condition]`。

**期待：** 受理。終了コード 7。

elseの分岐を評価しないので、整数のゼロ除算によるパニックは起きない。

```suz
fn main() -> u8 {
    fix divisor: u8 = 0
    fix result: u8 = ifexpr true { 7 } else { 1 / divisor }
    return result
}
```

### expr-013 if文を初期値にする

主根拠：`[expr.if-value]`。

補助根拠：`[branch.statement]`。

**期待：** コンパイルエラー。

値を返す分岐はifexprで書く。

```suz
fn main() -> u8 {
    fix value = if true { 1 } else { 2 }
    return 0
}
```

### expr-014 ifexprのelse漏れ

主根拠：`[expr.if-value]`。

**期待：** コンパイルエラー。

ifexprには最後のelseが必要。

```suz
fn main() -> u8 {
    fix value = ifexpr true { 1 }
    return 0
}
```

### expr-015 exprとifexpr、elif

主根拠：`[expr.block]`。

補助根拠：`[expr.if-value]`、`[block.value]`、`[if.elif]`。

**期待：** 受理。終了コード 5。

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

主根拠：`[expr.block]`。

補助根拠：`[block.statement]`。

**期待：** コンパイルエラー。

値を作るブロックはexprで書く。

```suz
fn main() -> u8 {
    fix value = { 1 }
    return 0
}
```

### expr-017 exprの中でwhileを使って値を計算する

主根拠：`[expr.block]`。

補助根拠：`[expr.jump-prohibited]`、`[block.value]`、`[while.loop]`。

**期待：** 受理。終了コード 3。

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

主根拠：`[expr.struct-in-cond]`。

補助根拠：`[expr.equality]`。

**期待：** 受理。終了コード 0。

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

主根拠：`[expr.struct-in-cond]`。

**期待：** 受理。終了コード 0。

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

主根拠：`[expr.not-precedence]`。

補助根拠：`[expr.not]`。

**期待：** コンパイルエラー。

not value == 0は(not value) == 0と読む。valueは整数なのでnotを適用できない。比較結果を否定する場合はnot (value == 0)と書く。

```suz
fn main() -> u8 {
    fix value: i32 = 1
    fix different = not value == 0
    return ifexpr different { 0 } else { 1 }
}
```

### expr-021 算術・シフト・ビット演算と比較の優先順位

主根拠：`[expr.precedence]`。

**期待：** 受理。終了コード 0。

2 + 3 * 4は14、1 << 1 + 2は8。6 & 3 == 2は、ビットANDの結果2と比較する。

```suz
fn main() -> u8 {
    fix arithmetic = 2 + 3 * 4
    fix shifted = 1 << 1 + 2
    fix matched = 6 & 3 == 2
    return ifexpr arithmetic == 14 && shifted == 8 && matched { 0 } else { 1 }
}
```

### expr-022 同じ優先順位の二項演算は左から結合する

主根拠：`[expr.associativity]`。

**期待：** 受理。終了コード 0。

10 - 3 - 2は5、20 / 2 / 2は5になる。右側を括弧でまとめると、それぞれ9と20になる。

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

### expr-023 関数の実引数を左から評価

主根拠：`[expr.eval-order]`。

**期待：** 受理。終了コード 0。

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

標準エラー：

```text
info expr-023.suz:2: 1
info expr-023.suz:2: 2
```

### expr-024 論理演算の短絡評価

主根拠：`[expr.short-circuit]`。

**期待：** 受理。終了コード 0。

probeは呼ばれず、ログは出ない。

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

### expr-025 負数の除算・剰余・右シフト

主根拠：`[expr.int-division]`。

補助根拠：`[expr.shift]`。

**期待：** 受理。終了コード 0。

-7 / 3は-2、-7 % 3は-1。符号付きの-7 >> 1は符号ビットで埋めるため-4になる。符号なしの249 >> 1は124になる。

```suz
fn main() -> u8 {
    fix negative: i8 = -7
    fix positive: u8 = 249
    fix quotient = negative / 3
    fix remainder = negative % 3
    fix signed_shift = negative >> 1
    fix unsigned_shift = positive >> 1
    return ifexpr quotient == -2 && remainder == -1 &&
        signed_shift == -4 && unsigned_shift == 124 { 0 } else { 1 }
}
```

### expr-026 同じ向きの不等号を連鎖させる

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**期待：** 受理。終了コード 0。

厳密な不等号と等号付きの不等号を同じ向きで組み合わせられる。後ろのオペランドのu8型を、前のリテラルにも適用する。

```suz
fn main() -> u8 {
    fix upper: u8 = 3
    fix ascending = 1 < 2 <= upper
    fix descending = upper >= 3 > 1
    return ifexpr ascending && descending { 0 } else { 1 }
}
```

### expr-027 比較の連鎖で不等号の向きを変える

主根拠：`[expr.comparison-chain]`。

**期待：** コンパイルエラー。

1 < 2 > 3は途中で不等号の向きが変わる。

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 > 3
    return ifexpr ordered { 0 } else { 1 }
}
```

### expr-028 等値比較を含む同じ向きの連鎖

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**期待：** 受理。終了コード 0。

==は先頭にも途中にも置ける。不等号の向きは連鎖全体でそろえる。

```suz
fn main() -> u8 {
    fix ascending = 1 == 1 < 2 == 2 <= 3
    fix descending = 3 == 3 >= 2 == 2 > 1
    fix equal = 2 == 2 == 2
    return ifexpr ascending && descending && equal { 0 } else { 1 }
}
```

### expr-029 比較の連鎖に!=を混ぜる

主根拠：`[expr.comparison-chain]`。

**期待：** コンパイルエラー。

不等号の連鎖に!=を混ぜることはできない。

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 != 3
    return ifexpr ordered { 0 } else { 1 }
}
```

### expr-030 等値比較を挟んでも不等号の向きは変えられない

主根拠：`[expr.comparison-chain]`。

**期待：** コンパイルエラー。

==を挟んだ前後で、不等号の向きが変わっている。

```suz
fn main() -> u8 {
    fix ordered = 1 < 2 == 2 > 1
    return ifexpr ordered { 0 } else { 1 }
}
```

### expr-031 !=だけの比較も連鎖できない

主根拠：`[expr.comparison-chain]`。

**期待：** コンパイルエラー。

!=は2つのオペランドの単独の比較で使う。

```suz
fn main() -> u8 {
    fix different = 1 != 2 != 1
    return ifexpr different { 0 } else { 1 }
}
```

### expr-032 等値比較だけの連鎖でbool・enum・構造体を比較する

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.equality]`。

**期待：** 受理。終了コード 0。

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

### expr-033 短絡で評価しない比較対象の型不一致

主根拠：`[expr.comparison-chain]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**期待：** コンパイルエラー。

最初の比較が不成立でも、連鎖全体を型検査する。u8とi32は一致しない。

```suz
fn main() -> u8 {
    fix first: u8 = 3
    fix second: u8 = 2
    fix third: i32 = 4
    fix ordered = first < second < third
    return ifexpr ordered { 1 } else { 0 }
}
```

### expr-034 比較の連鎖は中間の式を1回だけ評価する

主根拠：`[expr.comparison-chain-evaluation]`。

**期待：** 受理。終了コード 0。

mark(2)の結果を両側の比較で共有する。

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

標準エラー：

```text
info expr-034.suz:2: 1
info expr-034.suz:2: 2
info expr-034.suz:2: 3
```

### expr-035 比較の連鎖は不成立で評価を打ち切る

主根拠：`[expr.comparison-chain-evaluation]`。

**期待：** 受理。終了コード 0。

3 < 2が不成立なので、probeは呼ばれずログも出ない。

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

### expr-036 NaNを含む構造体は自分自身とも等しくない

主根拠：`[expr.equality]`。

補助根拠：`[expr.nan-comparison]`。

**期待：** 受理。終了コード 0。

構造体の等値比較にも各フィールドの==を使う。NaNのフィールドがあるため、同じ変数同士を比較しても==はfalse、!=はtrueになる。

```suz
struct Sample { value: f64 }

fn main() -> u8 {
    fix sample = Sample { value: nan }
    return ifexpr not (sample == sample) && sample != sample { 0 } else { 1 }
}
```

### expr-037 NaNで比較の連鎖が不成立になれば残りを評価しない

主根拠：`[expr.nan-comparison]`。

補助根拠：`[expr.comparison-chain-evaluation]`。

**期待：** 受理。終了コード 0。

0.0 < nanがfalseなので、probeは呼ばれずログも出ない。

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

### expr-038 f32からf64への変換で有限値と特殊値を保つ

主根拠：`[expr.as]`。

補助根拠：`[expr.nan-comparison]`、`[expr.cast-risk]`。

**期待：** 受理。終了コード 0。

f32からf64への変換は常に成功し、orを必要としない。有限値は正確に表し、無限大とNaNも保つ。

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

### expr-039 boolから整数へのasは使えない

主根拠：`[expr.as]`。

**期待：** コンパイルエラー。

boolから整数への変換はasが許可する型の組み合わせに含まれない。

```suz
fn main() -> u8 {
    fix value = true as u8
    return value
}
```

### expr-040 異なるenumへ直接asで変換する

主根拠：`[expr.as]`。

**期待：** コンパイルエラー。

enum同士の直接変換は許可されない。orを付けても許可される型の組み合わせは変わらない。

```suz
enum First: u8 { A, B }
enum Second: u8 { A, B }

fn main() -> u8 {
    fix source = First.A
    fix value = source as Second or Second.A
    return 0
}
```

### expr-041 必ず成功する変換と定数式の変換

主根拠：`[expr.cast-risk]`。

補助根拠：`[const.cast-check]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    fix small: u8 = 7
    fix wide = small as i64
    fix literal = 255 as u8
    return ifexpr wide == 7 && literal == 255 { 0 } else { 1 }
}
```

### expr-042 値が小さくてもfixからの縮小変換にはorが必要

主根拠：`[expr.cast-risk]`。

補助根拠：`[const.expression]`、`[expr.cast-handler]`。

**期待：** コンパイルエラー。

rawはconstではなく、変換は型で判定すると失敗しうる。

```suz
fn main() -> u8 {
    fix raw: i64 = 1
    fix value = raw as u8
    return value
}
```

### expr-043 変換元を1回だけ評価し、成功時の代替値を省略

主根拠：`[expr.cast-fallback]`。

補助根拠：`[log.output]`。

**期待：** 受理。終了コード 7。

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

標準エラー：

```text
info expr-043.suz:2: source
```

### expr-044 変換失敗時だけ代替値を評価

主根拠：`[expr.cast-fallback]`。

**期待：** 受理。終了コード 9。

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

標準エラー：

```text
info expr-044.suz:2: source
info expr-044.suz:7: fallback
```

### expr-045 orの代替値の型不一致

主根拠：`[expr.cast-fallback]`。

**期待：** コンパイルエラー。

代替値にはu8を要求する。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix value = raw as u8 or false
    return 0
}
```

### expr-046 or return exprで関数を抜ける

主根拠：`[expr.cast-return]`。

補助根拠：`[expr.block]`、`[expr.or-return-position]`。

**期待：** 受理。終了コード 7。

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

標準エラー：

```text
info expr-046.suz:4: conversion failed
```

### expr-047 or return exprの成功経路

主根拠：`[expr.cast-return]`。

補助根拠：`[expr.or-return-position]`。

**期待：** 受理。終了コード 3。

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

### expr-048 戻り値voidのor return

主根拠：`[expr.cast-return]`。

補助根拠：`[fn.return]`。

**期待：** 受理。終了コード 0。

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

標準エラー：

```text
info expr-048.suz:8: done
```

### expr-049 or returnを含む右辺全体の丸括弧

主根拠：`[expr.or-return-position]`。

**期待：** 受理。終了コード 7。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix count = (raw as u8 or return 7)
    return count
}
```

### expr-050 加算の途中にor returnを埋め込む

主根拠：`[expr.or-return-position]`。

**期待：** コンパイルエラー。

or returnを付けた変換が初期値全体ではない。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    fix result = (raw as u8 or return 1) + 2
    return result
}
```

### expr-051 関数の引数にor returnを埋め込む

主根拠：`[expr.or-return-position]`。

**期待：** コンパイルエラー。

実引数の内部のor returnは禁止。

```suz
fn use(value: u8) -> void {}

fn main() -> u8 {
    fix raw: i64 = 300
    use(raw as u8 or return 1)
    return 0
}
```

### expr-052 returnに渡す式の中にor returnを書く

主根拠：`[expr.or-return-position]`。

**期待：** コンパイルエラー。

returnの値を作る式にはor returnを置けない。

```suz
fn main() -> u8 {
    fix raw: i64 = 300
    return raw as u8 or return 7
}
```

### expr-053 構造体のフィールド初期値にor returnを書く

主根拠：`[expr.or-return-position]`。

**期待：** コンパイルエラー。

変換はフィールド初期値の全体でも、fixの初期値全体ではない。

```suz
struct Counter { value: u8 }

fn main() -> u8 {
    fix raw: i64 = 300
    fix counter = Counter { value: raw as u8 or return 7 }
    return counter.value
}
```

### expr-054 orの代替値の中にor returnを書く

主根拠：`[expr.or-return-position]`。

**期待：** コンパイルエラー。

内側の変換は外側のorの代替値に埋め込まれているため、or returnを付けられない。

```suz
fn main() -> u8 {
    fix first: i64 = 300
    fix second: i64 = 400
    fix value = first as u8 or (second as u8 or return 7)
    return value
}
```

### expr-055 キャストのorは右側の式全体を代替値にする

主根拠：`[expr.cast-binding]`。

補助根拠：`[expr.cast-fallback]`。

**期待：** 受理。終了コード 0。

変換は成功するので、firstは7。secondは変換結果に2を加えて9になる。

```suz
fn main() -> u8 {
    fix raw: i64 = 7
    fix first = raw as u8 or 1 + 2
    fix second = (raw as u8 or 1) + 2
    return ifexpr first == 7 && second == 9 { 0 } else { 1 }
}
```

### expr-056 enumと整数の相互変換

主根拠：`[expr.enum-conversion]`。

補助根拠：`[enum.variant]`、`[expr.cast-risk]`、`[rt.as-int-enum]`。

**期待：** 受理。終了コード 0。

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

### expr-057 同じ加算の失敗をorで処理

主根拠：`[expr.arithmetic-handler]`。

補助根拠：`[expr.arithmetic-fallback]`。

**期待：** 受理。終了コード 7。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = (top + 1) or 7
    return result
}
```

### expr-058 浮動小数点の除算に演算失敗のorを付ける

主根拠：`[expr.arithmetic-handler]`。

補助根拠：`[rt.float]`。

**期待：** コンパイルエラー。

演算失敗を処理するorの対象は整数演算。浮動小数点の除算には付けられない。

```suz
fn main() -> u8 {
    fix zero = 0.0
    fix result = (1.0 / zero) or 0.0
    return 0
}
```

### expr-059 算術のorは対象の演算を1回だけ評価

主根拠：`[expr.arithmetic-fallback]`。

補助根拠：`[expr.arithmetic-scope]`。

**期待：** 受理。終了コード 7。

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

標準エラー：

```text
info expr-059.suz:2: left
info expr-059.suz:7: right
info expr-059.suz:12: alternative
```

### expr-060 加算全体を処理してから代入する

主根拠：`[expr.arithmetic-fallback]`。

補助根拠：`[assign.target]`。

**期待：** 受理。終了コード 7。

countへの加算が失敗した場合の代替値を、通常の代入で保存する。

```suz
fn main() -> u8 {
    var count: u8 = 255
    count = (count + 1) or 7
    return count
}
```

### expr-061 整数演算の失敗時に初期化を中断して戻る

主根拠：`[expr.arithmetic-fallback]`。

補助根拠：`[expr.or-return-position]`。

**期待：** 受理。終了コード 7。

加算全体がfixの初期値なのでor returnを付けられる。オーバーフローしたら7を返し、後続のreturnには進まない。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = (top + 1) or return 7
    return result
}
```

### expr-062 通常の代入で除算と加算をそれぞれ処理する

主根拠：`[expr.arithmetic-scope]`。

補助根拠：`[assign.target]`、`[expr.arithmetic-fallback]`。

**期待：** 受理。終了コード 0。

bind-028の複合代入を通常の代入に書き換えた形。divisorが0なら除算の代替値0を加えて255になる。1なら除算は成功するが加算がオーバーフローし、外側の代替値1を代入する。

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

### expr-063 外側のorは内側の加算を処理しない

主根拠：`[expr.arithmetic-scope]`。

補助根拠：`[rt.panic]`、`[rt.int-overflow]`。

**期待：** 受理。実行時パニック。終了コード 101。

乗算のorに到達する前に、内側の加算が失敗する。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: u8) -> void {
    fix result = ((value + 1) * 1) or 0
}

fn main() -> u8 {
    operation(255)
    return 0
}
```

### expr-064 呼び出し先のパニックをorで捕まえない

主根拠：`[expr.arithmetic-scope]`。

補助根拠：`[rt.panic]`、`[rt.div-zero]`。

**期待：** 受理。実行時パニック。終了コード 101。

divide内の除算が失敗する。加算に付いたorは実行しない。標準エラーに除算の位置と理由を出す。

```suz
fn divide(divisor: i32) -> i32 {
    return 1 / divisor
}

fn main() -> u8 {
    fix result = (divide(0) + 1) or 0
    return 0
}
```

### expr-065 orの代替値にあるorは右から結合する

主根拠：`[expr.or-binding]`。

補助根拠：`[expr.arithmetic-fallback]`。

**期待：** 受理。終了コード 7。

最初の加算が失敗すると、代替値の加算とその失敗処理を評価する。両方の加算が失敗し、最後の7になる。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = top + 1 or top + 1 or 7
    return result
}
```


## 8. 関数と制御

### flow-001 関数の戻り値型を省略する

主根拠：`[fn.return-type]`。

**期待：** コンパイルエラー。

値を返さない関数も、戻り値型を-> voidと明記する必要がある。

```suz
fn action() {}

fn main() -> u8 { return 0 }
```

### flow-002 仮引数の入れ子のフィールドを変更する

主根拠：`[fn.parameter]`。

補助根拠：`[assign.target]`。

**期待：** コンパイルエラー。

仮引数のフィールドは、入れ子であっても変更できない。

```suz
struct Point { x: i32 }
struct Box { point: Point }

fn change(value: Box) -> void {
    value.point.x = 9
}

fn main() -> u8 { return 0 }
```

### flow-003 仮引数と同名のローカル変数

主根拠：`[fn.parameter-scope]`。

**期待：** コンパイルエラー。

valueが仮引数名と重複する。

```suz
fn use(value: i32) -> void {
    fix value = 1
}

fn main() -> u8 { return 0 }
```

### flow-004 関数呼び出しの引数が不足する

主根拠：`[fn.call]`。

**期待：** コンパイルエラー。

sumには2つの実引数が必要。

```suz
fn sum(left: u8, right: u8) -> u8 {
    return left + right
}

fn main() -> u8 {
    return sum(1)
}
```

### flow-005 関数の実引数と仮引数の型が異なる

主根拠：`[fn.call]`。

補助根拠：`[type.numeric-conversion]`。

**期待：** コンパイルエラー。

valueの型はu16であり、u8の仮引数へ暗黙変換しない。

```suz
fn identity(input: u8) -> u8 {
    return input
}

fn main() -> u8 {
    fix value: u16 = 1
    return identity(value)
}
```

### flow-006 関数の相互再帰

主根拠：`[fn.recursion]`。

補助根拠：`[decl.top-level]`、`[fn.call]`。

**期待：** 受理。終了コード 0。

is_evenとis_oddが互いを呼び出す。0で再帰を止める。

```suz
fn is_even(n: u8) -> bool {
    if n == 0 {
        return true
    }
    return is_odd(n - 1)
}

fn is_odd(n: u8) -> bool {
    if n == 0 {
        return false
    }
    return is_even(n - 1)
}

fn main() -> u8 {
    return ifexpr is_even(4) && is_odd(3) { 0 } else { 1 }
}
```

### flow-007 void関数はreturnと末尾到達の両方で戻れる

主根拠：`[fn.return]`。

補助根拠：`[fn.fallthrough]`。

**期待：** 受理。終了コード 0。

stopがtrueならreturnで戻り、falseなら本体末尾に到達して戻る。

```suz
fn visit(stop: bool) -> void {
    if stop {
        return
    }
}

fn main() -> u8 {
    visit(true)
    visit(false)
    return 0
}
```

### flow-008 void関数のreturnに値を付ける

主根拠：`[fn.return]`。

**期待：** コンパイルエラー。

```suz
fn action() -> void {
    return 1
}

fn main() -> u8 { return 0 }
```

### flow-009 値を返す関数で式のないreturnを書く

主根拠：`[fn.return]`。

**期待：** コンパイルエラー。

ほかの経路にreturn式があっても、値のないreturnはu8関数では使えない。

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return
    }
    return 1
}

fn main() -> u8 { return 0 }
```

### flow-010 非void関数にreturn式がない

主根拠：`[fn.value-return-required]`。

**期待：** コンパイルエラー。

末尾へ到達しなくても、return式が少なくとも1つ必要。

```suz
fn wait_forever() -> i32 {
    loop {}
}
fn main() -> u8 { return 0 }
```

### flow-011 関数末尾へ到達する経路

主根拠：`[fn.fallthrough]`。

補助根拠：`[fn.flow-analysis]`。

**期待：** コンパイルエラー。

elseがなく、末尾へ進みうる。

```suz
fn choose(flag: bool) -> u8 {
    if flag {
        return 1
    }
}
fn main() -> u8 { return 0 }
```

### flow-012 安全な処理と分岐からの明示的return

主根拠：`[fn.flow-analysis]`。

補助根拠：`[if.elif]`。

**期待：** 受理。終了コード 2。

```suz
fn choose(value: i32) -> u8 {
    if value < 0 {
        return 1
    } elif value == 0 {
        return 2
    } else {
        return 3
    }
}

fn main() -> u8 {
    return choose(0)
}
```

### flow-013 while trueの内部から値を返す

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.value-return-required]`。

**期待：** 受理。終了コード 3。

条件はtrueリテラルで、ループを抜けるbreakはない。return式もあるため、関数末尾に別のreturnは必要ない。

```suz
fn main() -> u8 {
    while true {
        return 3
    }
}
```

### flow-014 trueの定数を条件にしたwhileは末尾へ進みうると判定する

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.fallthrough]`。

**期待：** コンパイルエラー。

条件に書かれているのは定数名であり、trueリテラルではない。関数末尾へ進みうるという構文上の判定になる。

```suz
const always = true

fn main() -> u8 {
    while always {
        return 3
    }
}
```

### flow-015 反復があるforでも関数末尾への経路を残す

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.fallthrough]`。

**期待：** コンパイルエラー。

範囲が0..<1であっても、forは後続へ進みうるものとして判定する。ループ内のreturnだけでは関数末尾への到達を防げない。

```suz
fn main() -> u8 {
    for i in 0..<1 {
        return 3
    }
}
```

### flow-016 内側のbreakは外側のloopを抜けない

主根拠：`[fn.flow-analysis]`。

補助根拠：`[loop.break]`。

**期待：** 受理。終了コード 4。

breakが抜けるのは内側のloopだけ。外側のloopには、それを抜けるbreakがない。

```suz
fn main() -> u8 {
    loop {
        loop {
            break
        }
        return 4
    }
}
```

### flow-017 実行されない分岐のbreakも到達判定で数える

主根拠：`[fn.flow-analysis]`。

補助根拠：`[fn.fallthrough]`。

**期待：** コンパイルエラー。

条件がfalseでも、外側のloopを抜けるbreakが構文上存在する。ループ後にreturnがないためエラーになる。

```suz
fn main() -> u8 {
    loop {
        if false {
            break
        }
        return 3
    }
}
```

### flow-018 matchの対象を1回評価し、一致した腕だけ実行する

主根拠：`[match.target]`。

**期待：** 受理。終了コード 0。

subjectを1回呼び、値2に対応する腕だけを実行する。

```suz
fn subject() -> i32 {
    log info `subject`
    return 2
}

fn main() -> u8 {
    match subject() {
        1 => { log info `first` }
        2 => { log info `second` }
        _ => { log info `other` }
    }
    return 0
}
```

標準エラー：

```text
info flow-018.suz:2: subject
info flow-018.suz:9: second
```

### flow-019 整数matchの定数パターンとワイルドカード

主根拠：`[match.pattern]`。

補助根拠：`[match.exhaustive]`。

**期待：** 受理。終了コード 3。

```suz
const selected: i32 = 2

fn main() -> u8 {
    fix input: i32 = 2
    match input {
        selected => { return 3 }
        _ => { return 4 }
    }
}
```

### flow-020 boolでワイルドカードを使う

主根拠：`[match.pattern]`。

**期待：** コンパイルエラー。

全値を列挙していてもboolの_は禁止。

```suz
fn main() -> u8 {
    match true {
        true => {}
        false => {}
        _ => {}
    }
    return 0
}
```

### flow-021 enumでワイルドカードを使う

主根拠：`[match.pattern]`。

**期待：** コンパイルエラー。

enumの_は禁止。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    match Wave.Sine {
        Wave.Sine => {}
        Wave.Saw => {}
        _ => {}
    }
    return 0
}
```

### flow-022 評価後のパターンの重複

主根拠：`[match.pattern]`。

**期待：** コンパイルエラー。

1 + 1と2は同じ値。

```suz
fn main() -> u8 {
    match 2 {
        1 + 1 => {}
        2 => {}
        _ => {}
    }
    return 0
}
```

### flow-023 整数matchの途中にワイルドカードを置く

主根拠：`[match.pattern]`。

補助根拠：`[match.exhaustive]`。

**期待：** コンパイルエラー。

整数の_は最後の腕に置く必要がある。

```suz
fn main() -> u8 {
    match 1 {
        _ => {}
        1 => {}
    }
    return 0
}
```

### flow-024 fixの変数をmatchのパターンにする

主根拠：`[match.pattern]`。

補助根拠：`[const.expression]`。

**期待：** コンパイルエラー。

fixの変数は、数値リテラルで初期化しても定数式の構成要素にはならない。

```suz
fn main() -> u8 {
    fix selected: i32 = 1
    fix input: i32 = 1
    match input {
        selected => {}
        _ => {}
    }
    return 0
}
```

### flow-025 boolの全値を網羅するmatchexpr

主根拠：`[match.exhaustive]`。

補助根拠：`[match.value]`。

**期待：** 受理。終了コード 3。

```suz
fn main() -> u8 {
    fix flag = true
    return matchexpr flag {
        true => 3
        false => 4
    }
}
```

### flow-026 enumの全列挙子を網羅するmatchexpr

主根拠：`[match.exhaustive]`。

補助根拠：`[match.value]`。

**期待：** 受理。終了コード 2。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix wave = Wave.Saw
    return matchexpr wave {
        Wave.Sine => 1
        Wave.Saw => 2
    }
}
```

### flow-027 boolの網羅漏れ

主根拠：`[match.exhaustive]`。

**期待：** コンパイルエラー。

falseの腕が必要。

```suz
fn main() -> u8 {
    match true {
        true => {}
    }
    return 0
}
```

### flow-028 整数matchのワイルドカード漏れ

主根拠：`[match.exhaustive]`。

**期待：** コンパイルエラー。

整数matchには最後の_が必要。

```suz
fn main() -> u8 {
    match 0 {
        0 => {}
    }
    return 0
}
```

### flow-029 enumの列挙子を網羅していないmatch

主根拠：`[match.exhaustive]`。

**期待：** コンパイルエラー。

対象がWave.Sineと書かれていても、型の全列挙子を網羅する必要がある。Wave.Sawの腕がない。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    match Wave.Sine {
        Wave.Sine => {}
    }
    return 0
}
```

### flow-030 最後のmatchの腕と閉じ括弧を同じ行に書く

主根拠：`[match.separator]`。

補助根拠：`[src.statement-end]`、`[match.arm]`。

**期待：** 受理。終了コード 2。

腕同士は改行で区切るが、最後の腕のブロックとmatchを閉じる波括弧の間は改行を省略できる。

```suz
fn main() -> u8 {
    match false {
        true => { return 1 }
        false => { return 2 } }
}
```

### flow-031 matchの腕を同じ行に並べる

主根拠：`[match.separator]`。

**期待：** コンパイルエラー。

ブロックを閉じても次の腕との間には改行が必要。最後の腕とmatchを閉じる波括弧の間だけ省略できる。

```suz
fn main() -> u8 {
    match true {
        true => {} false => {}
    }
    return 0
}
```

### flow-032 文のmatchとifは分岐の結果を捨てる

主根拠：`[branch.statement]`。

補助根拠：`[match.exhaustive]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    if true {
        1
    } else {
        false
    }
    match false {
        true => 1
        false => false
    }
    return 0
}
```

### flow-033 expr末尾に値がない

主根拠：`[block.value]`。

**期待：** コンパイルエラー。

末尾はログ文であり、値を持つ式ではない。

```suz
fn main() -> u8 {
    fix value = expr {
        log info `message`
    }
    return 0
}
```

### flow-034 exprの末尾をvoidの式にする

主根拠：`[block.value]`。

補助根拠：`[branch.expression]`。

**期待：** コンパイルエラー。

結果を変数に保存せず捨てる場合でも、expr自体はvoid以外の値を作る必要がある。

```suz
fn action() -> void {}

fn main() -> u8 {
    expr {
        action()
    }
    return 0
}
```

### flow-035 elseを次の行に置く

主根拠：`[expr.else-line]`。

**期待：** コンパイルエラー。

elseは直前の}と同じ行に置く。

```suz
fn main() -> u8 {
    if true {
    }
    else {
    }
    return 0
}
```

### flow-036 条件を順に評価し、成立後のelifを評価しない

主根拠：`[if.condition]`。

補助根拠：`[if.elif]`。

**期待：** 受理。終了コード 2。

1番目の条件はfalse、2番目はtrue。3番目の条件を評価せず、2番目の分岐から戻る。

```suz
fn check(id: u8, result: bool) -> bool {
    log info `${id}`
    return result
}

fn main() -> u8 {
    if check(1, false) {
        return 1
    } elif check(2, true) {
        return 2
    } elif check(3, true) {
        return 3
    } else {
        return 4
    }
}
```

標準エラー：

```text
info flow-036.suz:2: 1
info flow-036.suz:2: 2
```

### flow-037 ifの条件に整数を使う

主根拠：`[if.condition]`。

補助根拠：`[type.bool]`。

**期待：** コンパイルエラー。

整数の1はboolとして扱わない。

```suz
fn main() -> u8 {
    if 1 {
        return 1
    }
    return 0
}
```

### flow-038 評価されないelifの条件にもboolを要求する

主根拠：`[if.condition]`。

**期待：** コンパイルエラー。

先頭の条件がtrueでも、elifの条件の型は検査する。

```suz
fn main() -> u8 {
    if true {
        return 0
    } elif 1 {
        return 1
    } else {
        return 2
    }
}
```

### flow-039 whileの条件に整数を使う

主根拠：`[while.loop]`。

補助根拠：`[type.bool]`。

**期待：** コンパイルエラー。

整数の0はboolとして扱わない。

```suz
fn main() -> u8 {
    while 0 {}
    return 0
}
```

### flow-040 loop内のbreakとcontinue

主根拠：`[loop.unconditional]`。

補助根拠：`[loop.break]`、`[loop.continue]`。

**期待：** 受理。終了コード 3。

```suz
fn main() -> u8 {
    var count: u8 = 0
    loop {
        count += 1
        if count == 2 {
            continue
        }
        if count == 3 {
            break
        }
    }
    return count
}
```

### flow-041 for専用の範囲記法を変数に保存する

主根拠：`[for.range]`。

**期待：** コンパイルエラー。

..<の範囲記法はfor専用であり、値を作る式として使えない。

```suz
fn main() -> u8 {
    fix range = 0..<3
    return 0
}
```

### flow-042 範囲演算子の前後の空白

主根拠：`[for.range-spacing]`。

**期待：** コンパイルエラー。

..<は両端の式の境界に隣接する必要がある。

```suz
fn main() -> u8 {
    for i in 0 ..< 3 {
        log info `${i}`
    }
    return 0
}
```

### flow-043 範囲演算子の直後にコメントと改行を挟む

主根拠：`[for.range-spacing]`。

補助根拠：`[src.line-continuation]`。

**期待：** コンパイルエラー。

範囲演算子には両端への隣接が必要。ほかの二項演算子のように直後の改行で継続できない。

```suz
fn main() -> u8 {
    for i in 0..<// 終了値は次の行
        3 {}
    return 0
}
```

### flow-044 forの両端に異なる整数型を使う

主根拠：`[for.bounds]`。

補助根拠：`[type.numeric-conversion]`。

**期待：** コンパイルエラー。

開始値のu8と終了値のu16は一致しない。

```suz
fn main() -> u8 {
    fix start: u8 = 0
    fix end: u16 = 3
    for i in start..<end {}
    return 0
}
```

### flow-045 変数を使う範囲と最大値を含む範囲

主根拠：`[for.bounds]`。

補助根拠：`[type.context]`、`[for.variable]`。

**期待：** 受理。終了コード 4。

iとlastはu8。255の反復後に増分によるオーバーフローを起こさない。

```suz
fn main() -> u8 {
    fix end: u8 = 3
    fix top: u8 = 255
    var total: u8 = 0
    for i in 0..<end {
        total += i
    }
    for last in top..=top {
        total += 1
    }
    return total
}
```

### flow-046 空の範囲

主根拠：`[for.bounds]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    for i in 3..<3 {
        return 1
    }
    for j in 4..=3 {
        return 2
    }
    return 0
}
```

### flow-047 範囲の両端を左から1回だけ評価

主根拠：`[for.bounds]`。

補助根拠：`[expr.eval-order]`。

**期待：** 受理。終了コード 0。

```suz
fn lower() -> u8 {
    log info `lower`
    return 0
}

fn upper() -> u8 {
    log info `upper`
    return 2
}

fn main() -> u8 {
    for i in lower()..<upper() {
        log info `i=${i}`
    }
    return 0
}
```

標準エラー：

```text
info flow-047.suz:2: lower
info flow-047.suz:7: upper
info flow-047.suz:13: i=0
info flow-047.suz:13: i=1
```

### flow-048 変更可能な終了値をfixにコピーして範囲に使う

主根拠：`[for.bound-reference]`。

補助根拠：`[for.bounds]`。

**期待：** 受理。終了コード 3。

endの現在値をfixのlimitにコピーする。範囲はlimitを使うため、本体でendを変更しても3回反復する。

```suz
fn main() -> u8 {
    var end: u8 = 3
    fix limit = end
    var count: u8 = 0
    for i in 0..<limit {
        end = 0
        count += 1
    }
    return count
}
```

### flow-049 forの終了値にvarを直接使う

主根拠：`[for.bound-reference]`。

**期待：** コンパイルエラー。

endを範囲式から直接参照している。現在値を使う場合は先にfixへコピーする。

```suz
fn main() -> u8 {
    var end: u8 = 3
    for i in 0..<end {
        end = 0
    }
    return 0
}
```

### flow-050 範囲の開始値の関数引数にvarを使う

主根拠：`[for.bound-reference]`。

**期待：** コンパイルエラー。

開始値の式にある実引数も参照の検査対象になる。

```suz
fn identity(value: u8) -> u8 {
    return value
}

fn main() -> u8 {
    var start: u8 = 0
    for i in identity(start)..<3 {
        start = 1
    }
    return 0
}
```

### flow-051 範囲の終了値にvarのフィールドを使う

主根拠：`[for.bound-reference]`。

**期待：** コンパイルエラー。

limitsのフィールドへの参照もvarからの参照として扱う。

```suz
struct Limits { end: u8 }

fn main() -> u8 {
    var limits = Limits { end: 3 }
    for i in 0..<limits.end {
        limits.end = 0
    }
    return 0
}
```

### flow-052 ループ変数への代入

主根拠：`[for.variable]`。

**期待：** コンパイルエラー。

ループ変数は変更不可。

```suz
fn main() -> u8 {
    for i in 0..<3 {
        i = 2
    }
    return 0
}
```

### flow-053 forの外からループ変数を参照する

主根拠：`[for.variable]`。

**期待：** コンパイルエラー。

iを参照できるのはループ本体の中だけ。

```suz
fn main() -> u8 {
    fix end: u8 = 3
    for i in 0..<end {}
    return i
}
```

### flow-054 forのcontinueは次の値へ進む

主根拠：`[loop.continue]`。

補助根拠：`[for.bounds]`。

**期待：** 受理。終了コード 4。

iが2の反復では加算を飛ばす。1と3を加えた結果は4になる。

```suz
fn main() -> u8 {
    fix end: u8 = 3
    var total: u8 = 0
    for i in 1..=end {
        if i == 2 {
            continue
        }
        total += i
    }
    return total
}
```

### flow-055 ループの外にbreakを書く

主根拠：`[loop.jump-context]`。

**期待：** コンパイルエラー。

```suz
fn main() -> u8 {
    break
    return 0
}
```

### flow-056 ループの外にcontinueを書く

主根拠：`[loop.jump-context]`。

**期待：** コンパイルエラー。

```suz
fn main() -> u8 {
    continue
    return 0
}
```


## 9. 実行時の検査

### runtime-001 加算のオーバーフロー

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

加算結果256がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: u8) -> void {
    fix result = value + 1
}

fn main() -> u8 {
    operation(255)
    return 0
}
```

### runtime-002 減算のオーバーフロー

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

減算結果-1がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: u8) -> void {
    fix result = value - 1
}

fn main() -> u8 {
    operation(0)
    return 0
}
```

### runtime-003 乗算のオーバーフロー

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

乗算結果256がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: u8) -> void {
    fix result = value * 2
}

fn main() -> u8 {
    operation(128)
    return 0
}
```

### runtime-004 最小値の単項マイナス

主根拠：`[rt.int-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

128がi8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: i8) -> void {
    fix result = -value
}

fn main() -> u8 {
    operation(-128)
    return 0
}
```

### runtime-005 整数のゼロ除算

主根拠：`[rt.div-zero]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

除数が0。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(divisor: i32) -> void {
    fix result = 7 / divisor
}

fn main() -> u8 {
    operation(0)
    return 0
}
```

### runtime-006 整数のゼロによる剰余

主根拠：`[rt.div-zero]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

除数が0。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(divisor: i32) -> void {
    fix result = 7 % divisor
}

fn main() -> u8 {
    operation(0)
    return 0
}
```

### runtime-007 最小値を-1で割る

主根拠：`[rt.div-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

符号付き整数の最小値と-1の組み合わせ。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: i8) -> void {
    fix result = value / -1
}

fn main() -> u8 {
    operation(-128)
    return 0
}
```

### runtime-008 最小値と-1の剰余

主根拠：`[rt.div-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

剰余も最小値と-1の組み合わせでパニックする。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: i8) -> void {
    fix result = value % -1
}

fn main() -> u8 {
    operation(-128)
    return 0
}
```

### runtime-009 負のシフト数

主根拠：`[rt.shift-count]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

右辺が負。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(count: i32) -> void {
    fix result = 1 << count
}

fn main() -> u8 {
    operation(-1)
    return 0
}
```

### runtime-010 ビット幅以上のシフト数

主根拠：`[rt.shift-count]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

u8のビット幅と同じ8。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: u8) -> void {
    fix result = value >> 8
}

fn main() -> u8 {
    operation(1)
    return 0
}
```

### runtime-011 左シフトの結果が範囲外

主根拠：`[rt.shift-overflow]`。

補助根拠：`[rt.panic]`。

**期待：** 受理。実行時パニック。終了コード 101。

256がu8の範囲外。 標準エラーにファイル・行・列と理由を出す。文言の完全一致は要求しない。

```suz
fn operation(value: u8) -> void {
    fix result = value << 1
}

fn main() -> u8 {
    operation(128)
    return 0
}
```

### runtime-012 浮動小数点のゼロ除算は無限大かNaNになる

主根拠：`[rt.float]`。

補助根拠：`[expr.nan-comparison]`。

**期待：** 受理。終了コード 0。

正の値を正のゼロで割るとinf、負のゼロで割ると-infになる。0.0 / 0.0はNaNになり、自分自身との!=がtrueになる。いずれもパニックしない。

```suz
fn main() -> u8 {
    fix zero = 0.0
    fix negative_zero = -0.0
    fix positive = 1.0 / zero
    fix negative = 1.0 / negative_zero
    fix invalid = zero / zero
    return ifexpr positive == inf && negative == -inf && invalid != invalid { 0 } else { 1 }
}
```

### runtime-013 f32の加算でも偶数側へ丸める

主根拠：`[rt.float-rounding]`。

補助根拠：`[type.float]`。

**期待：** 受理。終了コード 0。

2つのオペランドはともにf32で正確に表せる。加算の数学的な結果16777217は16777216に丸められる。

```suz
fn main() -> u8 {
    fix base: f32 = 16_777_216.0
    fix increment: f32 = 1.0
    fix result = base + increment
    return ifexpr result == base { 0 } else { 1 }
}
```

### runtime-014 浮動小数点から整数への変換は切り捨て後に範囲を検査

主根拠：`[rt.as-float-int]`。

補助根拠：`[expr.cast-fallback]`。

**期待：** 受理。終了コード 0。

255.75は255に、-0.75は0になってu8へ変換できる。256.0と-1.0は範囲外なので代替値になる。

```suz
fn main() -> u8 {
    fix upper = 255.75
    fix near_zero = -0.75
    fix too_large = 256.0
    fix negative = -1.0
    fix a = upper as u8 or 9
    fix b = near_zero as u8 or 9
    fix c = too_large as u8 or 9
    fix d = negative as u8 or 9
    return ifexpr a == 255 && b == 0 && c == 9 && d == 9 { 0 } else { 1 }
}
```

### runtime-015 NaN・無限大から整数への変換失敗

主根拠：`[rt.as-float-int]`。

補助根拠：`[expr.cast-fallback]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    fix first = nan as i32 or 7
    fix second = inf as i32 or 8
    return ifexpr first == 7 && second == 8 { 0 } else { 1 }
}
```

### runtime-016 整数からf32への変換で偶数側へ丸める

主根拠：`[rt.as-int-float]`。

補助根拠：`[rt.float-rounding]`。

**期待：** 受理。終了コード 0。

この大きさのf32では隣り合う値の間隔が2になる。16777217は16777216へ、16777219は16777220へ丸める。どちらも距離が等しい候補のうち仮数の最下位ビットが0の側を選ぶ。

```suz
fn main() -> u8 {
    fix first: i32 = 16_777_217
    fix second: i32 = 16_777_219
    fix down = first as f32
    fix up = second as f32
    return ifexpr down == 16_777_216.0 && up == 16_777_220.0 { 0 } else { 1 }
}
```

### runtime-017 enumの値が変換先の整数型に収まらない

主根拠：`[rt.as-enum-int]`。

補助根拠：`[enum.variant]`、`[expr.cast-risk]`、`[expr.cast-fallback]`。

**期待：** 受理。終了コード 0。

基底型はu8なので、V128の値128まで宣言できる。i8へ変換すると、V127の127は成功し、V128の128は範囲外となって代替値-1を使う。

```suz
enum Code: u8 {
    V000, V001, V002, V003, V004, V005, V006, V007,
    V008, V009, V010, V011, V012, V013, V014, V015,
    V016, V017, V018, V019, V020, V021, V022, V023,
    V024, V025, V026, V027, V028, V029, V030, V031,
    V032, V033, V034, V035, V036, V037, V038, V039,
    V040, V041, V042, V043, V044, V045, V046, V047,
    V048, V049, V050, V051, V052, V053, V054, V055,
    V056, V057, V058, V059, V060, V061, V062, V063,
    V064, V065, V066, V067, V068, V069, V070, V071,
    V072, V073, V074, V075, V076, V077, V078, V079,
    V080, V081, V082, V083, V084, V085, V086, V087,
    V088, V089, V090, V091, V092, V093, V094, V095,
    V096, V097, V098, V099, V100, V101, V102, V103,
    V104, V105, V106, V107, V108, V109, V110, V111,
    V112, V113, V114, V115, V116, V117, V118, V119,
    V120, V121, V122, V123, V124, V125, V126, V127,
    V128,
}

fn convert(value: Code) -> i8 {
    fix converted = value as i8 or -1
    return converted
}

fn main() -> u8 {
    fix success = convert(Code.V127)
    fix failure = convert(Code.V128)
    return ifexpr success == 127 && failure == -1 { 0 } else { 1 }
}
```

### runtime-018 整数からenumへの変換は基底型へラップしない

主根拠：`[rt.as-int-enum]`。

補助根拠：`[expr.enum-conversion]`。

**期待：** 受理。終了コード 0。

256をu8へラップすれば0になるが、enumへの変換では元の256を検査する。対応する列挙子がないので代替値のSawになる。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    fix raw: u16 = 256
    fix wave = raw as Wave or Wave.Saw
    return ifexpr wave == Wave.Saw { 0 } else { 1 }
}
```


## 10. ログ

### log-001 テンプレートを変数に保存

主根拠：`[log.template]`。

**期待：** コンパイルエラー。

テンプレートはlog infoの出力内容専用。

```suz
fn main() -> u8 {
    fix text = `hello`
    return 0
}
```

### log-002 テンプレートをソース上で複数行にする

主根拠：`[log.template-line]`。

**期待：** コンパイルエラー。

開くバッククォートと閉じるバッククォートは同じ行に書く必要がある。

```suz
fn main() -> u8 {
    log info `first
second`
    return 0
}
```

### log-003 ログに構造体を埋め込む

主根拠：`[log.interpolation]`。

**期待：** コンパイルエラー。

構造体は埋め込める型に含まれない。

```suz
struct Point { x: i32 }
fn main() -> u8 {
    fix p = Point { x: 1 }
    log info `${p}`
    return 0
}
```

### log-004 voidの式をログに埋め込む

主根拠：`[log.interpolation]`。

**期待：** コンパイルエラー。

voidはログに埋め込める型ではない。

```suz
fn action() -> void {}

fn main() -> u8 {
    log info `${action()}`
    return 0
}
```

### log-009 埋め込み式から別のログを出力する

主根拠：`[log.interpolation]`。

補助根拠：`[log.output]`。

**期待：** 受理。終了コード 0。

外側のログは、valueの評価を終えてから出力を始める。valueが出すinnerの行が先に完了し、その後にouterの行が続く。

```suz
fn value() -> u8 {
    log info `inner`
    return 3
}

fn main() -> u8 {
    log info `outer: ${value()}`
    return 0
}
```

標準エラー：

```text
info log-009.suz:2: inner
info log-009.suz:7: outer: 3
```

### log-010 埋め込み式の評価中にパニックする

主根拠：`[log.interpolation]`。

補助根拠：`[rt.div-zero]`、`[rt.panic]`。

**期待：** 実行時パニック。終了コード 101。

divideはinnerを出力した後、ゼロ除算でパニックする。外側のログは埋め込み式の評価が完了していないため、接頭辞も本文も出力しない。

```suz
fn divide(divisor: i32) -> i32 {
    log info `inner`
    return 1 / divisor
}

fn main() -> u8 {
    log info `outer: ${divide(0)}`
    return 0
}
```

標準エラーは次の行で始まり、その後にlog-010.suzの3行目の除算でゼロ除算が起きたことを示すパニック診断が続く。

```text
info log-010.suz:2: inner
```

### log-005 定義されていないテンプレートのエスケープ

主根拠：`[log.escape]`。

**期待：** コンパイルエラー。

\qは定義されているエスケープに含まれない。

```suz
fn main() -> u8 {
    log info `invalid: \q`
    return 0
}
```

### log-006 ログのエスケープと本文のコメント記号

主根拠：`[log.escape]`。

補助根拠：`[log.output]`、`[log.template-text]`。

**期待：** 受理。終了コード 0。

```suz
fn main() -> u8 {
    log info `literal: \${value} // text\nnext`
    return 0
}
```

標準エラー：

```text
info log-006.suz:2: literal: ${value} // text
next
```

### log-007 bool・enum・整数のログ表示

主根拠：`[log.value-format]`。

補助根拠：`[log.output]`。

**期待：** 受理。終了コード 0。

enumは型名を付けず列挙子名だけを表示する。整数の区切りのアンダースコアは表示しない。

```suz
enum Wave: u8 { Sine, Saw }

fn main() -> u8 {
    log info `${true} ${false} ${Wave.Saw} ${1_234} ${-7}`
    return 0
}
```

標準エラー：

```text
info log-007.suz:4: true false Saw 1234 -7
```

### log-008 浮動小数点の通常表記と小さい値の指数表記

主根拠：`[log.float-format]`。

補助根拠：`[log.value-format]`。

**期待：** 受理。終了コード 0。

整数に見える値にも小数点を付ける。絶対値が1e-6未満の0でない値は指数表記にする。

```suz
fn main() -> u8 {
    log info `${1.0} ${0.125} ${1.0e-8} ${-1.0e-8}`
    return 0
}
```

標準エラー：

```text
info log-008.suz:2: 1.0 0.125 1.0e-8 -1.0e-8
```


## 11. 整数演算の診断設定

警告設定は `--warn-unhandled-arithmetic`、強制設定は `--deny-unhandled-arithmetic`。

### diag-001 安全な値でもorなしの整数演算を診断

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**期待：** 通常：受理。警告設定：受理して警告。強制設定：コンパイルエラー。

対象は+の1箇所。通常時の終了コードは3。

```suz
fn main() -> u8 {
    fix value: u8 = 1 + 2
    return value
}
```

### diag-002 最も外側のorだけでは内側の演算が残る

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**期待：** 通常：受理。警告設定：受理して警告。強制設定：コンパイルエラー。

対象は内側の+の1箇所。通常時の終了コードは4。

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix value = ((a + 1) * 2) or 0
    return value
}
```

### diag-003 複合代入も診断対象

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**期待：** 通常：受理。警告設定：受理して警告。強制設定：コンパイルエラー。

対象は+=の1箇所。通常時の終了コードは1。

```suz
fn main() -> u8 {
    var count: u8 = 0
    count += 1
    return count
}
```

### diag-004 実行されない分岐の整数演算も診断する

主根拠：`[diagnostic.arithmetic]`。

補助根拠：`[diagnostic.arithmetic-target]`。

**期待：** 通常：受理。警告設定：受理して警告。強制設定：コンパイルエラー。

対象は+の1箇所。分岐が実行されなくても診断対象になる。通常時の終了コードは0。

```suz
fn main() -> u8 {
    fix top: u8 = 255
    if false {
        fix result = top + 1
    }
    return 0
}
```

### diag-005 全演算に失敗処理を付ける

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.arithmetic-scope]`。

**期待：** 3設定とも受理。対象の警告なし。終了コード 4。

```suz
fn main() -> u8 {
    fix a: u8 = 1
    fix value = (((a + 1) or 0) * 2) or 0
    return value
}
```

### diag-006 ラップ演算は整数演算の警告対象に含まれない

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.wrap]`、`[expr.as-trunc]`。

**期待：** 3設定とも受理。対象の警告なし。終了コード 1。

```suz
fn main() -> u8 {
    fix value: u8 = 255 +% 1
    fix low = 257 as% u8
    return value +% low
}
```

### diag-007 右辺の失敗処理とは別に複合代入を診断する

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.arithmetic-scope]`。

**期待：** 通常：受理。警告設定：受理して警告。強制設定：コンパイルエラー。

除算には対応するorがあり、診断対象は+=の1箇所。通常時の終了コードは11。

```suz
fn main() -> u8 {
    var count: u8 = 10
    fix divisor: u8 = 1
    count += (1 / divisor) or 0
    return count
}
```

### diag-008 キャストのor必須ルールは診断設定で変わらない

主根拠：`[diagnostic.arithmetic-target]`。

補助根拠：`[expr.cast-handler]`。

**期待：** 3設定ともコンパイルエラー。

rawは定数式ではなく、i64からu8への変換にはorが必要。警告設定でもエラーを警告には変更しない。

```suz
fn main() -> u8 {
    fix raw: i64 = 1
    fix value = raw as u8
    return value
}
```
