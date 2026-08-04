//! Embeds a content stamp of the CLI's sources into the binary.
//!
//! This is the "let the build system do it" half of issue 0363's simplification.
//! The freshness question — "was this binary built from these sources?" — is
//! answered by comparing a stamp taken HERE against one recomputed at run time,
//! rather than by a shell script comparing mtimes from outside.
//!
//! Cargo cannot answer the question itself, and it is worth being precise about
//! why, because for most consumers it CAN: Rust Entry packages call codegen as a
//! linked library (`nros-build` depends on `nros-cli-core` for exactly this
//! reason), so cargo fingerprints it normally and no guard is involved. Two
//! consumers are outside that graph:
//!
//!   * CMake / C / C++ — `find_program(nros)`, no cargo in the picture at all.
//!   * `nros sync` — it WRITES `.cargo/config.toml` and `nros-patch.toml`, i.e.
//!     it generates cargo's own input. A tool that produces the configuration
//!     cargo reads cannot be fingerprinted by cargo. That is bootstrap
//!     ordering, not a missing cargo feature.
//!
//! So the stamp exists for those two, and nothing else.

// `source_stamp.rs` is shared with the crate proper; build.rs uses only part of
// its surface, and an unused-function warning here would be noise, not signal.
#![allow(dead_code)]

use std::path::PathBuf;

include!("src/source_stamp.rs");

fn main() {
    // <root>/packages/cli/nros-cli-core -> <root>
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(PathBuf::from);

    let Some(root) = root else {
        println!("cargo:rustc-env=NROS_CLI_SOURCE_STAMP=unknown");
        return;
    };

    // Rerun when any CLI source changes. Emitting an explicit list REPLACES
    // cargo's default (rerun on any change in this package), so the list must
    // cover the whole sub-workspace — a change in `rosidl-codegen` must restamp
    // this crate even though it lives elsewhere.
    for rel in cli_input_files(&root) {
        println!("cargo:rerun-if-changed={}", root.join(&rel).display());
    }
    // …and when the index moves (commit, rebase, branch switch), since the
    // stamp reads index blob SHAs.
    let index = root.join(".git/index");
    if index.exists() {
        println!("cargo:rerun-if-changed={}", index.display());
    }

    let stamp = source_stamp(&root).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=NROS_CLI_SOURCE_STAMP={stamp}");

    // issue 0409 — the play_launch pin this CLI was built against. `nros sync`
    // shells out to `nros-launch-resolve`, which stamps the SAME value from its
    // own build; a mismatch means the resolver was compiled from a different
    // layer-2 checkout, and such a resolver produces models that are missing
    // DATA rather than failing (one predating rlm v0.1.1 silently drops every
    // `params` projection). Crate versions cannot catch it — the two are
    // versioned in lockstep and read the same number.
    //
    // Read from the SUBMODULE working tree, not the superproject gitlink: what
    // the resolver compiled in is the commit actually checked out.
    let play_launch = root.join("packages/cli/third-party/play_launch");
    let pin = if play_launch.exists() {
        std::process::Command::new("git")
            .args([
                "-C",
                &play_launch.display().to_string(),
                "rev-parse",
                "HEAD",
            ])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    println!("cargo:rustc-env=NROS_PLAY_LAUNCH_SHA={pin}");
}
