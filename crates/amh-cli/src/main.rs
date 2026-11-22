mod project;

use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    match project::run(&args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}
