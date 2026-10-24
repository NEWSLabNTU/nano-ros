//! `nros version` — Phase 111.A.12.

use eyre::Result;

pub fn run() -> Result<()> {
    println!("nros {}", env!("CARGO_PKG_VERSION"));
    // phase-429 W2 — the release version above answers "which build is this";
    // this answers "what does it emit, and what runtime can compile that". They
    // are different questions, and conflating them is what the old ABI guard
    // did. `nros --codegen-version` prints the number on its own, for scripts.
    println!("codegen version {}", crate::abi_guard::EMITTED_VERSION);
    // phase-478 I7 — and "which grammar judged this contract": the vendored
    // play_launch commit and the ros-launch-manifest tag. Here, not on
    // `--version`, because `install.sh`, `check-release-manifest.py` and the
    // launcher's dispatch read `--version` as one line.
    println!("{}", crate::BUILD_INFO);
    Ok(())
}
