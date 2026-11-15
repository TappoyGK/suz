# Suzランタイム

生成したCとリンクするランタイム本体と、メモリ確保・出力・プロセス終了を担う標準環境層を置く。動作の契約は[ランタイム仕様](../docs/runtime.md)に従う。

| ファイル | 責務 |
|---|---|
| `suz_config.h` | Cの型・評価精度・浮動小数点設定の検査 |
| `suz_runtime.h`・`suz_runtime.c` | 起動時の浮動小数点環境の初期化、整数演算の検査、ログとパニック診断の組み立て |
| `suz_format_float.c` | コンパイラと生成プログラムで共有する浮動小数点の表示形式と数値による境界判定 |
| `suz_float.c` | 整形した浮動小数点のバイト列をログへ出力する関数 |
| `suz_parse.h`・`suz_parse.c` | コンパイラと生成プログラムで共有する整数・真偽値・文字の解析 |
| `suz_char.h` | Unicodeスカラー値の検査とUTF-8の符号化・復号 |
| `suz_parse_struct.h`・`suz_parse_struct.c` | 型情報に従う構造体の検査と、各フィールドの値・文字列範囲の取得 |
| `suz_parse_float.c` | 10進の入力から浮動小数点への直接の最近接・偶数丸め |
| `suz_text.h`・`suz_text.c` | `symbol` の共有実体と `text` の範囲参照・切り出し、テキストプール、インターン化、容量計測、一時バッファ |
| `third_party/ryu/` | 最短の十進表記へ変換するRyuのC実装とライセンス |
| `suz_env.h` | メモリ確保・解放、出力、パニック終了の環境関数の宣言 |
| `suz_env.c` | `malloc`・`free` とC標準入出力を使う標準環境層 |

生成したCの入口は、Suzのmainを呼ぶ前に `suz_runtime_init()` を呼ぶ。`fesetenv(FE_DFL_ENV)` で丸めモード・例外マスク・例外フラグを初期化する。設定に失敗した場合は `abort()` で停止する。

整数検査関数の呼び出し条件と戻り値は [suz_runtime.h](suz_runtime.h) に記載する。

正確な整数算術は、符号と `unsigned _BitInt(128)` の絶対値を分けて保持する。コンパイラの範囲検査で中間値が収まることを保証し、生成コードがゼロ除算と格納先の範囲を検査する。整数と浮動小数点数の比較では、浮動小数点数の仮数と指数を取り出して比較し、整数を丸めない。これらの計算と正確な整数のログ出力は動的メモリを確保しない。

## テキストプール

`suz_text` は、終了まで移動・解放しない実体へのポインタである。実体には本文へのポインタ、バイト長、検索用のハッシュ値を保持する。constant textは静的な本文を参照し、dynamic textは実体の管理情報と本文をまとめて確保する。本文に終端バイトは付けず、NULも長さに含めて扱う。

検索表はオープンアドレス法を使い、32ビットのFNV-1aで探索を始める。ハッシュが一致した場合も長さと全バイトを照合する。登録済みの値を検索してから追加確保を判断し、先に登録された実体を再利用する。検索表の拡張でも実体のアドレスは変わらない。

`suz_text_pool_init(limit)` は起動時に1回呼ぶ。続けて、生成したCがすべてのconstant textを登録し、実体へのポインタを静的変数に保持する。通常の参照はこの変数を読み、ハッシュ計算や検索を行わない。CのAPIでは上限0を無制限として扱う。textを使う生成プログラムでは、Suzのmainを呼ぶ前に初期化する。上限は `--text-pool-limit` で指定した値を生成Cに埋め込み、省略時は1,073,741,824バイトとする。使用量は確保要求のサイズで数え、検索表の拡張中は旧領域と新領域の両方を数える。旧領域の解放後、その要求サイズを差し引く。

`suz_text_buffer` は一時的な組み立て用のバッファであり、プールの使用量に含めない。テンプレートの埋め込み値をすべて評価した後に本文を組み立てる。値として使うテンプレートはインターン化後に `suz_text_buffer_dispose()` で解放する。呼び出し条件は [suz_text.h](suz_text.h) に記載する。

環境関数 `suz_env_log` は、各出力断片とともに列挙型 `suz_log_level` のレベルを受け取る。標準環境層はレベルを無視して標準エラーへ出力する。

ログは埋め込み値を左から右へ評価して保持した後、`suz_log_begin()` から `suz_log_end()` までロックして断片を出力する。数値の整形には固定長の自動変数を使い、textは保持済みの本文を渡す。ログ本文の整形と出力では動的メモリを確保しない。

## textからの数値変換

整数・真偽値・浮動小数点の解析処理を、コンパイラの定数評価と生成プログラムで共有する。入力の長さ全体を検査し、ロケールや終端NULに依存しない。

浮動小数点の解析では、10進の入力を整数の比として扱い、商と剰余から変換先の精度へ直接丸める。先頭800桁と、それ以降に非ゼロの桁があるかを保持する。binary64の丸めの境界は770桁未満の有限小数で表せるため、この情報で境界の前後と一致を区別できる。解析用の固定領域を使い、入力の桁数に応じたメモリ確保は行わない。

構造体変換では、生成した型情報に従い、全フィールドと入力末尾を検査する。検査中は文字列の範囲だけを保持し、プールへの登録も一時バッファの確保も行わない。全体の検査後にエスケープを展開し、textフィールドをインターン化する。変換後の一時バッファは解放する。確保失敗とプール上限超過の位置には、変換の `as` を使う。

定数式の構造体変換にも同じ解析処理を使う。結果のtextフィールドは静的に配置し、他のconstant textとともに起動時に登録する。

## ビルド

リポジトリのルートで、生成した `program.c` とランタイム本体・標準環境層をリンクする。

```sh
./scripts/build-c.sh program.c -o program
./scripts/build-c.sh program.c -o program-optimized -O2
```

ビルドコマンドはClangを使う。`CC` で、C23に対応したClangの実行ファイルを指定できる。C23と厳密な浮動小数点演算、積和の縮約停止、非正規化数のIEEE処理を指定する。最適化は `-O0`（既定）または `-O2` を選べる。出力先は新規ファイルを指定する。

生成Cとランタイムは `suz_config.h` を通じて、型の表現・非正規化数・式の評価精度を検査する。検出可能な危険な浮動小数点設定はコンパイルエラーにする。Cのプリプロセッサから判別できない設定もあるため、配布物のビルドには上記コマンドを使う。

## 環境層の差し替え

[suz_env.h](suz_env.h) の4関数を、[環境関数の契約](../docs/runtime.md#4-環境関数)に従って実装する。ビルド時に `--env` でその実装を指定する。

```sh
./scripts/build-c.sh program.c -o program --env custom_env.c
```

## 検証

リポジトリのルートで、ランタイムと標準環境層のテストを実行する。

```sh
cargo test --test spec_cases runtime::
cargo test --test spec_cases logs::
cargo test --test spec_cases float_logs::
cargo test --test spec_cases symbol_runtime::
```

最適化別の全型の境界値検査と浮動小数点ログの大量検査は、リリース検査で実行する。

```sh
./scripts/test-release.sh
```

環境層・整数検査・パニックのテストは [runtime.rs](../compiler/tests/spec_cases/runtime.rs)、ログのテストは [logs.rs](../compiler/tests/spec_cases/logs.rs) に置く。

テキストプールは [symbol_runtime.rs](../compiler/tests/spec_cases/symbol_runtime.rs) から検証用のCプログラムを実行して確認する。環境層で確保要求と解放を記録し、共有実体、容量計測、上限超過、確保失敗、診断中の追加確保の有無を検査する。普段は最適化なしで動作を検査する。リリース検査ではAddressSanitizer・UndefinedBehaviorSanitizerを有効にし、最適化なし・ありの両方を検査する。

起動時の一括登録、初期化後の定数参照、ログの追加確保の有無は [symbol_startup.rs](../compiler/tests/spec_cases/symbol_startup.rs) で検証する。

構造体変換の失敗時のプール使用量、一時領域の解放、確保失敗の診断位置、定数結果の静的配置は [symbol_struct_runtime.rs](../compiler/tests/spec_cases/symbol_struct_runtime.rs) で検証する。

浮動小数点ログは [float_logs.rs](../compiler/tests/spec_cases/float_logs.rs) で検証する。両精度の境界値・特殊値、各指数の境界と疑似乱数値の読み戻し、最短の桁数、ロケールからの独立性を確認する。C11の64ビット整数演算だけを使うRyuの経路も、AddressSanitizer・UndefinedBehaviorSanitizerを有効にして実行する。

同梱するRyuの取得元と使用バージョンは次のとおり。

- リポジトリ：[ulfjack/ryu](https://github.com/ulfjack/ryu)
- 使用バージョン（固定コミット）：[`4c0618b0e44f7ef027ebae05d2cc7812048f7c8f`](https://github.com/ulfjack/ryu/tree/4c0618b0e44f7ef027ebae05d2cc7812048f7c8f)

同梱範囲とライセンスは [third_party/ryu/README.md](third_party/ryu/README.md) に記載する。
