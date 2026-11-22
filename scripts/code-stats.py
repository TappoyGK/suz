#!/usr/bin/env python3
"""ソースの配置規約に従い、現在の作業ツリーの行数を分類して表示する。"""

from pathlib import Path
import subprocess


def main():
    root = Path(__file__).resolve().parent.parent
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "-z"], cwd=root
    ).decode().split("\0")
    counts = dict.fromkeys([
        "Compiler implementation", "Compiler unit tests", "Integration and spec tests",
        "Runtime", "Third-party code", "Documentation", "Spec examples",
    ], 0)
    for name in sorted(set(paths) - {""}):
        path = Path(name)
        source = root / path
        if not source.is_file():
            continue
        if name.startswith("compiler/src/") and path.suffix == ".rs":
            lines = source.read_text().splitlines()
            if path.name.endswith("tests.rs") or "tests" in path.parts:
                split = 0
            else:
                # トップレベルのテスト宣言から末尾まではテスト専用にする。
                # 本体内の計測フックは分離せず、実装本体に含める。
                split = next((i for i, line in enumerate(lines)
                              if line == "#[cfg(test)]"), len(lines))
            counts["Compiler implementation"] += split
            counts["Compiler unit tests"] += len(lines) - split
            continue
        if name.startswith("compiler/tests/") and path.suffix in {".rs", ".c"}:
            group = "Integration and spec tests"
        elif name.startswith("runtime/") and path.suffix in {".c", ".h"}:
            group = "Third-party code" if name.startswith("runtime/third_party/") else "Runtime"
        elif path.parent == Path("docs") and path.suffix == ".md":
            group = "Documentation"
        elif path.parent == Path("docs/spec-cases") and path.suffix == ".md":
            group = "Spec examples"
        else:
            continue
        counts[group] += len(source.read_text().splitlines())

    print("| Category | Lines |")
    print("|---|---:|")
    for group, count in counts.items():
        print(f"| {group} | {count:,} |")


if __name__ == "__main__":
    main()
