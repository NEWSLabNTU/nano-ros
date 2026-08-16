use std::{env, fs::File, io::Write, path::PathBuf};

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());

    // Copy memory.x to the output directory
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();

    // Tell the linker where to find memory.x
    println!("cargo:rustc-link-search={}", out.display());

    // Rebuild if memory.x changes
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");

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
