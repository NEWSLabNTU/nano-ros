//! `nros version` — Phase 111.A.12.

use eyre::Result;

pub fn run() -> Result<()> {
    println!("nros {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
