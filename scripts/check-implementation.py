#!/usr/bin/env python3
"""実装規則の自動検査を起動する。"""

from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True

from implementation.check import check_repository


def main():
    root = Path(__file__).resolve().parent.parent
    try:
        counts, exceptions, errors = check_repository(root)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'Implementation check failed: {error}', file=sys.stderr)
        return 1
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print(f'Checked code line limits for {len(counts)} files; {len(exceptions)} exceptions verified.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
