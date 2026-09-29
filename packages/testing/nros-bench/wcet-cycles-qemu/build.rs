//! Emit `memory.x` into OUT_DIR so `cortex-m-rt`'s `link.x` finds it, and bake
//! the provenance a cycle count means nothing without.
//!
//! phase-471 W4 — one helper, eleven scripts; see
//! `nros_build_paths::link_script`.

use std::env;

fn main() {
    nros_build_paths::link_script!("memory.x");

    // Issue 0403 item 3 — the conditions a cycle count means nothing without.
    //
    // Baked at build time because the binary cannot learn them at run time: it
    // is a no_std image with a semihosting stdout and no filesystem. Recorded
    // so a measurement can be audited later — "does this number still describe
    // this callback" is unanswerable without the commit and the profile.
    //
    // Both fall back to a literal that says UNKNOWN rather than to something
    // plausible. A wrong-but-plausible provenance is worse than an absent one:
    // it is the same failure as a manufactured WCET, one level up.
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=NROS_WCET_PROFILE={profile}");

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=NROS_WCET_COMMIT={commit}");
}
