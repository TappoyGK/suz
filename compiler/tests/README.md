# 仕様例集のテスト基盤

リリース全体の操作は[リリース手順](../../docs/releasing.md)に従う。

## 実行方法

リポジトリのルートで実行する。

| 用途 | コマンド | 対象 |
|---|---|---|
| 普段の確認 | `cargo test` | Suzの構文・型検査、CLI・環境層・テスト基盤の確認、例集の書式と全例の実行 |
| 配布検査 | `python3 scripts/test-distribution.py PATH --version VERSION` | 指定した配布バイナリをソースツリー外へコピーして検証 |
| リリース検査 | `./scripts/test-release.sh` | 書式・Clippy、Rustのreleaseビルドで全テスト、生成Cを `-O2` にした例集全体の実行 |

Cを繰り返しビルドする追加の型・演算・制御の組合せ検査、テキストプール上限を設定別・コマンド別に検証する実行検査、大量の変形入力、浮動小数点ログの全指数・疑似乱数値、最適化別の境界値検査は、リリース検査用の `#[ignore]` テストにする。通常の `cargo test` では理由とともに `ignored` と表示される。

リリース検査は `--include-ignored` で普段のテストも含めて実行する。`SUZ_CASES` は解除し、例集の全例を検証する。`SUZ_JOBS` と `CC` は引き継ぐ。`./scripts/test-release.sh --offline` のようにCargoのオプションも渡せる。

失敗箇所だけを調べる場合は、`cargo test --test spec_cases テスト名 -- --ignored --nocapture` でリリース検査用のテストを個別に実行できる。

```sh
cargo test
```

通常の `cargo test` は、コンパイラ・環境層の機能テストと、例集の書式検査・全例の実行検証を行う。正常終了とパニックの例では、生成したCをコンパイル・実行して終了コードと出力を確認する。CコンパイラにはC23に対応したClangを使い、`CC` で実行ファイルを変更できる。

例集の実行テストだけを起動するには、次の[スクリプト](../../scripts/test-spec-cases.sh)を使う。

```sh
./scripts/test-spec-cases.sh
```

スクリプトは例集の全例を実行し、件数と結果を表示する。`SUZ_CASES` にカンマ区切りのIDを指定すると、対象を絞れる。`SUZ_JOBS` と `CC` も環境変数で変更できる。

`cargo test --test spec_cases execute_spec_cases` を直接実行しても、対象の選び方は同じである。各例の表にある設定で、読み込み済みのソースをRust APIへ渡してSuzコンパイルを行う。CLIの引数・終了コード・診断出力は専用のテストで確認する。正常終了の例は、生成したCをビルド・実行して、終了コードと出力を照合する。未知のID、未対応の構文、未実装の診断照合は失敗にする。未対応の例を自動的に除外して成功扱いにはしない。

並列数は環境変数 `SUZ_JOBS` で指定する。省略時は8、1を指定すると逐次実行になる。値は1以上の整数とし、不正な値は実行前にエラーにする。実際のワーカー数は検証件数を上限とする。

```sh
export SUZ_JOBS=8
```

一度だけ変更する場合は、実行コマンドの前に `SUZ_JOBS=1` などを付ける。`SUZ_CASES` とは独立した設定であり、検証対象を変えずに並列数を比較できる。

設定別にSuzコンパイルと診断の照合を行い、生成したCを個別のオブジェクトファイルにコンパイルする。この2段階はそれぞれ並列に処理する。最後に実行対象を1つの実行ファイルへリンクし、対象を選んで別プロセスとして並列に起動する。Cの非公開シンボルは例ごとに分離され、実行時の状態やパニックは他の実行に影響しない。毎回コンパイル・実行し、結果は再利用しない。

入力ファイルは選択した例だけを作成し、CファイルはID・設定ごとに分ける。失敗したジョブがあっても残りを実行し、失敗内容はID順・期待結果表の設定順にまとめる。`-- --nocapture` を指定すると、実行件数・ワーカー数・成功件数・失敗件数も表示する。

各段階の時間は次のコマンドで確認できる。表示する時間は全ワーカーの累積時間であり、テスト全体の経過時間とは異なる。

```sh
SUZ_TIMINGS=1 ./scripts/test-spec-cases.sh
```

実行テストは、正常終了と、「コンパイル診断」を記載したコンパイルエラー、「パニック診断」を記載したパニックを照合する。警告は、件数・規則・位置を照合する。普段の例集全体は `execute_spec_cases`、生成Cを最適化した例集全体は `execute_optimized_spec_cases` というテストとして実行する。

## 管理するデータ

- `docs/spec-cases.md`：例集の書式、目次、全体の件数。
- `docs/spec-cases/*.md`：分野別のID、ソースまたは入力バイト列、説明、期待結果の表、期待する出力。
- `compiler/tests/support/catalog.rs`：分野別ファイルの読み込み、ファイル間のID重複、分野別・結果別・全体の件数の検査。
- `compiler/tests/support/mod.rs`：共有するデータ型とソースファイルの保存。
- `compiler/tests/support/cases.rs`：各例の入力・説明・期待値の抽出。
- `compiler/tests/support/expectations.rs`：期待結果・診断の表の読み取り、設定の検査、診断出力の照合。
- `compiler/tests/support/markdown.rs`：見出し、表、コードブロックの解析。
- `compiler/tests/support/batch.rs`：生成Cの一括リンクとプロセスごとの実行。
- `compiler/tests/support/runtime_link.rs`：生成Cを直接検証するテストで使う、標準ランタイムとのリンク設定。
- `compiler/tests/support/timing.rs`：各段階の呼び出し回数と累積時間の計測。
- `compiler/tests/spec_cases/batch.rs`：実行状態の分離とCコンパイル失敗の検証。
- `compiler/tests/support/runner.rs`：並列数の設定、ジョブの実行、コンパイル・実行結果の照合。
- `compiler/tests/spec_cases.rs`：テストの入口、実際の例集の整合検査と実行。
- `compiler/tests/spec_cases/format.rs`：Markdownの書式・抽出のテスト。
- `compiler/tests/spec_cases/diagnostics.rs`：診断・警告の期待値と照合のテスト。
- `compiler/tests/spec_cases/names.rs`：宣言ごとの命名規則、アンダースコアの位置、診断位置、大文字・小文字を区別した参照を検査する。
- `compiler/tests/spec_cases/syntax_diagnostics.rs`：構文エラーの診断、終了コード、出力ファイルを生成しないこと、途中で切れた入力のテスト。
- `compiler/tests/spec_cases/invalid_inputs.rs`：既存例の各バイトの削除と、区切り記号の置換・重複による入力の検査。コンパイラがパニックせず、エラー診断のタグが仕様に存在することを確認する。
- `compiler/tests/spec_cases/panics.rs`：パニック診断の書式・出力照合と実行検証のテスト。
- `compiler/tests/spec_cases/c_build.rs`：不適切なC設定の拒否と、ビルドコマンドによる生成プログラムの検証。
- `compiler/tests/spec_cases/execution.rs`：CLI、変数、スコープ、制御文の実行テスト。
- `compiler/tests/spec_cases/expressions.rs`：演算子の型検査と実行結果のテスト。
- `compiler/tests/spec_cases/enums.rs`：列挙型の同一性、採番の上限、整数との変換、定数・関数・match・ログとの組み合わせのテスト。
- `compiler/tests/spec_cases/generic_numeric.rs`：汎用数値関数の型推論、具体型ごとの関数生成、未使用関数、キャストと警告、診断の関連位置のテスト。
- `compiler/tests/spec_cases/generic_properties.rs`：汎用関数と具体型で書いた関数の一致、値ブロックの型推論、入れ子の定数変換、本体の誤りより前にある保留判定の診断順序のテスト。
- `compiler/tests/spec_cases/function_values.rs`：関数値の型・受け渡し・評価順序、汎用関数の具体化と診断の検査。
- `compiler/tests/spec_cases/closures.rs`：明示的な値・参照キャプチャー、環境の寿命・評価順、入れ子のスコープと深さ、保存・値の書き換え・代入先と重なる右辺の書き換えの禁止を検査する。
- `compiler/tests/spec_cases/panic.rs`：panic文の断片出力・評価順・終了判定・構文の深さと、クロージャの終了範囲を検査する。
- `compiler/tests/spec_cases/arena.rs`・`arena_runtime.c`：固定領域の整列・入れ子と再利用、容量と計算のあふれ、失敗前の状態保持、容量オプションと出力物への引き継ぎを検査する。
- `compiler/tests/spec_cases/ffi_bytes.rs`：UTF-8バイト列・NUL・切り出し、実体の同一性と寿命、評価回数、ポインタ操作の制限、バイト数の範囲検査を確認する。
- `compiler/tests/spec_cases/ffi_link.rs`：外部ヘッダー・オブジェクト・ライブラリ、リンクの順序と失敗、emit-cのパス引き継ぎを実際のCLIで検査する。
- `compiler/tests/spec_cases/ffi.rs`：Cヘッダーとの関数型照合、全スカラー型のABI、引数の評価順、C固有型の変換、unsafeの境界を検査する。
- `compiler/tests/spec_cases/functions.rs`：関数の引数・戻り値、評価順序、再帰、仮引数の制約、voidのテスト。
- `compiler/tests/spec_cases/value_blocks.rs`：値ブロックのスコープ、外側の変数の更新、評価順序、制御移動と代入先の制約のテスト。
- `compiler/tests/spec_cases/arithmetic.rs`：整数の算術、境界値、パニック、定数評価のテスト。
- `compiler/tests/spec_cases/logs.rs`：テンプレート、整数・boolの表示、評価中のパニック、ログ出力のアトミック性のテスト。
- `compiler/tests/spec_cases/symbol_float_casts.rs`：浮動小数点への変換、定数と実行時の丸め、失敗処理を検査する。解析処理の境界値とRustの数値解析との比較は `compiler/src/check/text_parse_tests.rs` に置く。
- `compiler/tests/spec_cases/symbol_struct_casts.rs`：構造体変換の入れ子、フィールド型、エスケープ、改行、定数評価と失敗処理を検査する。
- `compiler/tests/spec_cases/symbol_struct_runtime.rs`：失敗した構造体変換がプールを増やさないこと、一時領域の解放、確保失敗の位置、定数結果の静的配置を検査する。
- `compiler/tests/spec_cases/symbol_casts.rs`：整数・真偽値への変換、整数のラップ変換、定数評価、失敗処理の型と制御移動を検査する。
- `compiler/tests/spec_cases/text_views.rs`：型変換と失敗処理の制約、Unicodeの全境界での切り出し・添字、範囲の保持、確保なしの読み取りと明示的なインターン化を検査する。
- `compiler/tests/spec_cases/symbol.rs`：テンプレートの定数依存、入れ子のスコープ、診断、定数と実行時の浮動小数点表示の一致を検査する。
- `compiler/tests/spec_cases/symbol_startup.rs`：未使用の定数を含む起動時の一括登録、初期化失敗の位置、定数参照とログ出力の追加確保の有無を検査する。
- `compiler/tests/spec_cases/symbol_runtime.rs`・`symbol_runtime.c`：環境層の確保・解放の記録によるテキストプールの共有実体、容量計測、上限、確保失敗、一時バッファの検査。プール単体のCプログラムはテスト起動ごとに最適化設定別で1回ビルドし、Cargoの出力ディレクトリに置いた同じ実行ファイルを各検査から別プロセスで起動する。検査結果は再利用しない。`text_environment.c` は生成プログラムの確保要求を記録し、ログの確保要求がないこと、確保失敗時のソース位置、NULを含む出力を検査する。textのC実行テストは、通常の開発プロファイルでは動作を検査し、リリースプロファイルではサニタイザーも有効にする。最適化ありのプール検査は、プロファイルによらずサニタイザーを有効にする。
- `compiler/tests/spec_cases/matches.rs`：整数・boolのmatchとmatchexpr、定数パターン、制御移動、式の結果を捨てる場合のテスト。
- `compiler/tests/spec_cases/ranges.rs`：forの整数境界、入れ子の制御、範囲の参照・スコープ・診断のテスト。
- `compiler/tests/spec_cases/shifts.rs`：シフトの型・境界値・定数評価のテスト。
- `compiler/tests/spec_cases/arithmetic_handlers.rs`：整数演算の失敗処理、制御移動の位置、診断対象のテスト。
- `compiler/tests/spec_cases/casts.rs`：型だけで成功を保証できる変換の検査と実行結果のテスト。
- `compiler/tests/spec_cases/floats.rs`：浮動小数点のリテラル・型・比較・算術のテスト。
- `compiler/tests/spec_cases/workers.rs`：並列数とジョブ実行のテスト。
- `compiler/tests/support/test_util.rs`：一時ディレクトリと生成Cの実行に使う共通処理。
- `compiler/tests/support/integer_bounds.rs`：型検査・演算テストで共有する整数型の境界値。

分割したファイルは、同じ `spec_cases` テストターゲットのモジュールとして読み込む。`execute_spec_cases` の実行コマンドは前述のとおりである。個々のテストは関数名で絞り込める。完全名には `expressions::` などのモジュール名が付く。

ソースと期待結果は例集だけで管理する。別の期待結果ファイルはない。通常のソースは末尾にLFを付け、生バイト指定は16進数をそのまま復元する。テスト中は一時ディレクトリにID名の `.suz` ファイルを作成し、読み戻してバイト単位で比較する。終了後は削除する。

## Markdownの読み取り

書式は `docs/spec-cases.md` の「期待結果の書式」に従う。テストは `docs/spec-cases/` 直下の `.md` ファイルをすべて読み込み、例を統合する。分野別ファイルの追加時にRust側のファイル一覧を更新する必要はない。ファイル間のID重複と、目次と結果別件数の表との不一致はエラーにする。

`### ID 題名` から次の同じ階層以上の見出しまでを例の範囲とし、「ソース」のコードブロックと「期待結果」の表を対応付ける。表の列名・設定名・結果名は固定である。「標準出力」「標準エラー」「パニック前の標準エラー」のコードブロックは、末尾の改行も含めて期待値にする。コードブロック内の見出しやラベルは、入力・出力の一部として扱う。

説明用の見出しと例の見出しを区別し、例の範囲内にだけ書式の契約を適用する。主根拠の前後の空白や末尾の句点は判定に使わない。日本語の説明文は解析しない。表の値を直接読み取る。主根拠は言語仕様内に存在する規則名か確認する。診断コードとしては扱わない。

以下をエラーとして検出する。

- IDの重複、不正なID、ソースや期待結果の欠落・重複。
- 不正な16進数、閉じていないコードブロック、ラベルとコードブロックの不一致。
- 表の列名・列数の不一致、未知の設定・結果、数値の形式や範囲の誤り。
- 空の期待結果表、設定の重複、正常終了・パニックのコード省略、コンパイルエラーへの終了コード指定。
- 出力ブロックの重複、適用先のない出力ブロック。
- 入力図とソースのバイト列の不一致、目次表・結果別件数の表と実際の例の不一致。

期待結果の表の各行が検証対象を宣言する。IDの接頭辞で実行設定を選ばない。パーサーは値の形式を確認し、終了コードや警告数の値そのものは表から取得する。

実際の例集を読むテストには、特定のIDや件数、ソースのコピーを置かない。例の追加に応じて検証対象も増える。パーサー自身の単体テストには小さなサンプル文書を使い、未知の分野や設定の組み合わせも読めることを確認する。

「コンパイル診断」の表は、設定ごとに最初のエラーの規則・行・列を指定する。規則は言語仕様に存在するか確認する。行・列は1以上で、設定の重複やコンパイルエラー以外の設定への指定はエラーである。記載のないコンパイルエラーの例は抽出できるが、実行検証では失敗する。

コンパイルエラーの例では、Rust APIが `ErrorKind::InvalidSource` を返すことと、診断の規則・行・列を照合する。メッセージは空でない説明と末尾の `[rule]` を持つ1行とし、説明の文言は固定しない。コンパイル成功、未対応エラー、コンパイラ自体のパニックは期待するコンパイルエラーとして扱わない。

「警告」の表は、設定ごとに対象の演算子の規則・行・列を列挙する。実行検証では、Rust APIが返す各警告の規則・行・列を、出力順に依存せず照合する。警告の不足・余分な警告・位置や規則の違いを失敗にする。警告数が正でも表がない例は抽出だけ可能で、実行検証は失敗する。

「パニック診断」の表は、パニックになる設定ごとに最初に失敗する検査の規則・行・列を指定する。規則は `rt.` で始まり、言語仕様に存在するか確認する。同じ設定の重複や、パニック以外の設定への指定はエラーである。記載のない例は抽出できるが、実行検証では失敗する。

パニック時は終了コード101と標準出力を照合し、標準エラーが「パニック前の標準エラー」の内容と `panic FILE:LINE:COLUMN: MESSAGE [rule]` 形式の診断1行だけであることを確認する。ファイル・規則・位置を照合し、MESSAGEの文言は固定しない。ログの過不足、複数の診断、異常終了、診断を出さない通常の `return 101` は失敗になる。

## CI

GitHub Actionsの `CI` ワークフローは、`release/*` 向けの作業PRでは `work checks` として書式・Clippy・通常テストを実行し、リリースブランチへのpushでは通常テストを実行する。main向けのリリースPRとmainへのpushでは `release checks` として全検査・配布検査を実行する。手動実行でも選択したブランチの全検査を行う。起動条件とマージ手順は[リリース手順](../../docs/releasing.md)に従う。

`main` の検査が成功すると、検証済みバイナリ・SHA-256・コミットIDを含むアーカイブをActionsの成果物として30日間保存する。公開には、[リリース手順](../../docs/releasing.md)に従って対象コミットの成果物を使う。

変更ファイルの判定は `ubuntu-latest`、ビルドとテストは `macos-26`（arm64）、Xcode 26.5で実行する。Rustは `rust-toolchain.toml` に従い、例集とRustのテストはそれぞれ並列数3で実行する。GitHubのランナー更新による環境の違いを確認できるよう、OS・Rust・Clang・SDKのバージョンを実行ログに出力する。

ワークフローは [.github/workflows/ci.yml](../../.github/workflows/ci.yml) で定義する。CIで使う各コマンドは、ローカルでも実行できる。

CIは、ビルドや検証の入力にならないMarkdown文書だけの変更では、変更ファイルの判定のみを行う。対象と例外は[リリース手順](../../docs/releasing.md#ciの実行条件)に従う。判定の回帰テストは `python3 scripts/test-ci-changes.py` で実行する。

## 例の追加・変更

1. 関連する主根拠の既存例とコンパイラのテストを確認し、既存例では検出できない誤動作を特定する。既存例への統合を優先し、未実装の構文を避けるためだけの例は追加しない。
2. 例集のソース・説明・期待結果の表・出力ブロックを更新する。結果を誤って計算した場合や、制御を取り違えた場合に、終了コードや出力が変わることを確認する。
3. 例を追加・削除した場合は、例集の件数表を更新する。既存の書式に沿った追加ではRust側の変更は不要である。
4. `cargo test` を実行する。
