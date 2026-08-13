//! Stamp the binary with the play_launch commit it was compiled from.
//!
//! issue 0409 — a resolver built from an older `play_launch` checkout produces
//! models that are missing DATA rather than failing: one predating rlm v0.1.1
//! silently drops every `[[component]].params` / `params_files` projection, and
//! 22 models lost their params that way with no error anywhere.
//!
//! The crate VERSION cannot catch that: the resolver is versioned in lockstep
//! with the CLI, so a stale binary and a current one both read `0.5.0`. What
//! actually differs is the vendored layer-2 source it compiled in, which
//! advances by the play_launch SUBMODULE PIN. Both this binary and `nros`
//! stamp that pin, and `nros sync` refuses a resolver whose stamp is not its
//! own — the same "verify, don't assume" rule the CLI already applies to itself
//! (issues 0363/0197).
//!
//! Unknown is a legitimate outcome (a tarball build, a vendored source drop
//! with no git). It stamps `unknown`, and the consumer treats an unknown stamp
//! as unverifiable rather than as a mismatch.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let submodule = std::path::Path::new(&manifest)
        .parent()
        .map(|p| p.join("third-party/play_launch"));

    // issue 0419 — a `.git` FILE, not merely an existing directory. An
    // uninitialised submodule is an empty dir, and `git -C <empty dir>
    // rev-parse HEAD` walks UP and returns the SUPERPROJECT's HEAD, so the
    // binary would stamp a nano-ros commit as its play_launch pin. Same fault
    // as the CLI side; both must agree on "unknown" or the 0409 guard compares
    // two different things and reports an impossible mismatch.
    if let Some(p) = submodule.as_ref() {
        // The gitlink FILE itself is not enough: for a submodule `.git` holds
        // `gitdir: …` and its CONTENT never changes when the submodule's HEAD
        // moves — the move happens in `<gitdir>/HEAD`. Watching only the
        // gitlink means this build script never re-runs on a pin bump, so the
        // binary keeps a stale sha and the 0409 guard it feeds compares a lie.
        // Observed when the play_launch pointer moved for rlm v0.1.6: the
        // resolver rebuilt and still reported the previous commit.
        let gitlink = p.join(".git");
        println!("cargo:rerun-if-changed={}", gitlink.display());
        if let Ok(text) = std::fs::read_to_string(&gitlink)
            && let Some(rel) = text.strip_prefix("gitdir:").map(str::trim)
        {
            let gitdir = p.join(rel);
            println!("cargo:rerun-if-changed={}", gitdir.join("HEAD").display());
        }
    }
    let sha = submodule
        .filter(|p| p.join(".git").exists())
        .and_then(|p| {
            // `git -C <submodule> rev-parse HEAD`: from the SUPERPROJECT the
            // index holds only the gitlink, so a superproject-side lookup would
            // report the pin the tree is *supposed* to be at rather than the one
            // actually checked out — and the checked-out one is what got
            // compiled in.
            Command::new("git")
                .args(["-C", &p.display().to_string(), "rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=NROS_PLAY_LAUNCH_SHA={sha}");
}
