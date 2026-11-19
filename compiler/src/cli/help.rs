//! CLIの使い方を表示するヘルプ文字列を組み立てる。
//!
//! 利用元: [`crate::cli`]。

use super::arguments::Command;

pub const USAGE: &str = "Usage:
  suz build INPUT -o OUTPUT [OPTIONS]
  suz run INPUT [OPTIONS]
  suz check INPUT [OPTIONS]
  suz emit-c INPUT -o DIRECTORY [OPTIONS]
  suz --version
  suz --help
  suz --licenses";

pub fn text(command: Option<Command>) -> String {
    let mut text = match command {
        None => format!(
            "{USAGE}\n\nCommands:\n  build    Build an executable\n  run      Build and run a program\n  check    Validate source without a C compiler\n  emit-c   Export C and runtime sources\n"
        ),
        Some(command) => {
            let output = match command {
                Command::Build => " -o OUTPUT",
                Command::EmitC => " -o DIRECTORY",
                _ => "",
            };
            format!("Usage: suz {} INPUT{output} [OPTIONS]\n", command.name())
        }
    };
    text.push_str("\nOptions:\n");
    if command.is_none() || matches!(command, Some(Command::Build | Command::EmitC)) {
        text.push_str("  -o PATH                        Output path (required)\n");
    }
    if command.is_none() || matches!(command, Some(Command::Build | Command::Run)) {
        text.push_str("  -O0, -O2                       Optimization (default: -O0)\n  --cc PATH                      Clang executable (default: clang on PATH)\n");
    }
    if command != Some(Command::Check) {
        text.push_str("  --env FILE                     Environment C source (default: bundled environment)\n");
    }
    if command != Some(Command::Check) {
        text.push_str(
            "  --arena-initial SIZE           Initial arena bytes; 0 or K/M/G (default: 64M)\n  --arena-grow SIZE              Arena growth bytes; 0 disables growth (default: 64M)\n  --arena-limit SIZE             Total arena bytes; K/M/G or unlimited (default: unlimited)\n  --text-pool-limit SIZE         Pool byte limit; K/M/G or unlimited (default: 1G)\n",
        );
    }
    if command.is_none() || command == Some(Command::Build) {
        text.push_str("  --force                        Replace an existing regular output file\n");
    }
    text.push_str("  --include-dir DIR              Add a C header search directory (repeatable)\n  --library-dir DIR              Add a library search directory (repeatable)\n  --link-library NAME            Link a C library (repeatable, ordered)\n  --link-object FILE             Link an object or archive (repeatable, ordered)\n");
    text.push_str("  --log-ignore                   Record executed ignores and their reasons (default: disabled)\n");
    text.push_str("  --log-level LEVEL              Minimum log level: debug/info/warn/error/off (default: info)\n");
    text.push_str("  --log-allow                    Show suppressed warnings and their reasons\n  --warn-unhandled-arithmetic     Warn about integer arithmetic without an or handler\n  --deny-unhandled-arithmetic     Reject integer arithmetic without an or handler\n  --help                         Display help\n  --                             Treat remaining arguments as filenames\n\nArithmetic diagnostics are disabled by default.\n");
    if command.is_none() {
        text.push_str("  --version                      Display the version\n  --licenses                     Display copyright and license notices\n");
    }
    text
}
