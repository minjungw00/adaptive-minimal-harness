//! Initial CLI surface: deliberately small, with no unimplemented promises.

use std::env;
use std::process::ExitCode;

const HELP: &str = "AMH — Adaptive Minimal Harness

USAGE:
    amh [OPTIONS]

OPTIONS:
    -h, --help       Show this help
    -V, --version    Show package metadata version

STATUS:
    Foundation only. Project installation, policy resolution, and enforcement
    commands are not implemented yet.
";

fn run(args: &[String]) -> Result<String, String> {
    match args {
        [] => Ok(HELP.to_owned()),
        [arg] if arg == "-h" || arg == "--help" => Ok(HELP.to_owned()),
        [arg] if arg == "-V" || arg == "--version" => {
            Ok(format!("amh {}", env!("CARGO_PKG_VERSION")))
        }
        _ => Err("unsupported command; run 'amh --help'".to_owned()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_describes_only_implemented_behavior() {
        let output = run(&["--help".to_owned()]).expect("help");
        assert!(output.contains("Foundation only"));
        assert!(!output.contains("amh init"));
    }

    #[test]
    fn unimplemented_commands_fail_explicitly() {
        assert!(run(&["init".to_owned()]).is_err());
        assert!(run(&["compile".to_owned()]).is_err());
    }

    #[test]
    fn package_version_is_available() {
        assert!(run(&["--version".to_owned()]).expect("version").starts_with("amh "));
    }
}
