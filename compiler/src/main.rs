mod cli;

use std::{
    io::{self, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(cli::run).unwrap_or_else(|_| {
        Err(cli::Failure::tool(
            3,
            "tool.exit",
            "internal compiler error",
        ))
    });
    match result {
        Ok(cli::Outcome::Code(code)) => ExitCode::from(code),
        Ok(cli::Outcome::Signal(number)) => cli::signals::terminate(number),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{}", error.message);
            ExitCode::from(error.code)
        }
    }
}
