# Suzランタイム

生成したCとリンクするランタイム本体と、出力・プロセス終了を担う標準環境層を置く。動作の契約は[ランタイム仕様](../docs/runtime.md)に従う。

| ファイル | 責務 |
|---|---|
| `suz_config.h` | Cの型・評価精度・浮動小数点設定の検査 |
| `suz_runtime.h`・`suz_runtime.c` | 起動時の浮動小数点環境の初期化、整数演算の検査、ログとパニック診断の組み立て |
| `suz_float.c` | 浮動小数点ログの表示形式と数値による境界判定 |
| `third_party/ryu/` | 最短の十進表記へ変換するRyuのC実装とライセンス |
| `suz_env.h` | 出力とパニック終了の環境関数の宣言 |
| `suz_env.c` | C標準入出力を使う標準環境層 |

生成したCの入口は、Suzのmainを呼ぶ前に `suz_runtime_init()` を呼ぶ。`fesetenv(FE_DFL_ENV)` で丸めモード・例外マスク・例外フラグを初期化する。設定に失敗した場合は `abort()` で停止する。

整数検査関数の呼び出し条件と戻り値は [suz_runtime.h](suz_runtime.h) に記載する。

## ビルド

リポジトリのルートで、生成した `program.c` とランタイム本体・標準環境層をリンクする。

```sh
./scripts/build-c.sh program.c -o program
./scripts/build-c.sh program.c -o program-optimized -O2
```

ビルドコマンドはClangを使う。`CC` で、同じオプションを扱えるコンパイラの実行ファイルを指定できる。C11と厳密な浮動小数点演算、積和の縮約停止、非正規化数のIEEE処理を指定する。最適化は `-O0`（既定）または `-O2` を選べる。出力先は新規ファイルを指定する。

生成Cとランタイムは `suz_config.h` を通じて、型の表現・非正規化数・式の評価精度を検査する。検出可能な危険な浮動小数点設定はコンパイルエラーにする。Cのプリプロセッサから判別できない設定もあるため、配布物のビルドには上記コマンドを使う。

## 環境層の差し替え

[suz_env.h](suz_env.h) の2関数を、[環境関数の契約](../docs/runtime.md#4-環境関数)に従って実装する。ビルド時に `--env` でその実装を指定する。

```sh
./scripts/build-c.sh program.c -o program --env custom_env.c
```

## 検証

リポジトリのルートで、ランタイムと標準環境層のテストを実行する。

```sh
cargo test --test spec_cases runtime::
cargo test --test spec_cases logs::
cargo test --test spec_cases float_logs::
```

最適化別の全型の境界値検査と浮動小数点ログの大量検査は、リリース検査で実行する。

```sh
./scripts/test-release.sh
```

環境層・整数検査・パニックのテストは [runtime.rs](../compiler/tests/spec_cases/runtime.rs)、ログのテストは [logs.rs](../compiler/tests/spec_cases/logs.rs) に置く。

浮動小数点ログは [float_logs.rs](../compiler/tests/spec_cases/float_logs.rs) で検証する。両精度の境界値・特殊値、各指数の境界と疑似乱数値の読み戻し、最短の桁数、ロケールからの独立性を確認する。C11の64ビット整数演算だけを使うRyuの経路も、AddressSanitizer・UndefinedBehaviorSanitizerを有効にして実行する。

同梱するRyuの取得元と使用バージョンは次のとおり。

- リポジトリ：[ulfjack/ryu](https://github.com/ulfjack/ryu)
- 使用バージョン（固定コミット）：[`4c0618b0e44f7ef027ebae05d2cc7812048f7c8f`](https://github.com/ulfjack/ryu/tree/4c0618b0e44f7ef027ebae05d2cc7812048f7c8f)

同梱範囲とライセンスは [third_party/ryu/README.md](third_party/ryu/README.md) に記載する。
