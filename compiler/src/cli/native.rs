//! Clangと実行環境を確認し、生成Cのビルドと実行ファイルの公開を行う。
//!
//! 利用元: [`crate::cli`]。
//! 主な対象規則: `[tool.build-settings]`、`[tool.extra-flags]`、`[tool.c-compiler]`、`[tool.environment]`、`[tool.messages]`、`[tool.output]`、`[tool.prerequisites]`。

use super::{
    Failure, Result,
    arguments::Options,
    emit,
    output::{StagedDirectory, rename_new},
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub struct Build {
    directory: StagedDirectory,
    cc: PathBuf,
    extra_flags: Vec<String>,
}

fn error(rule: &str, error: impl std::fmt::Display) -> Failure {
    Failure::tool(2, rule, error)
}

fn prerequisite(message: impl std::fmt::Display) -> Failure {
    let mut failure = error("tool.prerequisites", message);
    failure
        .message
        .push_str("\nInstall Xcode Command Line Tools: xcode-select --install");
    failure
}

fn compiler_output(output: &Output) -> Result<()> {
    let mut stderr = io::stderr().lock();
    stderr
        .write_all(&output.stdout)
        .and_then(|_| stderr.write_all(&output.stderr))
        .map_err(|e| error("tool.messages", e))
}

impl Build {
    pub fn prepare(options: &Options) -> Result<Self> {
        for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
            if std::env::var_os(name).is_some_and(|v| !v.is_empty()) {
                return Err(error(
                    "tool.build-settings",
                    format!("{name} must be empty; use SUZ_CFLAGS and -O0 or -O2"),
                ));
            }
        }
        let extra_flags = extra_flags()?;
        if let Some(output) = &options.output {
            validate_output(options, output)?;
        }
        let parent = if let Some(output) = &options.output {
            output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_owned()
        } else {
            std::env::temp_dir()
        };
        let parent = fs::canonicalize(parent).map_err(|e| error("tool.temporary", e))?;
        let directory = StagedDirectory::new(&parent).map_err(|e| error("tool.temporary", e))?;
        let build = Self {
            directory,
            extra_flags,
            cc: options.cc.clone().unwrap_or_else(|| "clang".into()),
        };
        build.probe()?;
        Ok(build)
    }

    fn command(&self, extra_flags: &[String]) -> Command {
        let mut command = Command::new(&self.cc);
        command.env("TMPDIR", self.directory.path());
        command.args(extra_flags);
        command.args([
            "-std=c23",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fhonor-nans",
            "-fhonor-infinities",
            "-ffp-contract=off",
            "-fdenormal-fp-math=ieee",
        ]);
        command
    }

    // コンパイラとSDKの存在確認には利用者のフラグを混ぜない。
    // 追加フラグの値の拒否は本ビルドでClangの診断とともに報告する。
    fn probe(&self) -> Result<()> {
        let root = self.directory.path();
        let source = root.join("probe.c");
        let object = root.join("probe.o");
        fs::write(
            &source,
            r#"#if !defined(__clang__)
#error "Clang required [tool.c-compiler]"
#endif
#if !defined(__STDC_VERSION__) || __STDC_VERSION__ < 202311L
#error "C23 required [tool.c-compiler]"
#endif
_BitInt(128) suz_compiler_probe;
"#,
        )
        .map_err(|e| error("tool.temporary", e))?;
        let result = self
            .command(&[])
            .args(["-Wno-unused-command-line-argument", "-fsyntax-only"])
            .arg(&source)
            .output()
            .map_err(|e| {
                prerequisite(format!("cannot start Clang '{}': {e}", self.cc.display()))
            })?;
        if !result.status.success() {
            compiler_output(&result)?;
            return Err(error(
                "tool.c-compiler",
                "C23-capable Clang with 128-bit _BitInt support is required",
            ));
        }
        fs::write(&source, "#include <stdio.h>\n#include <stdlib.h>\n#include <fenv.h>\nint main(void) { return fesetenv(FE_DFL_ENV); }\n")
            .map_err(|e| error("tool.temporary", e))?;
        for link in [false, true] {
            let mut command = self.command(&[]);
            if link {
                command
                    .arg(&object)
                    .args(["-lm", "-o"])
                    .arg(root.join("probe"));
            } else {
                command
                    .args(["-Wno-unused-command-line-argument", "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(&object);
            }
            let result = command.output().map_err(|e| {
                prerequisite(format!("cannot start Clang '{}': {e}", self.cc.display()))
            })?;
            if !result.status.success() {
                compiler_output(&result)?;
                let requirement = if link {
                    "linker or SDK libraries"
                } else {
                    "C compiler or SDK headers"
                };
                return Err(prerequisite(format!(
                    "{requirement} are unavailable or unusable"
                )));
            }
        }
        Ok(())
    }

    pub fn compile(&self, options: &Options, c: &str) -> Result<PathBuf> {
        let environment = options
            .environment
            .as_ref()
            .map(|path| {
                fs::read(path)
                    .map_err(|e| error("tool.environment", format!("{}: {e}", path.display())))
            })
            .transpose()?;
        let root = self.directory.path();
        emit::unpack(c, root, environment.as_deref()).map_err(|e| error("tool.temporary", e))?;
        let executable = root.join("program");
        let mut command = self.command(&self.extra_flags);
        // 内部ヘッダーは引用符のincludeだけに公開し、externの<...>からは探索させない。
        command
            .arg(&options.optimization)
            .arg("-iquote")
            .arg(root.join("runtime"))
            .arg("-iquote")
            .arg(root.join("runtime/third_party/ryu"));
        for path in [
            "program.c",
            "runtime/suz_runtime.c",
            "runtime/suz_arena.c",
            "runtime/suz_text.c",
            "runtime/suz_parse.c",
            "runtime/suz_parse_float.c",
            "runtime/suz_parse_struct.c",
            "runtime/suz_env.c",
            "runtime/suz_float.c",
            "runtime/suz_format_float.c",
            "runtime/third_party/ryu/ryu/f2s.c",
            "runtime/third_party/ryu/ryu/d2s.c",
        ] {
            command.arg(root.join(path));
        }
        if c.contains("#define SUZ_LOG_ARENA 1\n") {
            command.arg(root.join("runtime/suz_arena_report.c"));
        }
        let result = command
            .args(&options.foreign.args)
            .args(["-lm", "-o"])
            .arg(&executable)
            .output()
            .map_err(|e| error("tool.c-compiler", e))?;
        compiler_output(&result)?;
        if !result.status.success() {
            return Err(Failure::tool(
                4,
                "tool.c-compiler",
                "Clang compilation or linking failed",
            ));
        }
        Ok(executable)
    }

    pub fn publish(&self, options: &Options, executable: &Path, destination: &Path) -> Result<()> {
        validate_output(options, destination)?;
        let result = if options.force {
            fs::rename(executable, destination)
        } else {
            rename_new(executable, destination)
        };
        result.map_err(|e| {
            error(
                "tool.publish-output",
                format!("{}: {e}", destination.display()),
            )
        })
    }
}

fn validate_output(options: &Options, output: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(output) {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(error("tool.output", e)),
    };
    if !options.force || !metadata.is_file() {
        return Err(error(
            "tool.output",
            format!(
                "output already exists or is not a replaceable regular file: {}",
                output.display()
            ),
        ));
    }
    for input in std::iter::once(&options.input).chain(options.environment.iter()) {
        let input_metadata = fs::metadata(input).map_err(|e| error("tool.output", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() == input_metadata.dev() && metadata.ino() == input_metadata.ino() {
                return Err(error("tool.output", "output aliases an input file"));
            }
        }
    }
    Ok(())
}

// 出力用スクリプトと同じ許可リストを使う。シェルの引用・展開は行わず、
// 空白で分けた各項目をそのままClangの引数として渡す。
fn extra_flags() -> Result<Vec<String>> {
    let value = match std::env::var("SUZ_CFLAGS") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(Vec::new()),
        Err(_) => return Err(error("tool.extra-flags", "SUZ_CFLAGS must be valid UTF-8")),
    };
    value
        .split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
        .filter(|flag| !flag.is_empty())
        .map(|flag| {
            let allowed = include_str!("../../../scripts/cflags.txt")
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .any(|entry| {
                    if entry.ends_with('=') {
                        flag.strip_prefix(entry).is_some_and(|value| {
                            !value.is_empty()
                                && (entry != "-fsanitize="
                                    || value
                                        .split(',')
                                        .all(|v| matches!(v, "address" | "undefined")))
                        })
                    } else {
                        flag == entry
                    }
                });
            if allowed {
                Ok(flag.to_owned())
            } else {
                Err(error(
                    "tool.extra-flags",
                    format!("unsupported SUZ_CFLAGS flag: {flag}"),
                ))
            }
        })
        .collect()
}
