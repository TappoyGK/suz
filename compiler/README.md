# Suzコンパイラ

開発にはRust 1.100.0を使う。リポジトリ直下の `rust-toolchain.toml` で、ツールチェーンとrustfmt・Clippyを指定する。言語エディションは2024とする。

言語の規則は[仕様](../docs/spec.md)、検証対象は[仕様例集](../docs/spec-cases.md)に記載する。

単一バイナリとして提供するコマンドの契約・対応環境・配布方針は[ツール仕様](../docs/tool.md)で定める。

整数演算の検査、テキストプール、環境層の実装・単体検証は[ランタイム](../runtime/README.md)を参照する。

## ビルドと実行

コンパイラのビルドにはClangと `ar` も使う。`build.rs` が同梱のRyu、浮動小数点の表示処理、textの解析処理を静的にリンクする。定数評価と生成プログラムで同じ表示・解析処理を使う。`CC` と `AR` で各ツールの実行ファイルを指定できる。

```sh
cargo run -- build sample.suz -o sample -O2
./sample
cargo run -- run sample.suz
```

追加フラグの意味と指定例は、下の[追加のCフラグ](#追加のcフラグ)を参照する。

`build` と `run` は外部のClang・リンカー・SDKを使う。`--cc PATH` でClangを、`--env FILE` で環境層を指定できる。最適化は `-O0`（既定）または `-O2` を選ぶ。

テキストプールの容量は[ツール仕様3.1節](../docs/tool.md#31-テキストプールの上限)、出力先の保護と一時ファイルは[同4節](../docs/tool.md#4-出力と一時ファイル)、`run`の実行環境と終了状態は[同2.2節](../docs/tool.md#22-run)を参照すること。

## 追加のCフラグ

`SUZ_CFLAGS` は、Suzから生成したCとランタイムをClangでビルドするときの追加設定である。Suzコンパイラ自体をビルドするRustの設定ではない。`build`・`run` と、`emit-c` が出力する `scripts/build-c.sh` で使える。

| フラグ | 意味と使いどころ |
|---|---|
| `-march=値` | 使用してよいCPUの命令セットを指定する。`native` はビルドするマシンに合わせる。他のCPUへ配布する場合は、そのCPUでも実行できる指定を選ぶ。 |
| `-mcpu=値` | CPUの種類を指定し、そのCPUの命令と性能特性に合わせてコードを生成する。対応するCPU名はClangと対象環境によって異なる。 |
| `-mtune=値` | 使用可能な命令セットを変えず、指定CPUで速くなるよう命令の選び方や並べ方を調整する。 |
| `-g` | デバッガで関数名やソース位置を調べるための情報を付ける。対象は生成したCとランタイムであり、Suzソースを直接ステップ実行する機能ではない。 |
| `-fno-omit-frame-pointer` | 関数の呼び出し履歴をたどるための情報を保持し、プロファイラで呼び出し元を調べやすくする。 |
| `-flto`、`-flto=full` | リンク時に複数のCファイルをまとめて最適化する。ファイルをまたぐ関数のインライン化などが可能になるが、ビルド時間やメモリ使用量が増えることがある。 |
| `-flto=thin` | ファイル間の情報を利用しつつ、各ファイルの最適化を分けて行うThinLTOを使う。大きなプログラムでリンク時最適化の負担を抑えたいときに使う。 |
| `-fprofile-instr-generate`、`-fprofile-instr-generate=パス` | 関数や分岐が実行された回数を記録するコードを加える。実行後の記録先は既定で `default.profraw`、または指定したパスになる。`LLVM_PROFILE_FILE` でも記録先を指定できる。 |
| `-fprofile-instr-use=パス` | 事前に集めた実行記録を使い、よく通る処理を優先して最適化する（PGO）。渡すのは、記録した `.profraw` を `llvm-profdata merge` で変換した `.profdata` ファイル。 |
| `-fsanitize=address` | 生成Cやランタイムの範囲外アクセス、解放済み領域の使用などを実行時に検出する。メモリ使用量と実行時間が増えるため、不具合の調査に使う。 |
| `-fsanitize=undefined` | 生成Cやランタイムで、Cの未定義動作が起きていないかを実行時に検査する。`-fsanitize=address,undefined` で両方を使える。 |

受け付けるフラグと書式は[ツール仕様の追加フラグ](../docs/tool.md#3-オプション)を参照すること。フラグ自体の詳細は[Clangの引数一覧](https://clang.llvm.org/docs/ClangCommandLineReference.html)、PGOの手順は[Clangの利用手引き](https://clang.llvm.org/docs/UsersManual.html#profiling-with-instrumentation)を参照すること。追加フラグによる速度の変化は、同じ入力で比較する。

たとえば、最適化したプログラムにデバッグ情報と呼び出し履歴を付ける場合は次のように指定する。

```sh
SUZ_CFLAGS="-g -fno-omit-frame-pointer" cargo run -- build sample.suz -o sample -O2
```

メモリ関連の不具合を検査しながら実行する場合：

```sh
SUZ_CFLAGS="-g -fsanitize=address,undefined" cargo run -- run sample.suz -O2
```

最適化レベルはSuzの引数として指定する。

### Suzが固定する浮動小数点の設定

浮動小数点の固定フラグは[ツール仕様](../docs/tool.md#3-オプション)、環境とpragmaの契約は[ランタイム仕様](../docs/runtime.md#2-cと実行環境の前提)で定める。各設定の目的は次のとおりである。

| 設定 | 意味 |
|---|---|
| `-fhonor-nans`、`-fhonor-infinities` | NaN・無限大が存在する前提で計算する。存在しないと仮定して比較や演算が省かれるのを防ぐ。 |
| `-ffp-contract=off` | 乗算と加算を1つの積和命令にまとめない。途中の丸めを省くことで結果が変わるのを防ぐ。 |
| `-fdenormal-fp-math=ieee` | 非常に小さい浮動小数点数（非正規化数）を、ゼロに置き換えずIEEEの規則で扱う。 |
| `float_control(precise, on)` | Cのソース側で、演算順序や符号付きゼロなどを無視する最適化を抑える。 |
| `FENV_ACCESS ON` | 丸めモードや例外フラグを読み書きすることをClangへ伝える。Suzでは環境を初期化する関数内だけに適用する。 |

`-ffp-model=strict` を全体に指定すると、演算中にも浮動小数点環境が変わりうるものとして扱われる。Suzは入口で環境を整え、その後の変更を認めないため、このフラグは使わない。これにより、各要素を独立に計算するループのベクトル化を妨げずに済む。共通ヘッダーのpragmaだけでは、NaN・無限大を無視する設定や積和への縮約を防げない。通常は、これらを指定する同梱の `scripts/build-c.sh` を使う。

## ソースの検査

リポジトリのルートから実行する。

```sh
cargo run -- check sample.suz
cargo run -- check sample.suz --warn-unhandled-arithmetic
cargo run -- --help
cargo run -- --version
cargo run -- --licenses
```

`check` は外部のCコンパイラ・SDKを使わずにソースを検査する。整数演算の診断設定には `--warn-unhandled-arithmetic` または `--deny-unhandled-arithmetic` を指定できる。

終了コードと診断形式は[ツール仕様](../docs/tool.md#5-メッセージと終了)に従う。

## C一式の出力

```sh
cargo run -- emit-c sample.suz -o generated
cd generated
./scripts/build-c.sh program.c -o sample -O2
./sample
```

`emit-c` は新規ディレクトリへ、生成C・ランタイム・Ryu・ライセンス・ビルドスクリプトを出力する。出力自体には外部ツールを使わない。`--env FILE` を指定すると、そのCソースを `runtime/suz_env.c` として収録する。出力先が既に存在する場合はエラーとなる。

## コンパイラAPI

コンパイラAPIでは`CompilationOptions.log_level`に`LogLevel`列挙型を指定する。既定値と設定を省略するAPIは`Info`を使う。

## 検証

```sh
cargo test
```

内部実装の説明は、非公開項目も含めたrustdocで読める。生成後に `target/doc/suz/index.html` を開く。CIとリリース検査でも、リンク切れなどの警告をエラーとして生成を確認する。

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items
```

コードのコメント・rustdocとMarkdown文書は日本語で書く。コマンドの出力と診断メッセージは英語とする。

リリース前は、書式・Clippy・rustdocと、最適化・サニタイザー・大量入力を含む全検査を実行する。

```sh
./scripts/test-release.sh
```

字句・構文の検査、例集の書式検査、生成したCを実行して戻り値を確認するテストを実行する。Cコンパイラは `clang` を使う。`CC` には別のCコンパイラの実行ファイルを指定できる。

例集から選んだ入力をコンパイル・実行する手順は [テスト基盤](tests/README.md) に記載している。

## ライセンス

Suzは[MIT](../LICENSE-MIT)または[Apache-2.0](../LICENSE-APACHE)のいずれかを選択して利用できる。著作権者はTAPPOY G.K.とする。Ryuには[同梱ライブラリのライセンス](../runtime/third_party/ryu/README.md)を適用する。
