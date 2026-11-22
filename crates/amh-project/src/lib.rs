//! User-owned policy storage and read-only delivery.
mod model;
mod policy_lookup;
mod render;
mod workspace;

pub use model::*;
pub use policy_lookup::*;
pub use workspace::*;

pub type Result<T> = std::result::Result<T, String>;

pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
