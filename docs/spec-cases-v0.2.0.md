# Suz v0.2.0 追加仕様例集

[追加仕様](spec-v0.2.0.md)に対する入力と期待結果をまとめる。期待結果は仕様から導く。Suzソースの例は[仕様例集](spec-cases.md)の書式に従い、IDは `text-NNN` とする。通常のソースは独立したファイルとして保存する。ランタイムとツールの検証は、別の表で入力・操作と期待結果を定める。

## 恒久的な反映先

この文書は統合用の一時文書であり、v0.2.0では保持し、その次のバージョンへ移る際に削除する。例と検証条件は次の場所へ移す。「新規」と記したパスは反映先であり、現時点でテストを実装したことを意味しない。

| 対象 | 恒久的な反映先 | 反映する内容 |
|---|---|---|
| `text-001`〜`text-025` | `docs/spec-cases/text.md`（新規） | 主根拠、ソース、期待結果、診断の規則と位置、出力全文。 |
| ソース例の登録 | `docs/spec-cases.md` | `text` 分野の目次・件数・全体の件数。 |
| 既存例の変更 | `docs/spec-cases/log.md` など、各IDの既存ファイル | 下表の期待結果と規則タグの変更。リリース済みIDを振り直さない。 |
| ランタイム検証の登録 | `compiler/tests/spec_cases.rs` | `text_runtime`・`text_struct_runtime` モジュールの登録。 |
| 検証方法の説明 | `compiler/tests/README.md` | 各テストファイルの責務と、普段・リリースの検査での扱い。 |

以下のテスト名は恒久的な反映先を特定するための名前である。テストには、この文書の表の入力・操作・期待結果をすべて移す。検証用のCと環境層もテスト側に保持する。規則の根拠は統合後の `docs/spec.md`、`docs/runtime.md`、`docs/tool.md` を参照し、この文書の読み込みや `pool-NNN` だけの参照に依存させない。

| ID | テストファイル | テスト名 |
|---|---|---|
| pool-001 | `compiler/tests/spec_cases/text_runtime.rs`（新規） | `interning_preserves_identity_in_both_registration_orders` |
| pool-002 | 同上 | `hash_collisions_preserve_distinct_contents` |
| pool-003 | 同上 | `unused_text_is_not_registered` |
| pool-004 | 同上 | `pool_accounts_dynamic_storage_and_metadata` |
| pool-005 | 同上 | `existing_text_is_reused_at_the_pool_limit` |
| pool-006 | 同上 | `pool_limit_is_checked_before_allocation` |
| pool-007 | 同上 | `table_growth_accounts_both_allocations` |
| pool-008 | 同上 | `temporary_log_text_does_not_accumulate` |
| pool-009 | `compiler/tests/spec_cases/text_struct_runtime.rs` | `struct_text_parsing_does_not_intern_failed_inputs_and_frees_temporary_storage` |
| pool-010 | `compiler/tests/spec_cases/text_runtime.rs` | `allocation_failures_report_the_operation_location` |
| pool-010 | `compiler/tests/spec_cases/text_struct_runtime.rs` | `struct_text_parsing_does_not_intern_failed_inputs_and_frees_temporary_storage` |
| pool-011 | `compiler/tests/spec_cases/text_struct_casts.rs` | `struct_text_casts_preserve_constants_scalar_types_and_lazy_evaluation` |
| pool-012 | `compiler/tests/spec_cases/native_cli.rs` | `build_and_run_apply_text_pool_limits` |
| pool-012 | `compiler/tests/spec_cases/emit.rs` | `exported_text_pool_limits_preserve_units_defaults_and_full_u64_range`・`emitted_c_preserves_text_pool_limits` |
| pool-013 | `compiler/tests/spec_cases/cli.rs` | `invalid_text_pool_limits_are_rejected` |

正式なソース例は既存の `execute_spec_cases` と `execute_optimized_spec_cases` から検証する。ランタイム・ツールの境界条件は通常テストに登録し、リリース検査でも実行する。プール上限の引数検査と生成Cへの設定の反映は通常テストで確認し、設定別・コマンド別にCをビルドして実行する検査はリリース検査に置く。大量の反復を追加する場合は、その反復部分をリリース検査へ分ける。

この文書を削除する前に、25例のカタログへの登録と13件の検証条件のテストへの反映を照合し、通常・リリース検査の成功を確認する。目録と他の文書に残る一時文書へのリンクも更新する。

## 既存例との関係

| 既存例 | v0.2.0での扱い |
|---|---|
| `log-001` テンプレートを変数に保存 | コンパイルエラーから受理へ変更する。値としての利用は本書の `text-001`・`text-002` でも確認する。 |
| `log-002`・`log-007`・`log-008` | 改行・不正エスケープ・本文の規則は維持し、主根拠を対応する `text.*` に変更する。 |
| `log-003`・`log-004`・`log-009`・`log-010` | 埋め込み可能な型と数値表示の既存確認を維持し、共通規則のタグを `text.*` に変更する。 |
| `log-005`・`log-006` | ログの評価順序とパニック前の出力抑止の確認を維持する。根拠を `text.interpolation` と `log.evaluation` に整理する。 |
| `expr-049`〜`expr-058` | 代替値の型と脱出制御の既存規則を維持する。本書ではtext固有の経路だけを追加する。 |

## 追加例

### text-001 関数から返したtextを構造体へ保存してコピーする

主根拠：`[text.copy]`。

関数の復帰、元変数への再代入、構造体フィールドの更新後もコピー先の内容が変わらない。

**ソース：**

```suz
struct Message {
    body: text,
}
fn make(name: text) -> text {
    return `Hello, ${name}`
}
fn main() -> u8 {
    var name = `Suz`
    var first = Message { body: make(name) }
    fix copied = first
    name = `World`
    first.body = make(name)
    if copied.body == `Hello, Suz` && first.body == `Hello, World` { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-002 異なる生成経路の同値と空文字列を比較する

主根拠：`[text.compare]`。

これは比較結果の確認である。実体の共有そのものは後述のランタイム検証で確認する。

**ソース：**

```suz
fn identity(value: text) -> text { return value }
fn main() -> u8 {
    fix name = identity(`Suz`)
    fix generated = `${name}`
    fix empty = identity(``)
    if name == generated && generated != `suz` && empty == `` { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-003 Unicodeの辞書順と接頭辞を比較する

主根拠：`[text.order]`。

「é」はU+00E9、分解形はU+0065とU+0301。「𐀀」はU+10000、「」はU+E000である。UTF-16のコード単位順との取り違えとUnicode正規化を検出する。

**ソース：**

```suz
fn main() -> u8 {
    fix composed = `é`
    fix decomposed = `é`
    if not (`` < `a` && `a` < `aa` && `A` < `a` <= `a`) { return 1 }
    if not (`あ` < `い` && `𐀀` > ``) { return 2 }
    if composed == decomposed { return 3 }
    if not (`a\0b` > `a` && `\0` < `\t`) { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-004 textを加算演算で連結しようとする

主根拠：`[text.operations]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `a` + `b`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.operations]` | 2 | 21 |

### text-005 数値とboolをSuzの表記で読み取る

主根拠：`[text.cast-format]`。

**ソース：**

```suz
fn main() -> u8 {
    fix integer = `1_000` as u16 or 0
    fix minimum = `-128` as i8 or 0
    fix fraction = `1.2_5e+1` as f64 or 0.0
    fix integral = `1` as f32 or 0.0
    fix yes = `true` as bool or false
    fix no = `false` as bool or true
    if integer == 1000 && minimum == -128 && fraction == 12.5 && integral == 1.0 && yes && not no { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-006 数値とboolの入力全体を検査する

主根拠：`[text.cast-format]`。

**ソース：**

```suz
fn invalid(value: text) -> bool {
    return (value as u8 or 99) == 99
}
fn main() -> u8 {
    if not (invalid(``) && invalid(` 1`) && invalid(`1 `)) { return 1 }
    if not (invalid(`1\0`) && invalid(`1+2`) && invalid(`(1)`)) { return 2 }
    if not (invalid(`+1`) && invalid(`1__0`) && invalid(`1.0`)) { return 3 }
    if not (invalid(`0x10`) && invalid(`1//comment`)) { return 4 }
    if (`True` as bool or false) { return 5 }
    if (`1` as bool or false) { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-007 整数の上下限と浮動小数点の丸めを確認する

主根拠：`[text.cast-range]`。

**ソース：**

```suz
fn main() -> u8 {
    fix maximum = `18446744073709551615` as u64 or 0
    fix overflow = `18446744073709551616` as u64 or 7
    fix underflow = `-129` as i8 or 7
    fix unsigned = `-0` as u8 or 7
    fix rounded = `16777217` as f32 or 0.0
    fix too_large = `1.0e100` as f32 or 7.0
    fix tiny = `1.0e-1000` as f64 or 7.0
    fix negative_zero = `-0` as f64 or 7.0
    fix special = `nan` as f64 or 0.0
    fix infinity = `-inf` as f64 or 0.0
    if maximum != 18446744073709551615 || overflow != 7 || underflow != 7 || unsigned != 7 { return 1 }
    if rounded != 16777216.0 || too_large != 7.0 || tiny != 0.0 { return 2 }
    if 1.0 / negative_zero != -inf || special == special || infinity != -inf { return 3 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-008 成功する定数文字列のasにもorを要求する

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
const VALUE: u8 = `1` as u8
fn main() -> u8 { return VALUE }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.cast-handler]` | 1 | 23 |

### text-009 桁数の大きな整数と負数をas%でラップする

主根拠：`[text.cast-wrap]`。

**ソース：**

```suz
fn main() -> u8 {
    fix large = `18446744073709551617` as% u8 or 7
    fix negative = `-1` as% u8 or 7
    fix signed = `255` as% i8 or 7
    fix invalid = `18446744073709551617x` as% u8 or 7
    fix fraction = `1.0` as% u8 or 7
    if large == 1 && negative == 255 && signed == -1 && invalid == 7 && fraction == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-010 成功する定数文字列のas%にもorを要求する

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `256` as% u8
    return value
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.cast-handler]` | 2 | 23 |

### text-011 boolへのas%を拒否する

主根拠：`[text.cast-wrap]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `true` as% bool or false
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[text.cast-wrap]` | 2 | 24 |

### text-012 text変換の評価回数と代替値の遅延評価

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
fn source(value: text) -> text {
    log info `source`
    return value
}
fn fallback() -> u8 {
    log info `fallback`
    return 7
}
fn main() -> u8 {
    fix first = source(`1`) as u8 or fallback()
    fix second = source(`bad`) as% u8 or fallback()
    if first == 1 && second == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-012.suz:2: source
info text-012.suz:2: source
info text-012.suz:6: fallback
```

### text-013 text変換のor returnで呼び出し元に戻る

主根拠：`[text.cast-handler]`。

**ソース：**

```suz
fn parse(value: text) -> u8 {
    fix result = value as u8 or return 7
    return result
}
fn main() -> u8 {
    if parse(`2`) == 2 && parse(`bad`) == 7 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-014 テンプレートの埋め込みにor returnを置く

主根拠：`[expr.or-return-position]`。

**ソース：**

```suz
fn main() -> u8 {
    fix value = `${`bad` as u8 or return 1}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[expr.or-return-position]` | 2 | 35 |

### text-015 構造体へ入れ子の値とtextと列挙子を読み取る

主根拠：`[text.cast-struct]`。

**ソース：**

```suz
enum Mode: u8 { Fast, Slow }
struct Point {
    x: i32,
    y: i32,
}
struct Item {
    name: text,
    mode: Mode,
    point: Point,
}
fn main() -> u8 {
    fix input = `Item { point: Point { y: 2, x: 1 }, mode: Mode.Fast, name: \`Suz\` }`
    fix item = input as Item or return 1
    if item.name == `Suz` && item.mode == Mode.Fast && item.point.x == 1 && item.point.y == 2 { return 0 }
    return 2
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-016 構造体の型名とフィールドの過不足を検査する

主根拠：`[text.cast-struct]`。

**ソース：**

```suz
struct Point {
    x: u8,
    y: u8,
}
fn invalid(value: text) -> bool {
    fix point = value as Point or Point { x: 99, y: 99 }
    return point.x == 99 && point.y == 99
}
fn main() -> u8 {
    if not (invalid(`Other { x: 1, y: 2 }`) && invalid(`Point { x: 1 }`)) { return 1 }
    if not (invalid(`Point { x: 1, x: 2, y: 3 }`) && invalid(`Point { x: 1, y: 2, z: 3 }`)) { return 2 }
    if not (invalid(`Point { x: 256, y: 2 }`) && invalid(`Point { x: 1+2, y: 3 }`)) { return 3 }
    if not (invalid(`Point { x: 1, y: 2 }tail`) && invalid(`Point { x: 1, y: wrong }`)) { return 4 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-017 構造体内の文字列でエスケープを展開する

主根拠：`[text.cast-struct-text]`。

**ソース：**

```suz
struct Label {
    name: text,
}
fn main() -> u8 {
    fix input = `Label { name: \`a\\0b\` }`
    fix label = input as Label or return 1
    if label.name == `a\0b` && label.name != `a` { return 0 }
    return 2
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-018 変換入力内の不正エスケープと埋め込みを失敗にする

主根拠：`[text.cast-struct-text]`。

ソースのエスケープは合法である。復元された変換入力の不正をorが処理する。

**ソース：**

```suz
struct Label {
    name: text,
}
fn invalid(value: text) -> bool {
    fix label = value as Label or Label { name: `fallback` }
    return label.name == `fallback`
}
fn main() -> u8 {
    if not invalid(`Label { name: \`\\q\` }`) { return 1 }
    if not invalid(`Label { name: \`\${1}\` }`) { return 2 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-019 構造体入力の行数に応じて末尾カンマを検査する

主根拠：`[text.cast-struct-layout]`。

**ソース：**

```suz
struct Point {
    x: u8,
    y: u8,
}
fn valid(value: text) -> bool {
    fix point = value as Point or return false
    return point.x == 1 && point.y == 2
}
fn main() -> u8 {
    if not valid(`Point { x: 1, y: 2 }`) { return 1 }
    if not valid(`Point {\nx: 1,\ny: 2,\n}`) { return 2 }
    if not valid(`Point {\r\nx: 1,\r\ny: 2,\r\n}`) { return 3 }
    if valid(`Point { x: 1, y: 2, }`) { return 4 }
    if valid(`Point {\nx: 1,\ny: 2\n}`) { return 5 }
    if valid(`Point { x: 1\n, y: 2,\n}`) { return 6 }
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-020 constでテンプレートと型変換を評価する

主根拠：`[const.text-cast]`。

**ソース：**

```suz
struct Label {
    name: text,
}
const PRODUCT = `Suz`
const MAJOR: u8 = 1
const LABEL = `${PRODUCT} ${MAJOR}`
const COUNT: u8 = `256` as u8 or 10
const WRAPPED = `257` as% u8 or 10
const OBJECT = `Label { name: \`Suz\` }` as Label or Label { name: `` }
fn main() -> u8 {
    if LABEL == `Suz 1` && COUNT == 10 && WRAPPED == 1 && OBJECT.name == PRODUCT { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-021 textをmatchとmatchexprで分岐する

主根拠：`[text.match]`。

**ソース：**

```suz
fn classify(value: text) -> u8 {
    return matchexpr value {
        `ready` => 1
        `busy` => 2
        _ => 3
    }
}
fn main() -> u8 {
    var result: u8 = 0
    match `busy` {
        `ready` => { result = 1 }
        `busy` => { result = 2 }
        _ => { result = 3 }
    }
    if result == 2 && classify(`ready`) == 1 && classify(`other`) == 3 { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

### text-022 内容が等しいtextパターンの重複を拒否する

主根拠：`[match.pattern]`。

**ソース：**

```suz
const NAME = `ready`
fn main() -> u8 {
    match `ready` {
        NAME => { return 0 }
        `${NAME}` => { return 1 }
        _ => { return 2 }
    }
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.pattern]` | 5 | 9 |

### text-023 textのmatchに最後のワイルドカードを要求する

主根拠：`[match.exhaustive]`。

**ソース：**

```suz
fn main() -> u8 {
    match `ready` {
        `ready` => { return 0 }
    }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | コンパイルエラー | — | 0 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[match.exhaustive]` | 2 | 5 |

### text-024 text生成でも埋め込みを左から右へ1回ずつ評価する

主根拠：`[text.interpolation]`。

**ソース：**

```suz
fn piece(value: u8) -> text {
    log info `${value}`
    return `${value}`
}
fn main() -> u8 {
    fix value = `${piece(1)}:${piece(2)}`
    if value == `1:2` { return 0 }
    return 1
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-024.suz:2: 1
info text-024.suz:2: 2
```

### text-025 textをログへ埋め込む

主根拠：`[log.temporary-text]`。

**ソース：**

```suz
fn main() -> u8 {
    fix message = `Hello`
    log info `${message}`
    log info `count: ${1}`
    return 0
}
```

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**標準エラー：**

```text
info text-025.suz:3: Hello
info text-025.suz:4: count: 1
```

## ランタイムとツールの検証

以下はCランタイムまたはCLIを直接検証するケースである。`text` のアドレスやプール計測値をSuzのプログラムから取得する機能は使わない。環境層の確保・解放関数で要求サイズ、確保失敗、未解放領域を観測し、ランタイムの共有実体と計測値を照合する。各ケースは独立したプロセスで実行する。

| ID | 根拠 | 入力・操作 | 期待結果 |
|---|---|---|---|
| pool-001 | `[runtime.text-intern]` | 先に動的に `Suz` を生成し、後から同じ内容のconstant textを評価する。逆順でも実行する。 | 各実行で同じ実体を共有する。既存の実体を置き換えず、重複した文字列本体を保持しない。 |
| pool-002 | `[runtime.text-intern]` | 検索用ハッシュを同値にできるランタイム試験で、異なる2つの内容を登録する。 | 別の実体となる。各内容を再度生成すると、それぞれ対応する実体を再利用する。 |
| pool-003 | `[runtime.text-init]` | 未使用のconst、呼ばれない関数、実行されない分岐にそれぞれ固有の文字列を置く。実行される値は空文字列を含めて別に置く。 | 初期化時と実行後の登録集合に、未使用の内容が入らない。実際に使った内容だけが登録される。 |
| pool-004 | `[runtime.text-accounting]` | constant textを1つ使い、続いて別内容のdynamic textを生成する。実体と管理領域の確保要求を照合する。 | 静的な本体は計測せず、動的な本体・管理領域・未使用部分・余白を要求サイズで計測する。 |
| pool-005 | `[runtime.text-reuse]` | 管理領域拡張後の計測値と同じ値を上限として設定できるランタイム試験で、登録済みの内容を再利用する。 | 成功する。重複取得のためだけの表拡張や本体確保を行わない。 |
| pool-006 | `[rt.text-allocation]` | 初期化可能な容量を与え、次の新規登録に必要な追加確保だけが上限を超える状態を作る。 | 追加確保を環境層へ要求する前にパニックする。終了コード101。診断は生成元のバッククォートを指し、上限・現在値・必要量を含む。 |
| pool-007 | `[runtime.text-accounting]` | 検索表の拡張で旧領域と新領域を同時に保持させ、その合計が上限を超える条件と収まる条件を作る。 | 超える条件ではパニックし、収まる条件では成功する。旧領域の解放後は要求サイズ分を計測値から差し引く。 |
| pool-008 | `[log.temporary-text]` | ループで数値だけを埋め込み、毎回異なる本文をログへ1000回出力する。 | ログ本文はプールに登録されない。一時領域の未解放量が出力回数に比例して増えない。 |
| pool-009 | `[text.cast-struct-result]` | 未登録のtextフィールドを先に持ち、後続フィールドが不正な構造体入力を繰り返し変換する。 | 毎回orの代替値を使う。入力中のtextを新たに登録せず、解析用一時領域を保持し続けない。 |
| pool-010 | `[rt.text-location]` | 指定した確保要求だけを失敗させる環境層を用い、初期化・dynamic text生成・temporary text生成・構造体変換で確保を行う箇所をそれぞれ検査する。初期化が動的確保を行わない実装では、その経路の確保失敗は対象としない。 | 終了コード101。位置は順にmainのfn・バッククォート・log・as。診断経路で追加の動的確保を要求しない。 |
| pool-011 | `[text.nul]` | `text-017`で得た内容をログへ出力し、環境層へ渡る長さとバイト列を記録する。 | 本文の `61 00 62` を欠落なく出力する。NULで打ち切らない。 |
| pool-012 | `[tool.text-pool-limit]` | build・run・emit-cで省略、256M、2G、unlimitedを指定する。emit-cの生成物もビルドする。 | 省略は1,073,741,824バイト。各設定が実行時のプールへ渡り、unlimitedには有限のプール上限を設けない。 |
| pool-013 | `[tool.text-pool-size]` | 0、-1、1m、1.5G、18446744073709551616、18446744073709551615G、同一オプションの重複を指定する。 | 使用方法のエラーとなる。数値や単位換算がラップしない。 |

具体的な検索表の容量や構造体サイズは実装に依存する。上限境界の試験は実際の確保要求を基準に条件を作り、特定の管理領域サイズを言語の仕様として固定しない。

## 目次と件数

| 分野 | ID接頭辞 | 対応する仕様 | 件数 |
|---|---|---|---:|
| Suzソースの追加例 | `text` | text・変換・定数・分岐・ログ | 25 |
| 合計 | | | 25 |

ランタイム・ツールの検証は `pool-001`〜`pool-013` の13件であり、Suzソースの例の件数には含めない。
