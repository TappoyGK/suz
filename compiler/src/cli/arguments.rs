use super::{Failure, Result};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Build,
    Run,
    Check,
    EmitC,
}

impl Command {
    pub fn name(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Run => "run",
            Self::Check => "check",
            Self::EmitC => "emit-c",
        }
    }
}

pub enum Action {
    Help(Option<Command>),
    Version,
    Licenses,
    Execute(Options),
}

pub struct Options {
    pub command: Command,
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub environment: Option<PathBuf>,
    pub cc: Option<PathBuf>,
    pub optimization: OsString,
    pub force: bool,
    pub mode: suz::ArithmeticMode,
}

fn invalid(message: impl std::fmt::Display) -> Failure {
    Failure::tool(2, "tool.arguments", message)
}

pub fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Action> {
    let Some(first) = args.next() else {
        let mut error = invalid("missing command");
        error.message.push('\n');
        error.message.push_str(super::help::USAGE);
        return Err(error);
    };
    let command = match first.to_str() {
        Some("--help" | "--version" | "--licenses") => {
            if args.next().is_some() {
                return Err(invalid("information options must be used alone"));
            }
            return Ok(match first.to_str() {
                Some("--help") => Action::Help(None),
                Some("--version") => Action::Version,
                _ => Action::Licenses,
            });
        }
        Some("build") => Command::Build,
        Some("run") => Command::Run,
        Some("check") => Command::Check,
        Some("emit-c") => Command::EmitC,
        _ => return Err(invalid(format!("unknown command: {}", first.display()))),
    };
    let mut input = None;
    let mut output = None;
    let mut optimization = None;
    let mut cc = None;
    let mut environment = None;
    let mut mode = None;
    let mut force = false;
    let mut help = false;
    let mut paths_only = false;
    while let Some(arg) = args.next() {
        let native = matches!(command, Command::Build | Command::Run);
        if !paths_only {
            match arg.to_str() {
                Some("--") => {
                    paths_only = true;
                    continue;
                }
                Some("--help") if !help => {
                    help = true;
                    continue;
                }
                Some("-o") if matches!(command, Command::Build | Command::EmitC) => {
                    path_option(&mut output, &mut args, "-o")?;
                    continue;
                }
                Some("--cc") if native => {
                    path_option(&mut cc, &mut args, "--cc")?;
                    continue;
                }
                Some("--env") if command != Command::Check => {
                    path_option(&mut environment, &mut args, "--env")?;
                    continue;
                }
                Some("-O0" | "-O2") if native => {
                    if optimization.replace(arg).is_some() {
                        return Err(invalid("optimization options are mutually exclusive"));
                    }
                    continue;
                }
                Some("--force") if command == Command::Build && !force => {
                    force = true;
                    continue;
                }
                Some("--warn-unhandled-arithmetic" | "--deny-unhandled-arithmetic") => {
                    let value = if arg == "--warn-unhandled-arithmetic" {
                        suz::ArithmeticMode::Warn
                    } else {
                        suz::ArithmeticMode::Deny
                    };
                    if mode.replace(value).is_some() {
                        return Err(invalid(
                            "arithmetic diagnostic options are mutually exclusive",
                        ));
                    }
                    continue;
                }
                _ => {}
            }
            if arg.as_encoded_bytes().starts_with(b"-") {
                return Err(invalid(format!(
                    "invalid or duplicate option: {}",
                    arg.display()
                )));
            }
        }
        if arg.is_empty() || input.replace(PathBuf::from(arg)).is_some() {
            return Err(invalid("expected exactly one input file"));
        }
    }
    if help {
        return Ok(Action::Help(Some(command)));
    }
    let input = input.ok_or_else(|| invalid("missing input file"))?;
    if matches!(command, Command::Build | Command::EmitC) && output.is_none() {
        return Err(invalid("missing output path; use -o PATH"));
    }
    Ok(Action::Execute(Options {
        command,
        input,
        output,
        environment,
        cc,
        optimization: optimization.unwrap_or_else(|| "-O0".into()),
        force,
        mode: mode.unwrap_or_default(),
    }))
}

fn path_option(
    slot: &mut Option<PathBuf>,
    args: &mut impl Iterator<Item = OsString>,
    name: &str,
) -> Result<()> {
    if slot.is_some() {
        return Err(invalid(format!("duplicate {name} option")));
    }
    let value = args
        .next()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid(format!("expected a path after {name}")))?;
    *slot = Some(value.into());
    Ok(())
}
