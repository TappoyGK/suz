use super::{
    runner::parallel_map,
    timing::{C_TIME, RUN_TIME, timed_output},
};
use std::{
    collections::BTreeMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

type Result<T> = std::result::Result<T, String>;

pub struct Batch {
    executable: Option<PathBuf>,
    errors: BTreeMap<usize, String>,
}

fn compile_c(command: &mut Command) -> Result<()> {
    let output = timed_output(command, &C_TIME)
        .map_err(|error| format!("could not start the C compiler; check clang or CC: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "C compilation/linking failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

impl Batch {
    pub fn build(directory: &Path, inputs: &[(usize, PathBuf)], workers: usize) -> Result<Self> {
        Self::build_with_flags(directory, inputs, workers, &[])
    }

    pub fn build_optimized(
        directory: &Path,
        inputs: &[(usize, PathBuf)],
        workers: usize,
    ) -> Result<Self> {
        Self::build_with_flags(directory, inputs, workers, &["-O2"])
    }

    fn build_with_flags(
        directory: &Path,
        inputs: &[(usize, PathBuf)],
        workers: usize,
        extra_flags: &[&str],
    ) -> Result<Self> {
        let compiler = std::env::var_os("CC").unwrap_or_else(|| "clang".into());
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
        let flags = ["-std=c23", "-Wall", "-Wextra", "-Werror"];
        // Keep translation units separate: private C helpers and static state
        // from different Suz programs must not share a namespace.
        let compiled = parallel_map(inputs, workers, |(index, source)| {
            let text = fs::read_to_string(source)
                .map_err(|error| format!("could not read generated C: {error}"))?;
            let needs_runtime = text.contains("#include \"suz_runtime.h\"");
            let object = directory.join(format!("case-{index}.o"));
            let mut command = Command::new(&compiler);
            command
                .args(flags)
                .args(extra_flags)
                .arg(format!("-Dmain=suz_case_{index}"));
            if needs_runtime {
                command.arg("-I").arg(&runtime);
            }
            compile_c(command.arg("-c").arg(source).arg("-o").arg(&object))?;
            Ok((
                object,
                needs_runtime,
                super::float_runtime::required(&text),
                super::float_runtime::parser_required(&text),
                super::float_runtime::struct_parser_required(&text),
            ))
        });
        let mut errors = BTreeMap::new();
        let mut entries = Vec::new();
        let mut objects = Vec::new();
        let mut needs_runtime = false;
        let mut needs_float = false;
        let mut needs_float_parser = false;
        let mut needs_struct_parser = false;
        for ((index, _), result) in inputs.iter().zip(compiled) {
            match result {
                Ok((object, uses_runtime, uses_float, uses_float_parser, uses_struct_parser)) => {
                    entries.push(*index);
                    objects.push(object);
                    needs_runtime |= uses_runtime;
                    needs_float |= uses_float;
                    needs_float_parser |= uses_float_parser;
                    needs_struct_parser |= uses_struct_parser;
                }
                Err(error) => {
                    errors.insert(*index, error);
                }
            }
        }
        if entries.is_empty() {
            return Ok(Self {
                executable: None,
                errors,
            });
        }
        let mut driver = String::from("#include <stdio.h>\n#include <string.h>\n");
        for index in &entries {
            writeln!(driver, "int suz_case_{index}(void);").unwrap();
        }
        driver.push_str("int main(int argc, char **argv) {\n    if (argc == 2) {\n");
        for index in entries {
            writeln!(
                driver,
                "        if (strcmp(argv[1], \"{index}\") == 0) return suz_case_{index}();"
            )
            .unwrap();
        }
        driver.push_str(
            "    }\n    fputs(\"invalid case selector\\n\", stderr);\n    return 2;\n}\n",
        );
        let source = directory.join("case-driver.c");
        fs::write(&source, driver)
            .map_err(|error| format!("could not write case driver: {error}"))?;
        let executable = directory.join("case-runner");
        let mut command = Command::new(compiler);
        command
            .args(flags)
            .args(extra_flags)
            .arg(source)
            .args(objects);
        if needs_runtime {
            command
                .arg(runtime.join("suz_runtime.c"))
                .arg(runtime.join("suz_text.c"))
                .arg(runtime.join("suz_parse.c"))
                .arg("-lm")
                .arg(runtime.join("suz_env.c"));
        }
        if needs_struct_parser {
            command.arg(runtime.join("suz_parse_struct.c"));
        }
        if needs_float_parser {
            command.arg(runtime.join("suz_parse_float.c"));
        }
        if needs_float {
            super::float_runtime::link(&mut command, &runtime);
        }
        compile_c(command.arg("-o").arg(&executable))?;
        Ok(Self {
            executable: Some(executable),
            errors,
        })
    }

    pub fn run(&self, index: usize) -> Result<Output> {
        if let Some(error) = self.errors.get(&index) {
            return Err(error.clone());
        }
        let executable = self.executable.as_ref().ok_or("no executable cases")?;
        // Each invocation is a new process, including repeated settings of a case.
        timed_output(Command::new(executable).arg(index.to_string()), &RUN_TIME)
            .map_err(|error| format!("could not start case runner: {error}"))
    }
}
