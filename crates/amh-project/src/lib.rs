//! User-owned project workflow. This is deliberately separate from the optional
//! protected host: local plans provide review and concurrency checks, not authentication.
mod model;
mod resolve;
mod verify;
mod workspace;

pub use model::*;
pub use resolve::*;
pub use verify::*;
pub use workspace::*;

pub type Result<T> = std::result::Result<T, String>;

pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
