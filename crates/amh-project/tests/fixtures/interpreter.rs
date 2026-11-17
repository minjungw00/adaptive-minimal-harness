// Protocol fixture, not a language classifier. Compiled with the dev Rust toolchain.
use std::{
    env, fs,
    io::{self, Read},
    thread,
    time::Duration,
};
fn main() {
    let a: Vec<String> = env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("fail") => std::process::exit(17),
        Some("hang") => {
            thread::sleep(Duration::from_secs(5));
            return;
        }
        Some("invalid") => {
            println!("not valid JSON");
            return;
        }
        Some("oversize") => {
            print!("{}", "x".repeat(4 * 1024 * 1024 + 1));
            return;
        }
        _ => {}
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    fs::write(&a[3], &input).unwrap();
    let response = fs::read_to_string(&a[2]).unwrap();
    if a.get(4).map(String::as_str) == Some("exec") {
        assert!(a.windows(2).any(|p| p == ["--sandbox", "read-only"]));
        assert!(a.iter().any(|p| p == "--ephemeral"));
        assert!(a.iter().any(|p| p == "--skip-git-repo-check"));
        assert!(!input.trim_start().starts_with('{')); // Codex receives a bounded prompt and packet.
        let schema = a.iter().position(|p| p == "--output-schema").unwrap();
        assert!(fs::read_to_string(&a[schema + 1])
            .unwrap()
            .contains("request_digest"));
        let output = a.iter().position(|p| p == "--output-last-message").unwrap();
        fs::write(&a[output + 1], response).unwrap();
        println!("progress is not the structured answer");
    } else {
        assert!(input.trim_start().starts_with('{'));
        println!("{response}");
    }
}
