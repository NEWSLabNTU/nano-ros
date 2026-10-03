//! Issue 0452 — regenerate (or check) the COMMITTED cbindgen headers.
//!
//! `just regen-c-headers` runs this; `just check cbindgen-headers` runs it with
//! `--check`. It is the **only** writer of these files: build scripts compare
//! and warn (see `nros_build_helpers::generate_cbindgen_header`) so that no
//! build dirties the worktree.
//!
//! This is the Rust→C direction's equivalent of `scripts/gen-abi-bindings.sh`
//! plus `check-abi-bindings`, which have guarded the C→Rust direction since
//! RFC-0054. That asymmetry — one direction pinned and gated, the other
//! regenerated in place by every build — is what issue 0452 is.
//!
//! ## The table below is the SSoT for "which headers are committed cbindgen
//! output"
//!
//! The issue named two. A sweep for the CLASS (CLAUDE.md: fix the class, not the
//! reported site) found a third: `zpico-sys/c/include/zpico.h`, written in place
//! by `nros-zpico-build`'s own `generate_header`. All three are tracked, all
//! three were rewritten by builds, and all three are covered here.

#![forbid(unsafe_code)]

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

/// What a committed cbindgen header is made of.
///
/// `post` matters: `zpico.h` is NOT raw cbindgen output — `nros-zpico-build`
/// strips `extern ` lines and collapses blanks before writing. Regenerating it
/// without that pass would produce a file the build never wrote, so the
/// post-pass is part of the definition, not a detail of the old writer.
struct Header {
    /// Crate dir, relative to the repo root.
    crate_rel: &'static str,
    config: &'static str,
    /// Header path, relative to the crate dir.
    header_rel: &'static str,
    post: Option<fn(&str) -> String>,
    /// Rejects an implausible generation instead of committing it. zpico's
    /// build script has always had this guard ("keeping existing header"); a
    /// regenerator without it would happily write the truncated output.
    plausible: Option<fn(&str) -> bool>,
}

const HEADERS: &[Header] = &[
    Header {
        crate_rel: "packages/api/nros-c",
        config: "cbindgen.toml",
        header_rel: "include/nros/nros_generated.h",
        post: None,
        plausible: None,
    },
    Header {
        crate_rel: "packages/api/nros-cpp",
        config: "cbindgen.toml",
        header_rel: "include/nros/nros_cpp_ffi.h",
        post: None,
        plausible: None,
    },
    Header {
        crate_rel: "packages/rmw/zenoh/zpico-sys",
        config: "cbindgen.toml",
        header_rel: "c/include/zpico.h",
        post: Some(nros_zpico_build::post_process_header),
        plausible: Some(nros_zpico_build::is_plausible_generated_header),
    },
];

/// Render one header's final committed content.
fn render(root: &Path, h: &Header) -> Result<String, String> {
    let crate_dir = root.join(h.crate_rel);
    let raw = nros_build_helpers::render_cbindgen_header(&crate_dir, h.config)?;
    let out = match h.post {
        Some(f) => f(&raw),
        None => raw,
    };
    if let Some(check) = h.plausible
        && !check(&out)
    {
        return Err(format!(
            "cbindgen produced an implausible {} — refusing to write it",
            h.header_rel
        ));
    }
    Ok(out)
}

fn repo_root() -> PathBuf {
    // The binary lives in the workspace; CARGO_MANIFEST_DIR points at
    // packages/tooling/nros-build-helpers at compile time.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .unwrap_or(manifest)
        .to_path_buf()
}

/// The checkout whose headers this run reads and writes — issue 1538.
///
/// `$NROS_REPO_DIR` used to win outright, and a linked worktree inherits it
/// from the shell that spawned it, pointing at the PARENT checkout. So inside a
/// worktree `just check cbindgen-headers` compared the parent's crates against
/// the parent's committed headers and reported a verdict about a tree nobody
/// was editing — in the SAFE-LOOKING direction: a worktree that changed a
/// `#[repr(C)]` struct and forgot to regenerate got a green gate, because the
/// parent is consistent with itself.
///
/// The fix is the issue-1280 rule rather than a second spelling of it:
/// `nros_build_paths::reroot_foreign` keeps a value outside every checkout (the
/// out-of-tree case env-first exists for), keeps one inside THIS checkout, and
/// re-roots one inside a DIFFERENT checkout onto this one. "Which checkout" is
/// the marker walk, never `.git` — a worktree's `.git` is a file (issue 1336) —
/// and the walk stops at the innermost marker, which is what separates the
/// nested case (`<main>/.claude/worktrees/<id>`) that a prefix test cannot
/// (issue 1391).
///
/// Issue 1510 applied the same rule to workspace checkout resolution; this was
/// a second reader of the same variable that never got it.
fn resolve_root(env_value: Option<PathBuf>, here: &Path) -> PathBuf {
    match env_value {
        Some(value) => {
            let rerooted = nros_build_paths::reroot_foreign(&value, here);
            if rerooted != value {
                eprintln!(
                    "nros-cbindgen-headers: $NROS_REPO_DIR named another nano-ros \
                     checkout ({}); using this one ({}). A linked worktree inherits \
                     the parent shell's absolute paths — issue 1538 (rule: 1280).",
                    value.display(),
                    rerooted.display()
                );
            }
            rerooted
        }
        None => here.to_path_buf(),
    }
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let root = resolve_root(
        // repo-dir-env-ok: `resolve_root` applies `nros_build_paths::reroot_foreign` (issue 1538).
        std::env::var_os("NROS_REPO_DIR").map(PathBuf::from),
        &repo_root(),
    );

    let mut stale = Vec::new();
    let mut failed = false;

    for h in HEADERS {
        let crate_dir = root.join(h.crate_rel);
        let header = crate_dir.join(h.header_rel);
        if !crate_dir.join(h.config).is_file() {
            eprintln!("[FAIL] {}: no {}", crate_dir.display(), h.config);
            failed = true;
            continue;
        }

        let fresh = match render(&root, h) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[FAIL] {}: {e}", header.display());
                failed = true;
                continue;
            }
        };
        let committed = std::fs::read_to_string(&header).unwrap_or_default();

        if check {
            if committed != fresh {
                stale.push(header.clone());
            }
        } else if nros_build_helpers::write_committed_header(&header, &fresh) {
            println!("regenerated {}", header.display());
        } else {
            println!("unchanged   {}", header.display());
        }
    }

    if failed {
        return ExitCode::FAILURE;
    }
    if check {
        if stale.is_empty() {
            println!(
                "check-cbindgen-headers: OK ({} committed headers match a fresh generation)",
                HEADERS.len()
            );
        } else {
            eprintln!("[FAIL] these committed headers are STALE against their crate sources:");
            for h in &stale {
                eprintln!("         {}", h.display());
            }
            eprintln!("       Run `just regen-c-headers` and commit the result (issue 0452).");
            eprintln!("       If the diff is only the C23 enum-base guard, your cbindgen is not");
            eprintln!("       the pinned one — check `just check cbindgen-pin` first.");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// issue 1538 — the tool's ROOT decision, in both checkout shapes.
///
/// `reroot_foreign` is tested at its own home; what these pin is that THIS
/// tool's choice of root goes through it. Both shapes are built on disk rather
/// than asserted, because a side-by-side-only probe is what let issue 1391's
/// nesting case through: an agent worktree at `<main>/.claude/worktrees/<id>`
/// makes "keep" and "re-root" the same lexical test.
#[cfg(test)]
mod root_tests {
    use super::resolve_root;
    use std::path::{Path, PathBuf};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "nros-1538-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// A directory `nros_build_paths::checkout_root_of` recognises.
        fn checkout(&self, name: &str) -> PathBuf {
            let root = self.0.join(name);
            let marker = root.join(nros_build_paths::CHECKOUT_MARKER);
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(&marker, "[package]\nname = \"nros-core\"\n").unwrap();
            root
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn canon(p: &Path) -> PathBuf {
        p.canonicalize().unwrap()
    }

    #[test]
    fn a_sibling_worktree_does_not_check_the_parent() {
        let s = Scratch::new("sibling");
        let parent = s.checkout("main");
        let worktree = s.checkout("wt");
        let got = resolve_root(Some(parent), &worktree);
        assert_eq!(canon(&got), canon(&worktree));
    }

    /// The shape that matters here: agent worktrees NEST inside the parent, so
    /// the parent's root is a strict prefix of the worktree's.
    #[test]
    fn a_nested_worktree_does_not_check_the_parent() {
        let s = Scratch::new("nested");
        let parent = s.checkout("main");
        let worktree = s.checkout("main/.claude/worktrees/agent-x");
        let got = resolve_root(Some(parent.clone()), &worktree);
        assert_eq!(
            canon(&got),
            canon(&worktree),
            "the inherited parent must be re-rooted"
        );
        assert_ne!(canon(&got), canon(&parent));
    }

    /// The case env-first exists for, and the one a fix must not break.
    #[test]
    fn a_value_outside_every_checkout_is_still_honoured() {
        let s = Scratch::new("outside");
        let here = s.checkout("main");
        let sdk = s.0.join("not-a-checkout");
        std::fs::create_dir_all(&sdk).unwrap();
        assert_eq!(resolve_root(Some(sdk.clone()), &here), sdk);
    }

    #[test]
    fn this_checkout_and_no_value_both_mean_here() {
        let s = Scratch::new("here");
        let here = s.checkout("main");
        assert_eq!(
            canon(&resolve_root(Some(here.clone()), &here)),
            canon(&here)
        );
        assert_eq!(resolve_root(None, &here), here);
    }
}
