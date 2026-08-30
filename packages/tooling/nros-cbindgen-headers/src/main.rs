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

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let root = std::env::var("NROS_REPO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root());

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
