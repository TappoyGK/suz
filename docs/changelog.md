# 変更履歴

利用者に関わる変更をバージョンごとに記載する。

## v0.3.8

- `arena fn`を廃止し、通常の関数・メソッド・クロージャから呼び出し時の作業領域へ確保できるようにした。mainには既定の作業領域を設けた。（[#129](https://github.com/TappoyGK/suz/issues/129)・[#134](https://github.com/TappoyGK/suz/pull/134)）
- ループの各回で不要になる実体が、外側の作業領域に蓄積する場合の警告を追加した。（[#129](https://github.com/TappoyGK/suz/issues/129)・[#134](https://github.com/TappoyGK/suz/pull/134)）
- 作業領域に確保する配列を追加し、多次元配列・部分配列の参照・切り出し・反復に対応した。（[#97](https://github.com/TappoyGK/suz/issues/97)・[#142](https://github.com/TappoyGK/suz/pull/142)）
- 配列の寸法に名前を付け、引数同士の長さの一致を検査できるようにした。添字が範囲内と分かる場合は、境界検査と読み取り時の`or`を不要にした。（[#97](https://github.com/TappoyGK/suz/issues/97)・[#142](https://github.com/TappoyGK/suz/pull/142)）
- 浮動小数点演算で不要な一時変数と、定数のビット変換関数の呼び出しを減らした。（[#142](https://github.com/TappoyGK/suz/pull/142)）
- `--log-arena`で、作業領域ごとの最大使用量をツリー表示できるようにした。全体集計とCSV出力にも対応した。（[#147](https://github.com/TappoyGK/suz/issues/147)・[#151](https://github.com/TappoyGK/suz/pull/151)）
- 浮動小数点型で正確に表せない整数リテラルを、暗黙に丸めずコンパイルエラーにした。（[#126](https://github.com/TappoyGK/suz/issues/126)・[#164](https://github.com/TappoyGK/suz/pull/164)）
- READMEに作業領域によるメモリ管理の説明を加え、仕様書に操作ごとの処理量・資源消費の上限と条件を明記した。仕様例集も拡充した。（[#146](https://github.com/TappoyGK/suz/issues/146)・[#150](https://github.com/TappoyGK/suz/pull/150)・[#152](https://github.com/TappoyGK/suz/issues/152)・[#153](https://github.com/TappoyGK/suz/pull/153)）

## v0.3.7

- 関数・構造体・`fnshape`に型引数を指定できるようにし、関数呼び出しでは実引数から型引数を推論できるようにした。（[#118](https://github.com/TappoyGK/suz/issues/118)・[#119](https://github.com/TappoyGK/suz/pull/119)）
- 型引数に必要な演算子やメソッドを`@needs`で宣言し、`needs`で要求の一覧に名前を付けられるようにした。（[#118](https://github.com/TappoyGK/suz/issues/118)・[#119](https://github.com/TappoyGK/suz/pull/119)）

## v0.3.6

- 参照先の読み出しを`.p`、更新を`#p`に変更し、フィールドへの代入では最後のメンバーを`#`で指定するようにした。（[#106](https://github.com/TappoyGK/suz/issues/106)・[#107](https://github.com/TappoyGK/suz/pull/107)）
- 構造体・列挙型に読み取りメソッドと書き換えメソッドを定義できるようにした。（[#83](https://github.com/TappoyGK/suz/issues/83)・[#109](https://github.com/TappoyGK/suz/pull/109)）
- `unsafe`を`trusted`に変更した。C構造体と型付きポインタに対応し、Arenaの実体をCへ渡せるようにした。Cが呼び出し後もポインタを保持する仮引数には`keep`を指定する。（[#99](https://github.com/TappoyGK/suz/issues/99)・[#110](https://github.com/TappoyGK/suz/pull/110)）
- ASan使用時に、Arenaの回収後・再利用前のアクセスと確保領域の余白へのアクセスを検出するようにした。（[#105](https://github.com/TappoyGK/suz/issues/105)・[#111](https://github.com/TappoyGK/suz/pull/111)）
- 構造体・列挙型に算術・ビット演算・シフト・順序比較・`not`・`len`の演算子メソッドを定義できるようにした。対応する複合代入も使えるようにした。（[#112](https://github.com/TappoyGK/suz/issues/112)・[#113](https://github.com/TappoyGK/suz/pull/113)）

## v0.3.5

- 理由を出力して終了する`panic`文を追加した。（[#80](https://github.com/TappoyGK/suz/issues/80)・[#87](https://github.com/TappoyGK/suz/pull/87)）
- エラー型と失敗する関数を追加し、呼び出し側で失敗の処理を必須にした。（[#85](https://github.com/TappoyGK/suz/issues/85)・[#89](https://github.com/TappoyGK/suz/pull/89)）
- `log debug`・`log error`と`--log-level`を追加し、指定レベル未満のログをコード生成から除くようにした。`--log-ignore`で無視した失敗を記録できるようにした。ログとパニックの埋め込みは名前とフィールド参照に限定した。（[#90](https://github.com/TappoyGK/suz/issues/90)・[#91](https://github.com/TappoyGK/suz/pull/91)）
- `extern`と`unsafe`によるC関数の呼び出しを追加した。外部ヘッダー・オブジェクト・ライブラリの指定と、文字列のバイト列の受け渡しに対応した。（[#92](https://github.com/TappoyGK/suz/issues/92)・[#93](https://github.com/TappoyGK/suz/pull/93)）
- `arena`・`new`・`arena fn`を追加し、スコープ終了時に実体をまとめて回収できるようにした。参照の寿命を検査し、`objection [理由]`で利用者が寿命を保証する箇所を指定できるようにした。作業領域の初期容量・拡張量・上限も設定できるようにした。（[#94](https://github.com/TappoyGK/suz/issues/94)・[#95](https://github.com/TappoyGK/suz/pull/95)）
- `ignore: 理由`を`ignore [理由]`に変更し、`allow`・`objection`と理由の規則を統一した。（[#96](https://github.com/TappoyGK/suz/issues/96)・[#100](https://github.com/TappoyGK/suz/pull/100)）
- `ptr T`を非null、`ptr? T`をnull許容に分け、非nullの参照は`let`なしで使えるようにした。（[#98](https://github.com/TappoyGK/suz/issues/98)・[#101](https://github.com/TappoyGK/suz/pull/101)）
- トップレベルの`var`と`--deny-global-var`を廃止した。変更可能な状態はarenaに確保し、関数の引数で渡す。（[#98](https://github.com/TappoyGK/suz/issues/98)・[#102](https://github.com/TappoyGK/suz/pull/102)）

## v0.3.4

- `const`を廃止し、トップレベルの`fix`・`var`を追加した。大域変数への警告と、大域変数を禁止する`--deny-global-var`を追加した。（[#44](https://github.com/TappoyGK/suz/issues/44)・[#67](https://github.com/TappoyGK/suz/pull/67)）
- 構造体リテラルで、フィールド名と同じ名前の値を使う場合に`: 値`を省略できるようにした。（[#45](https://github.com/TappoyGK/suz/issues/45)・[#69](https://github.com/TappoyGK/suz/pull/69)）
- 構造体の一部のフィールドを変えた値を作る`with`式を追加した。（[#46](https://github.com/TappoyGK/suz/issues/46)・[#71](https://github.com/TappoyGK/suz/pull/71)）
- 関数を値として渡せるようにし、関数の形に名前を付ける`fnshape`を追加した。汎用関数も、要求される型に具体化して渡せるようにした。（[#66](https://github.com/TappoyGK/suz/issues/66)・[#75](https://github.com/TappoyGK/suz/pull/75)・[#78](https://github.com/TappoyGK/suz/pull/78)）
- 保存できる`static fn`と、呼び出し中だけ使える`fn`を追加した。クロージャで外側の値を保持したり、外側の変数を参照して書き換えたりできるようにした。（[#77](https://github.com/TappoyGK/suz/issues/77)・[#78](https://github.com/TappoyGK/suz/pull/78)）
- 構文の入れ子を256段までとし、超過時はコンパイルエラーにした。（[#72](https://github.com/TappoyGK/suz/issues/72)・[#73](https://github.com/TappoyGK/suz/pull/73)）
- コンパイラのメモリ使用量を削減した。（[#74](https://github.com/TappoyGK/suz/issues/74)・[#76](https://github.com/TappoyGK/suz/pull/76)）

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
