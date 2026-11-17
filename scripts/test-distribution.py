#!/usr/bin/env python3
"""指定されたリリース実行ファイルを、ビルドや置き換えをせずに検証する。

追加する検査は、配布物の欠落・開発環境への依存・設置場所への依存を検出するものに限る。
言語仕様、診断、実行時の振る舞いなど、通常のテストで確認できる検査は追加しない。
新機能の追加を理由に検査を増やさない。入力プログラムは配布経路を動かす最小限にする。
検査を追加する際は、配布固有のどの不具合を検出するのかをコメントに明記する。
その理由を示せない検査は追加せず、既存の検査であれば削除する。
"""

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib


def run(args, cwd, env, code=0):
    result = subprocess.run(
        list(map(str, args)), cwd=cwd, env=env, capture_output=True, timeout=60
    )
    if result.returncode != code:
        raise AssertionError(
            f"{args}: expected exit {code}, got {result.returncode}\n"
            + result.stderr.decode(errors="replace")
        )
    return result


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def verify(binary, version, cc, directory):
    # 配布物自身の動作を調べるため、開発者が設定した追加のCコンパイルフラグを除く。
    # 検査対象を再ビルドせず、一時ディレクトリへコピーした完成済みバイナリを使う。
    env = os.environ.copy()
    for name in ("CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS", "CC"):
        env.pop(name, None)
    copied = directory / "suz"
    shutil.copy2(binary, copied)
    source = directory / "program.suz"
    # 内蔵ランタイムと環境層を使う最小の入力で、配布物だけでリンク・実行できるか確認する。
    source.write_text("""fn main() -> u8 {
    log info `distribution check`
    return 7
}
""")
    empty = directory / "empty"
    empty.mkdir()
    without_tools = dict(env, PATH=str(empty))
    # ツールを検索できない環境で、配布された版番号と内蔵のヘルプ・ライセンスを確認する。
    # ソース上の定数や文書が正しくても、配布バイナリが古い／埋め込み漏れなら検出できる。
    result = run([copied, "--version"], directory, without_tools)
    require(result.stdout == f"suz v{version}\n".encode(), "incorrect version")
    for option in ("--help", "--licenses"):
        result = run([copied, option], directory, without_tools)
        require(bool(result.stdout.strip()), f"empty {option} output")
    # checkはCコンパイラ不要の機能なので、配布物でも外部ツールを呼ばずに使えることを確認する。
    run([copied, "check", source], directory, without_tools)
    expected_log = f"info {source.name}:2: distribution check\n".encode()
    exported = directory / "exported"
    # emit-cも外部ツール不要。配布バイナリにCソース・ランタイム・ビルドスクリプトが
    # 埋め込まれ、開発用リポジトリからコピーせずに書き出せることを検査する。
    run([copied, "emit-c", source, "-o", exported], directory, without_tools)
    for path in ("program.c", "runtime/suz_env.c", "scripts/build-c.sh"):
        require((exported / path).is_file(), f"missing exported file: {path}")

    # 配布バイナリの内蔵ファイルでClangと連携できるか、build/runの両経路を確認する。
    for command in ("build", "run"):
        args = [copied, command, source, "--cc", cc]
        if command == "build":
            output = directory / "program"
            run([*args, "-o", output], directory, env)
            result = run([output], directory, env, 7)
        else:
            result = run(args, directory, env, 7)
        require(
            not result.stdout and result.stderr == expected_log,
            f"incorrect program output: {result.stderr!r}",
        )

    # 書き出されたファイルだけでビルドし、埋め込まれた配布用ファイルの不足を検出する。
    run(
        [exported / "scripts/build-c.sh", "program.c", "-o", "program"],
        exported, dict(env, CC=str(cc)),
    )
    result = run([exported / "program"], directory, env, 7)
    require(
        not result.stdout and result.stderr == expected_log,
        f"incorrect exported program output: {result.stderr!r}",
    )

    relocated = directory / "relocated"
    # 一度動かしたバイナリを移動して再実行し、元の設置場所に依存しないことを確認する。
    # 同じ場所で動かし続ける通常のテストでは、絶対パスへの依存を見逃し得る。
    relocated.mkdir()
    moved = relocated / "suz"
    copied.rename(moved)
    run([moved, "run", source, "--cc", cc], relocated, env, 7)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument(
        "--version", help="expected version (default: compiler/Cargo.toml)"
    )
    parser.add_argument("--cc", default="clang")
    args = parser.parse_args()
    # 期待する版番号の取得だけにマニフェストを使う。検査対象のバイナリは作り直さない。
    # リポジトリ外で実行する場合は--versionで期待値を渡せる。
    if args.version is None:
        manifest = Path(__file__).resolve().parent.parent / "compiler/Cargo.toml"
        with manifest.open("rb") as source:
            args.version = tomllib.load(source)["package"]["version"]
    binary = args.binary.resolve(strict=True)
    cc = shutil.which(args.cc)
    if cc is None:
        parser.error(f"C compiler not found: {args.cc}")
    try:
        with tempfile.TemporaryDirectory(prefix="suz-distribution-") as temporary:
            verify(binary, args.version, Path(cc).absolute(), Path(temporary))
    except (AssertionError, OSError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"Distribution verification failed: {error}\n")
    print(f"Distribution verification passed: {binary.name}")
    # 検証した実物を識別するための値。ハッシュの表示自体は正しさの検査ではない。
    print(f"SHA-256: {hashlib.sha256(binary.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
