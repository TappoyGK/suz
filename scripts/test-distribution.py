#!/usr/bin/env python3
"""Verify the supplied release executable, without building or replacing it."""

import argparse
import hashlib
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile


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
    env = os.environ.copy()
    for name in ("CFLAGS", "CPPFLAGS", "LDFLAGS", "CC"):
        env.pop(name, None)
    copied = directory / "suz"
    shutil.copy2(binary, copied)
    source = directory / "program.suz"
    source.write_text("""fn main() -> u8 {
    fix zero: f64 = 0.0
    fix invalid = zero / zero
    if invalid == invalid || 1.0 / -zero != -inf { return 1 }
    fix tiny: f32 = 1.40129846e-45
    if tiny + tiny != 2.80259693e-45 { return 2 }
    log info `${invalid} ${-zero}`
    return 7
}
""")
    panic = directory / "panic.suz"
    panic.write_text("""fn main() -> u8 {
    var value: u8 = 255
    value += 1
    return value
}
""")
    empty = directory / "empty"
    empty.mkdir()
    without_tools = dict(env, PATH=str(empty))
    result = run([copied, "--version"], directory, without_tools)
    require(result.stdout == f"suz v{version}\n".encode(), "incorrect version")
    for option in ("--help", "--licenses"):
        result = run([copied, option], directory, without_tools)
        require(bool(result.stdout.strip()), f"empty {option} output")
    run([copied, "check", source], directory, without_tools)
    exported = directory / "exported"
    run([copied, "emit-c", source, "-o", exported], directory, without_tools)
    for path in ("program.c", "runtime/suz_env.c", "scripts/build-c.sh"):
        require((exported / path).is_file(), f"missing exported file: {path}")

    for optimization in ("-O0", "-O2"):
        for command in ("build", "run"):
            for input_file, expected in ((source, 7), (panic, 101)):
                args = [copied, command, input_file, optimization, "--cc", cc]
                if command == "build":
                    output = directory / f"{input_file.stem}{optimization}"
                    run([*args, "-o", output], directory, env)
                    result = run([output], directory, env, expected)
                else:
                    result = run(args, directory, env, expected)
                require(not result.stdout, "unexpected program stdout")
                if expected == 7:
                    require(
                        result.stderr == f"info {source.name}:7: nan -0.0\n".encode(),
                        f"incorrect log: {result.stderr!r}",
                    )
                else:
                    require(
                        re.fullmatch(rb"panic [^\n]+ \[rt\.[^\]\n]+\]\n", result.stderr),
                        f"incorrect panic diagnostic: {result.stderr!r}",
                    )
        run(
            [exported / "scripts/build-c.sh", "program.c", "-o", f"program{optimization}", optimization],
            exported, dict(env, CC=str(cc)),
        )
        run([exported / f"program{optimization}"], directory, env, 7)

    # A custom compiler path exercises actual missing SDK/linker failures without
    # modifying the developer's Xcode installation or system search paths.
    unavailable = [("compiler", None, without_tools)]
    for name, flags in (
        ("sdk", ["-isysroot", str(empty)]),
        ("linker", ["-fuse-ld=" + str(empty / "ld")]),
    ):
        wrapper = directory / f"clang-missing-{name}"
        wrapper.write_text(
            "#!/bin/sh\nexec " + shlex.join([str(cc), *flags]) + ' "$@"\n'
        )
        wrapper.chmod(0o755)
        unavailable.append((name, wrapper, env))
    diagnostics = {}
    for name, compiler, environment in unavailable:
        for command in ("build", "run"):
            args = [copied, command, source]
            output = directory / f"missing-{name}"
            if command == "build":
                args.extend(["-o", output])
            if compiler:
                args.extend(["--cc", compiler])
            result = run(args, directory, environment, 2)
            require(b"[tool.prerequisites]" in result.stderr, "missing prerequisite rule")
            require(b"xcode-select --install" in result.stderr, "missing installation guidance")
            require(not output.exists(), "published output after prerequisite failure")
            diagnostics[name] = result.stderr
    require(diagnostics["compiler"] != diagnostics["sdk"], "indistinguishable prerequisite errors")

    relocated = directory / "relocated"
    relocated.mkdir()
    moved = relocated / "suz"
    copied.rename(moved)
    run([moved, "run", source, "--cc", cc], relocated, env, 7)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--cc", default="clang")
    args = parser.parse_args()
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
    print(f"SHA-256: {hashlib.sha256(binary.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
