# 変更履歴

利用者に関わる変更をバージョンごとに記載する。

## v0.3.3

- 要素数・位置の整数型 `len_t` と、Unicodeスカラー値の個数を返す `len` を追加し、両方を予約語にした。（[#39](https://github.com/TappoyGK/suz/issues/39)・[#63](https://github.com/TappoyGK/suz/pull/63)）
- 文字列の切り出しで、開始位置と終了位置に使う整数の型をそろえる必要をなくした。（[#39](https://github.com/TappoyGK/suz/issues/39)・[#63](https://github.com/TappoyGK/suz/pull/63)）
- 一つの `symbol` のUnicodeスカラー値の個数の上限を2^31−1にした。（[#39](https://github.com/TappoyGK/suz/issues/39)・[#63](https://github.com/TappoyGK/suz/pull/63)）
- 数値型の暗黙の拡大変換、途中結果を切り詰めない整数演算、精度を失わない整数・浮動小数点数の比較を追加した。（[#32](https://github.com/TappoyGK/suz/issues/32)・[#35](https://github.com/TappoyGK/suz/pull/35)）
- `int`・`float` による汎用数値関数を追加した。（[#34](https://github.com/TappoyGK/suz/issues/34)・[#41](https://github.com/TappoyGK/suz/pull/41)）
- 値域で結果が決まる比較と、整数除算を浮動小数点演算に使う箇所への警告を追加した。（[#32](https://github.com/TappoyGK/suz/issues/32)・[#35](https://github.com/TappoyGK/suz/pull/35)）
- 理由付きの警告抑制 `allow`、`--log-allow`、`log warn` を追加した。環境層のログレベルを `suz_log_level` 列挙型に変更した。（[#31](https://github.com/TappoyGK/suz/issues/31)・[#33](https://github.com/TappoyGK/suz/pull/33)）
- 生成CをC23へ移行し、C23と128ビットの `_BitInt` に対応したClangを必須にした。（[#26](https://github.com/TappoyGK/suz/issues/26)・[#27](https://github.com/TappoyGK/suz/pull/27)）
- 浮動小数点演算のベクトル化を可能にし、`SUZ_CFLAGS` による追加ビルドフラグの指定に対応した。（[#47](https://github.com/TappoyGK/suz/issues/47)・[#62](https://github.com/TappoyGK/suz/pull/62)）
- 値の受け渡し・実体の共有・寿命の仕様説明を整理した。（[#43](https://github.com/TappoyGK/suz/issues/43)・[#61](https://github.com/TappoyGK/suz/pull/61)）

## v0.3.2

- `rev` を追加し、整数範囲を逆順に反復できるようにした。（[#6](https://github.com/TappoyGK/suz/issues/6)・[#13](https://github.com/TappoyGK/suz/pull/13)）
- Unicodeスカラー値を表す `char` 型と文字リテラルを追加した。（[#17](https://github.com/TappoyGK/suz/issues/17)・[#18](https://github.com/TappoyGK/suz/pull/18)）
- 文字範囲と文字列を `for`・`rev` で反復できるようにした。（[#17](https://github.com/TappoyGK/suz/issues/17)・[#18](https://github.com/TappoyGK/suz/pull/18)）
- 従来の `text` 型を `symbol` に改名し、文字列テンプレートの値の型も `symbol` に変更した。（[#20](https://github.com/TappoyGK/suz/issues/20)・[#21](https://github.com/TappoyGK/suz/pull/21)）
- 文字列の範囲を参照する新しい `text` 型と、`symbol` との相互変換を追加した。（[#20](https://github.com/TappoyGK/suz/issues/20)・[#21](https://github.com/TappoyGK/suz/pull/21)）
- `symbol`・`text` に、文字列の切り出し `[開始:終了]` と文字の読み取り `[位置]` を追加した。（[#20](https://github.com/TappoyGK/suz/issues/20)・[#21](https://github.com/TappoyGK/suz/pull/21)）
- `text` の切り出し・読み取り・比較・反復・ログ出力で、文字列のコピーと動的メモリ確保を不要にした。（[#20](https://github.com/TappoyGK/suz/issues/20)・[#21](https://github.com/TappoyGK/suz/pull/21)）
- 範囲記法を値として使った場合に、誤った位置や原因を示す診断を修正した。（[#5](https://github.com/TappoyGK/suz/issues/5)・[#7](https://github.com/TappoyGK/suz/pull/7)）

## v0.3.1

- ログ出力で、本文の組み立て用バッファと動的メモリ確保を不要にした。（[#4](https://github.com/TappoyGK/suz/pull/4)）
- 定数の文字列を起動時に登録し、実行中に参照するたびに行っていた検索をなくした。（[#4](https://github.com/TappoyGK/suz/pull/4)）

## v0.3.0

- 宣言の種類に応じた命名規則を追加し、違反をコンパイルエラーにした。（[#3](https://github.com/TappoyGK/suz/pull/3)）

## v0.2.0

- 文字列を値として扱う `text` 型と、等値・順序比較を追加した。（[#2](https://github.com/TappoyGK/suz/pull/2)）
- `text` から数値・真偽値・構造体への変換を追加した。（[#2](https://github.com/TappoyGK/suz/pull/2)）
- 文字列テンプレートで `\0` を扱えるようにした。（[#2](https://github.com/TappoyGK/suz/pull/2)）
- ログ本文がテキストプールに蓄積しないようにした。（[#2](https://github.com/TappoyGK/suz/pull/2)）
- テキストプールの容量を制限する `--text-pool-limit` を追加した。（[#2](https://github.com/TappoyGK/suz/pull/2)）

## v0.1.0

- macOS・Apple Silicon向けにSuzの初版を公開した。（[#1](https://github.com/TappoyGK/suz/pull/1)）
- 数値・真偽値・構造体・列挙型、変数・定数・関数、分岐・反復・パターン照合を実装した。（[#1](https://github.com/TappoyGK/suz/pull/1)）
- 整数演算の実行時検査、ラップ演算、型変換、`or` による失敗処理を実装した。（[#1](https://github.com/TappoyGK/suz/pull/1)）
- `log info` と、ソース位置を示すパニック診断を追加した。（[#1](https://github.com/TappoyGK/suz/pull/1)）
- ソース検査、Cコードの出力、Clangによるビルド・実行に対応した。（[#1](https://github.com/TappoyGK/suz/pull/1)）
