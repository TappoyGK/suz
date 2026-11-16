#!/usr/bin/env python3
"""Rust構文・Clippyによる実装規則の検査を行う。"""

import argparse
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True

from implementation.rust import apply_exceptions, inspect


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        violations, diagnostic_count, entries, errors = inspect(root)
        errors += apply_exceptions(violations, entries)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'Rust implementation check failed: {error}', file=sys.stderr)
        return 1
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print(f'Checked Rust implementation rules; {diagnostic_count} diagnostic sites; {len(violations)} existing violations verified.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
