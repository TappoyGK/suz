# 変更履歴

利用者に関わる変更をバージョンごとに記載する。

## v0.3.3

- 要素数・位置の整数型 `len_t` と、Unicodeスカラー値の個数を返す `len` を追加し、両方を予約語にした。（[Issue #39](https://github.com/TappoyGK/suz/issues/39)・[PR #63](https://github.com/TappoyGK/suz/pull/63)）
- 文字列の切り出しで、開始位置と終了位置に使う整数の型をそろえる必要をなくした。（[Issue #39](https://github.com/TappoyGK/suz/issues/39)・[PR #63](https://github.com/TappoyGK/suz/pull/63)）
- 一つの `symbol` のUnicodeスカラー値の個数の上限を2^31−1にした。（[Issue #39](https://github.com/TappoyGK/suz/issues/39)・[PR #63](https://github.com/TappoyGK/suz/pull/63)）
- 数値型の暗黙の拡大変換、途中結果を切り詰めない整数演算、精度を失わない整数・浮動小数点数の比較を追加した。（[Issue #32](https://github.com/TappoyGK/suz/issues/32)・[PR #35](https://github.com/TappoyGK/suz/pull/35)）
- `int`・`float` による汎用数値関数を追加した。（[Issue #34](https://github.com/TappoyGK/suz/issues/34)・[PR #41](https://github.com/TappoyGK/suz/pull/41)）
- 値域で結果が決まる比較と、整数除算を浮動小数点演算に使う箇所への警告を追加した。（[Issue #32](https://github.com/TappoyGK/suz/issues/32)・[PR #35](https://github.com/TappoyGK/suz/pull/35)）
- 理由付きの警告抑制 `allow`、`--log-allow`、`log warn` を追加した。環境層のログレベルを `suz_log_level` 列挙型に変更した。（[Issue #31](https://github.com/TappoyGK/suz/issues/31)・[PR #33](https://github.com/TappoyGK/suz/pull/33)）
- 生成CをC23へ移行し、C23と128ビットの `_BitInt` に対応したClangを必須にした。（[Issue #26](https://github.com/TappoyGK/suz/issues/26)・[PR #27](https://github.com/TappoyGK/suz/pull/27)）
- 浮動小数点演算のベクトル化を可能にし、`SUZ_CFLAGS` による追加ビルドフラグの指定に対応した。（[Issue #47](https://github.com/TappoyGK/suz/issues/47)・[PR #62](https://github.com/TappoyGK/suz/pull/62)）
- 値の受け渡し・実体の共有・寿命の仕様説明を整理した。（[Issue #43](https://github.com/TappoyGK/suz/issues/43)・[PR #61](https://github.com/TappoyGK/suz/pull/61)）

## v0.3.2

- `rev` を追加し、整数範囲を逆順に反復できるようにした。（[Issue #6](https://github.com/TappoyGK/suz/issues/6)・[PR #13](https://github.com/TappoyGK/suz/pull/13)）
- Unicodeスカラー値を表す `char` 型と文字リテラルを追加した。（[Issue #17](https://github.com/TappoyGK/suz/issues/17)・[PR #18](https://github.com/TappoyGK/suz/pull/18)）
- 文字範囲と文字列を `for`・`rev` で反復できるようにした。（[Issue #17](https://github.com/TappoyGK/suz/issues/17)・[PR #18](https://github.com/TappoyGK/suz/pull/18)）
- 従来の `text` 型を `symbol` に改名し、文字列テンプレートの値の型も `symbol` に変更した。（[Issue #20](https://github.com/TappoyGK/suz/issues/20)・[PR #21](https://github.com/TappoyGK/suz/pull/21)）
- 文字列の範囲を参照する新しい `text` 型と、`symbol` との相互変換を追加した。（[Issue #20](https://github.com/TappoyGK/suz/issues/20)・[PR #21](https://github.com/TappoyGK/suz/pull/21)）
- `symbol`・`text` に、文字列の切り出し `[開始:終了]` と文字の読み取り `[位置]` を追加した。（[Issue #20](https://github.com/TappoyGK/suz/issues/20)・[PR #21](https://github.com/TappoyGK/suz/pull/21)）
- `text` の切り出し・読み取り・比較・反復・ログ出力で、文字列のコピーと動的メモリ確保を不要にした。（[Issue #20](https://github.com/TappoyGK/suz/issues/20)・[PR #21](https://github.com/TappoyGK/suz/pull/21)）
- 範囲記法を値として使った場合に、誤った位置や原因を示す診断を修正した。（[Issue #5](https://github.com/TappoyGK/suz/issues/5)・[PR #7](https://github.com/TappoyGK/suz/pull/7)）

## v0.3.1

- ログを断片ごとに出力し、ログ本文の整形と出力のための動的メモリ確保をなくした。
- すべてのconstant textを起動時に登録する方式とした。実行中の定数参照では登録済みの実体を直接使い、ハッシュ計算と検索をなくした。初期化の確保失敗とプール上限超過はmainの開始前に診断するようにした。

## v0.3.0

- 宣言の種類ごとに命名規則を定め、違反をコンパイルエラーとした。定数は大文字、関数・変数・仮引数・フィールドは小文字を使い、構造体・列挙型・列挙子は大文字で始める規則とした。
- 名前の先頭・末尾・連続する `_` を禁止した。型名には小文字を必須とし、大文字の連続を禁止した。

## v0.2.0

- `text` 型を追加した。テンプレートを値として生成し、変数・定数・関数・構造体で扱えるようにした。
- 同じ内容のtextはテキストプールで実体を共有し、プログラムの終了まで保持する方式とした。等値比較とUnicodeの辞書順での順序比較を追加した。
- textから整数・浮動小数点数・真偽値・構造体への `as` 変換と、整数への `as%` 変換を追加した。変換失敗時の処理は `or` での指定を必須とした。
- テンプレートで `\0` を扱えるようにした。ログ本文は一時領域で生成し、テキストプールへ蓄積しない方式とした。
- `build`・`run`・`emit-c` に `--text-pool-limit` を追加した。既定の上限を1Gとし、バイト数・K・M・G・`unlimited` を指定できるようにした。

## v0.1.0

Suzの初版を公開した。

- 整数・浮動小数点数・真偽値・構造体・列挙型と、定数・変数・関数を提供した。
- 分岐・反復・パターン照合と、値を返す `ifexpr`・`matchexpr`・`expr` を提供した。
- 整数演算の実行時検査、ラップ演算、型変換、`or` による失敗処理を提供した。
- テンプレート文字列を使う `log info` と、ソース位置を示すパニック診断を提供した。
- `suz` にソース検査、実行ファイルのビルドと実行、C一式の出力を備えた。ランタイムとRyuを内包し、ビルドには外部のClang・リンカー・SDKを使う構成とした。
- macOS・Apple Siliconを対象とした。
