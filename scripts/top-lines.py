#!/usr/bin/env python3
"""Gitの無視対象を除き、行数の多いファイルを上位20件表示する。"""

import os
from pathlib import Path
import subprocess


def main():
    root = Path(__file__).resolve().parent.parent
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
    )
    counts = []
    for raw_path in set(paths.split(b"\0")) - {b""}:
        name = os.fsdecode(raw_path)
        path = root / name
        if path.is_file():
            with path.open("rb") as source:
                counts.append((sum(1 for _ in source), name))

    print(f"{'LINES':>7}  FILE")
    for count, name in sorted(counts, key=lambda item: (-item[0], item[1]))[:20]:
        print(f"{count:7}  {name}")


if __name__ == "__main__":
    main()
