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
                    format!("{name} must be empty; use -O0 or -O2"),
                ));
            }
        }
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
            cc: options.cc.clone().unwrap_or_else(|| "clang".into()),
        };
        build.probe()?;
        Ok(build)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.cc);
        command.env("TMPDIR", self.directory.path());
        command.args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-ffp-model=strict",
            "-ffp-contract=off",
            "-fdenormal-fp-math=ieee",
        ]);
        command
    }

    fn probe(&self) -> Result<()> {
        let root = self.directory.path();
        let source = root.join("probe.c");
        let object = root.join("probe.o");
        fs::write(&source, "#include <stdio.h>\n#include <stdlib.h>\n#include <fenv.h>\nint main(void) { return fesetenv(FE_DFL_ENV); }\n")
            .map_err(|e| error("tool.temporary", e))?;
        for link in [false, true] {
            let mut command = self.command();
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
        let mut command = self.command();
        command
            .arg(&options.optimization)
            .arg("-I")
            .arg(root.join("runtime"))
            .arg("-I")
            .arg(root.join("runtime/third_party/ryu"));
        for path in [
            "program.c",
            "runtime/suz_runtime.c",
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
        let result = command
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
