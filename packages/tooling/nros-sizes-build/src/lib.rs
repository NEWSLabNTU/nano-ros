//! Build-script helper for extracting Rust-side type sizes from a compiled rlib.
//!
//! The sibling `nros` crate exports sizes of its internal handle types via
//! `export_size!`, which emits `#[used] static __NROS_SIZE_FOO: [u8; size_of::<Foo>()]`.
//! This crate provides two helpers that consumer build scripts (`nros-c/build.rs`,
//! `nros-cpp/build.rs`) can call to recover those sizes at build time:
//!
//! * [`find_dep_rlib`] — locate the rlib for a direct dependency by spawning
//!   a nested `cargo build --message-format=json` and parsing the artifact event.
//! * [`extract_sizes`] — parse an rlib as an `ar` archive and, for every defined
//!   symbol whose name begins with a given prefix, record its storage size.
//!
//! See [Phase 87](../../../../docs/roadmap/phase-87-nros-cpp-compile-time-sizes.md)
//! for the motivating design; [Phase 118.E](../../../../docs/roadmap/phase-118-E-size-probe-rigorization.md)
//! for the race-hardening rewrite.
//!
//! # Probe mechanism
//!
//! Two layered paths. The first that succeeds wins.
//!
//! 1. **Isolated nested cargo (primary).** Spawns `cargo build -p <crate>
//!    --target=<triple> --no-default-features --features=<resolved>
//!    --message-format=json` against a probe-only target dir
//!    (`$OUT_DIR/sizes-probe-target-<rustc-slug>/` by default; override
//!    via `NROS_SIZES_PROBE_TARGET_DIR`). The probe-only target dir
//!    sidesteps the outer cargo's exclusive flock on its target dir —
//!    same-dir nested invocations deadlock because cargo holds the lock
//!    for the entire outer build, including time waiting on build-script
//!    subprocesses. The `compiler-artifact` JSON event reports the
//!    canonical rlib path deterministically on completion. Cost: one
//!    duplicate compile of the probed crate per (target, features) on a
//!    cold probe cache; warm-cache reruns are sub-second.
//!
//! There is deliberately NO fallback. A filesystem-watch path used to poll
//! `<target>/<triple>/<profile>/deps/` for an rlib the outer build might
//! produce, and it was removed by issue 0464 (completing phase-118.E.6, which
//! had planned exactly this and deferred it). Two reasons, and the second is
//! the serious one:
//!
//! * It was a RACE. Whether the rlib appeared before a 60 s timeout depended on
//!   the outer cargo's scheduling, so the build was not deterministic.
//! * It selected the NEWEST matching rlib by mtime, which in a shared probe
//!   directory could be ANOTHER consumer's build — observed resolving
//!   `EXECUTOR_SIZE` to 88680 in one crate and 89392 in another within one
//!   workspace.
//!
//! These sizes become the opaque-storage macros C and C++ callers allocate
//! against, so an approximate answer is a short buffer rather than a wrong
//! report. The probe now either computes the size or fails the build.
//!
//! # Corrosion / cross-toolchain compatibility
//!
//! Cross-build env (corrosion-driven CMake, etc.) leaks target-side
//! `RUSTFLAGS` into every rustc invocation, which breaks host-side
//! proc-macro compiles inside the nested cargo. The probe strips
//! `RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`,
//! `CARGO_BUILD_TARGET`, and `CARGO_BUILD_TARGET_DIR` from the nested
//! env. Safe because:
//!
//! * Rlibs don't link → link-args don't apply.
//! * `size_of::<T>()` depends on the target *triple*'s data layout, not on
//!   `-C target-cpu` / `-C target-feature` (those affect codegen, not layout).
//!
//! `--no-default-features` is mandatory on the nested invocation: most
//! nros crates default to `std`, which would auto-link on bare-metal
//! targets and fail. The explicit `--features` arg below restores
//! whatever the consumer activated (including `std` when target=host).
//!
//! When even the env scrubbing isn't enough (e.g. custom-target JSON
//! specs that don't resolve in the nested invocation), the filesystem
//! fallback takes over.

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::Command,
};

use object::{File as ObjectFile, Object, ObjectSymbol, read::archive::ArchiveFile};

/// Errors returned by this crate's helpers.
#[derive(Debug)]
pub enum Error {
    /// `cargo metadata` could not be invoked or returned a non-zero exit status.
    CargoMetadata(String),
    /// The metadata JSON was missing an expected field or had the wrong shape.
    MalformedMetadata(&'static str),
    /// No rlib matching `lib<name>-*.rlib` was found in any candidate `deps/` directory.
    RlibNotFound {
        crate_name: String,
        searched: Vec<PathBuf>,
    },
    /// I/O error reading a file from disk.
    Io(std::io::Error),
    /// The `object` crate could not parse the rlib or one of its members.
    Object(object::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CargoMetadata(msg) => write!(f, "cargo metadata failed: {msg}"),
            Error::MalformedMetadata(field) => {
                write!(f, "cargo metadata missing or malformed field: {field}")
            }
            Error::RlibNotFound {
                crate_name,
                searched,
            } => {
                write!(
                    f,
                    "no rlib matching lib{crate_name}-*.rlib found; searched: {searched:?}"
                )
            }
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Object(e) => write!(f, "object parse error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<object::Error> for Error {
    fn from(e: object::Error) -> Self {
        Error::Object(e)
    }
}

/// Issue 0563 — make the CONSUMING build script re-run when the crate this
/// probe measured changes.
///
/// The probe answers "how big is `Executor` on this target", and its consumers
/// bake that number into a generated header (`EXECUTOR_OPAQUE_U64S`). But a
/// build script is re-run only when its OWN package changes, never when a
/// dependency does — so editing `nros-node`'s layout left the number stale,
/// while `nros-c` itself recompiled against the new layout. The two then
/// disagreed and the const assert fired:
///
/// ```text
/// error[E0080]: evaluation panicked: EXECUTOR_OPAQUE_U64S too small for
/// Executor + backing
/// ```
///
/// which names neither the real cause nor a working remedy — the suggested
/// knobs are unrelated, and only `touch`ing the consumer's `build.rs` cleared
/// it. That is the issue-0196 shape (a probe watching less than it consumes)
/// and the same family as the sizes-header mirror bugs 0088/0114/0122/0123/
/// 0245/0268.
///
/// The watch list is rustc's own depfile for the probe rlib, so it is exact and
/// carries no hardcoded paths: every source that went into the measurement is
/// watched, and nothing else.
/// Locate rustc's depfile for a probe rlib. Issue 0563 / phase-400 W5.
///
/// Cargo writes the depfile beside the UPLIFTED artifact
/// (`<profile>/libnros.rlib` + `<profile>/libnros.d`) and NOT beside the hashed
/// copy in `deps/`. Measured in this repo's shared probe store: 182 uplifted
/// rlibs, 182 depfiles, and 269 `deps/` rlibs with none.
///
/// `filenames` in cargo's `compiler-artifact` event can name either, so a lookup
/// that only tried `rlib.with_extension("d")` found nothing whenever it got the
/// `deps/` spelling — which is why the caller's watch list was empty in some
/// builds and complete in others, for the same crate, minutes apart.
fn probe_depfile(rlib: &Path) -> Option<PathBuf> {
    let beside = rlib.with_extension("d");
    if beside.is_file() {
        return Some(beside);
    }
    // `deps/libnros-<hash>.rlib` -> `../libnros.d`. The hash suffix is what
    // distinguishes the unit; the depfile is named for the CRATE.
    let parent = rlib.parent()?;
    if parent.file_name()? != "deps" {
        return None;
    }
    let stem = rlib.file_stem()?.to_str()?;
    let (name, _hash) = stem.rsplit_once('-')?;
    let uplifted = parent.parent()?.join(format!("{name}.d"));
    uplifted.is_file().then_some(uplifted)
}

fn emit_probe_watches(rlib: &Path) {
    let dep = probe_depfile(rlib).unwrap_or_else(|| {
        // NOT a silent return. This used to be `let Ok(..) = read else { return }`,
        // which emitted ZERO watches whenever the depfile was not where it looked
        // — see `probe_depfile` for why that was most of the time — and left the
        // consumer's build script watching nothing of the crate it measured.
        //
        // That is the exact defect issue 0563 filed and this function was written
        // to fix, reintroduced by its own error handling, and it is silent in the
        // direction that ships: the stale size surfaces later as
        // `EXECUTOR_OPAQUE_U64S too small` in a different crate.
        //
        // phase-400 W5 measured what it costs once probe dirs are SHARED: cargo
        // decides freshness from the RECORDED path list, so one build that
        // recorded no watches governs every consumer keyed the same way. Observed
        // as 132 paths in nine Zephyr trees and 21 in a tenth
        // (`just shared-dir-churn`).
        //
        // So the contract in the doc comment above — "every source that went into
        // the measurement is watched" — is enforced rather than described. If it
        // cannot be met, the build stops here, where the message can name the
        // cause, instead of two crates away where it cannot.
        panic!(
            "nros-sizes-build: no depfile for probe rlib {}\n\
             Looked beside it and, for a `deps/` artifact, at the uplifted \
             sibling one directory up.\n\
             Without it this probe cannot state what it measured, and the \
             consumer would silently stop rebuilding when that crate changes \
             (issue 0563).",
            rlib.display()
        )
    });
    let Ok(text) = std::fs::read_to_string(&dep) else {
        panic!(
            "nros-sizes-build: cannot read probe depfile {}",
            dep.display()
        )
    };
    // `<target>: <src> <src> …`. Only the first colon separates; Windows drive
    // letters are not a concern here and paths in this tree contain no spaces.
    let Some((_, rest)) = text.split_once(':') else {
        return;
    };
    // Anything under the probe's own target dir is a build artifact (OUT_DIR
    // `*_config.rs` and friends). Watching those would arm a rebuild on output
    // this very probe produces, so they are skipped — sources only.
    let probe_root = rlib.ancestors().find(|p| p.ends_with("sizes-probe"));
    for f in rest.split_whitespace() {
        let path = Path::new(f);
        if let Some(root) = probe_root
            && path.starts_with(root)
        {
            continue;
        }
        println!("cargo:rerun-if-changed={f}");
    }
}

/// Locate the rlib for `crate_name` containing Phase 87's size-probe
/// symbols (any defined symbol starting with `symbol_prefix`).
///
/// Builds `crate_name` in an isolated nested target dir and returns the
/// resulting rlib. There is no fallback: on failure the caller must fail the
/// build rather than guess a size (issue 0464).
pub fn find_dep_rlib(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    find_dep_rlib_isolated(crate_name, symbol_prefix)
}

fn find_dep_rlib_isolated(crate_name: &str, symbol_prefix: &str) -> Result<PathBuf, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target = env::var("TARGET").map_err(|_| Error::MalformedMetadata("TARGET"))?;
    // phase-336 — the probe must compile at the SAME profile as the outer
    // build. `PROFILE` only ever says `debug` or `release` (cargo reports the
    // INHERITED base for a custom profile), so a `nros-relwithdebinfo` outer
    // build made this probe run a full extra `--release` compile of the crate
    // under test. `profile_dir_name()` recovers the real one from `OUT_DIR`.
    let profile = profile_dir_name()
        .or_else(|| env::var("PROFILE").ok())
        .unwrap_or_else(|| "debug".to_string());

    // Phase 118.E.2 (rustc isolation): include a slug derived from
    // `rustc -V` in the probe target dir name. Without it, switching
    // toolchains (e.g. rustup nightly → stable, or a corrosion build
    // that overrides CARGO_BUILD_RUSTC) leaves rmeta files in the
    // probe dir compiled by the previous rustc; the next cargo run
    // explodes with E0514 "found crate `X` compiled by an incompatible
    // version of rustc" instead of recompiling from scratch.
    // The rustc slug is appended in BOTH branches. It is what keeps a probe dir
    // from mixing rmeta produced by different toolchains — switch rustup
    // channels, or let corrosion override `CARGO_BUILD_RUSTC`, and the next
    // nested cargo dies with E0514 ("compiled by an incompatible version of
    // rustc") instead of recompiling.
    //
    // phase-336 W7: the env branch used the caller's path VERBATIM, so a
    // SHARED probe dir — the whole point of the env knob — silently gave up
    // that isolation. Sharing is only safe because the slug is here.
    let rustc_slug = rustc_version_slug();
    // Resolve the feature set BEFORE choosing the dir — it is part of the key.
    // (The nested `--features` argument below uses the same value.)
    let forwarded = resolved_features_for(crate_name).unwrap_or_else(|e| {
        println!(
            "cargo:warning=nros-sizes-build: feature-set resolution \
             failed ({e}); falling back to identity forwarding"
        );
        forwarded_features()
    });
    let probe_target_dir = if let Ok(dir) = env::var("NROS_SIZES_PROBE_TARGET_DIR") {
        // A SHARED dir must be keyed by everything that changes the probe's
        // ANSWER, not just by the toolchain.
        //
        // phase-336 W7 keyed it by the rustc slug alone. Nine differently-
        // featured `nros` rlibs then piled into one directory, and the
        // filesystem fallback — which picks the NEWEST rlib by mtime — could
        // return another consumer's build. That is not theoretical: it made
        // nros-c and nros-cpp resolve different `EXECUTOR_SIZE` values (88680
        // vs 89392) in the same workspace, tripping the header-agreement guard
        // and failing every C++ workspace fixture build.
        //
        // Keying by (target, features) restores the isolation while keeping the
        // reuse: consumers that would produce the SAME artifact still share.
        let key = probe_key(&target, &forwarded);
        PathBuf::from(dir).join(&rustc_slug).join(key)
    } else if let Some(root) = nros_build_paths::try_repo_root() {
        // phase-343 I1 — the SHARED dir is now the DEFAULT, not an opt-in.
        //
        // It used to be reachable only by exporting NROS_SIZES_PROBE_TARGET_DIR,
        // which `scripts/build/cargo.sh` does — so anything not transiting that
        // script (a bare `cargo build` in a leaf, a nested cmake/corrosion
        // probe, an IDE) silently took the branch below and paid ~195 MiB for a
        // private copy. Measured: 425 leaked probe dirs, 63.1 GiB, deduplicating
        // 81:1. Both branches were live in the same tree in the same week and
        // the wasteful one was the default, with no diagnostic.
        //
        // Keyed IDENTICALLY to the env branch — (rustc slug, target, features).
        // That keying is not cosmetic: phase-336 W7 keyed by rustc slug alone,
        // nine differently-featured `nros` rlibs piled into one directory, and
        // the mtime-newest fallback returned another consumer's build (nros-c
        // and nros-cpp resolved EXECUTOR_SIZE 88680 vs 89392). Sharing without
        // this key is worse than not sharing.
        let key = probe_key(&target, &forwarded);
        root.join("build")
            .join("sizes-probe")
            .join(&rustc_slug)
            .join(key)
    } else {
        // Out-of-tree consumer with no nano-ros checkout to find: keep the
        // private dir. Correctness first — there is no shared root to share.
        let out_dir = env::var("OUT_DIR").map_err(|_| Error::MalformedMetadata("OUT_DIR"))?;
        PathBuf::from(out_dir).join(format!("sizes-probe-target-{rustc_slug}"))
    };

    // phase-353 W4 — record WHY this directory exists.
    //
    // The key is an opaque FNV hash, so a probe dir carries no evidence of what
    // produced it. Measured 2026-08-15: 110 sub-key directories under one rustc
    // key holding 18 distinct `nros-core` identities, 37 GB — and there was no
    // way to tell whether they were split by target, by features or by knobs
    // without re-deriving every consumer's build. A key that cannot be
    // attributed cannot be narrowed, so narrowing it starts here.
    //
    // Write-once (`create_new`): identical for a given key by construction, and
    // never rewritten, so concurrent probes racing on the same directory cannot
    // expose a partial read and no mtime is restamped (issue 0562).
    //
    // Diagnostic only — nothing reads this back. It must never become an input
    // to the key it describes.
    if let Err(e) = write_key_provenance(&probe_target_dir, &target, &forwarded) {
        // A probe that cannot write its own note is not a probe that should
        // fail; the sizes are what matter.
        println!("cargo:warning=nros-sizes-build: could not record probe key inputs: {e}");
    }

    let mut cmd = Command::new(&cargo);
    cmd.env("CARGO_TARGET_DIR", &probe_target_dir)
        .arg("build")
        .arg("-p")
        .arg(crate_name)
        .arg("--target")
        .arg(&target)
        // Phase 118.E.2: must disable default features so the nested
        // invocation matches the outer's intent. Most nros crates
        // default to `std`; on bare-metal targets (`thumbv7m-none-eabi`
        // etc.) auto-enabling `std` makes `nros-serdes` and friends
        // emit `extern crate std` and fail with E0463 "can't find crate
        // for `std`". The explicit `--features` arg below restores
        // whatever the consumer actually activated (including `std`
        // when target=host).
        .arg("--no-default-features")
        .arg("--message-format=json-render-diagnostics");
    // Flags from the shared table, keyed by the target-directory name we
    // recovered above (see `nros_cargo_profile::build_args_for_dir`).
    for flag in nros_cargo_profile::build_args_for_dir(&profile) {
        cmd.arg(flag);
    }

    // NuttX build-std targets (`*-nuttx-*`) have NO precompiled `std`/`core`
    // shipped with the toolchain, so — unlike host or the tier-2 embedded
    // triples (zephyr `thumbv*`, riscv32, esp32, baremetal, threadx, all of
    // which ship a precompiled `core`/`std`) — the nested probe must compile
    // the standard library from source, exactly as the board FFI crate's
    // outer build does. Without this the nested cargo fails
    // `error[E0463]: can't find crate for core`, the probe falls back to the
    // filesystem watch (which loses a timing race against the slow
    // std-from-source compile), and the consumer drops to the committed
    // `NUTTX_FALLBACK_SIZES` — which silently rots below the real
    // `size_of::<Executor>()` and trips the `EXECUTOR_OPAQUE_U64S too small`
    // const assertion.
    //
    // Mirror the board FFI's `.cargo/config.toml`
    // (`packages/boards/nros-board-nuttx-*/nros-nuttx-ffi/.cargo/config.toml`):
    //   * `[unstable] build-std = ["std", "panic_abort"]`
    //   * `build-std-features = ["compiler-builtins-mem"]`
    //   * `[patch.crates-io] libc = { path = "third-party/nuttx/libc" }`
    // The libc patch is mandatory: std's NuttX port references symbols the
    // crates.io libc lacks (e.g. `_SC_HOST_NAME_MAX`), so without it
    // std-from-source fails with `error[E0425]: cannot find value
    // _SC_HOST_NAME_MAX in crate libc`. The patched libc lives at a stable
    // repo-relative path shared by every NuttX board, so it's discoverable by
    // walking up from `CARGO_MANIFEST_DIR`; inject it via `--config`.
    //
    // The nightly toolchain is inherited from the outer NuttX build (its
    // `rust-toolchain.toml` pins the nightly that `-Z build-std` requires);
    // `ffi-size-markers` (which emits the probed `__NROS_SIZE_*` symbols) is
    // supplied by workspace feature unification, same as on the host probe.
    // STRICTLY gated on a NuttX triple: host + every precompiled-std target is
    // untouched, and on a non-nightly toolchain the `-Z` flags just fail the
    // nested build → the existing filesystem fallback runs (no worse than
    // before).
    if target.contains("nuttx") {
        cmd.arg("-Z").arg("build-std=std,panic_abort");
        cmd.arg("-Z")
            .arg("build-std-features=compiler-builtins-mem");
        if let Some(libc_dir) = find_nuttx_patched_libc() {
            cmd.arg("--config").arg(format!(
                "patch.crates-io.libc.path=\"{}\"",
                libc_dir.display()
            ));
        } else {
            println!(
                "cargo:warning=nros-sizes-build: NuttX target {target} but the patched \
                 libc (third-party/nuttx/libc) was not found relative to \
                 CARGO_MANIFEST_DIR; std-from-source will likely fail and the probe \
                 will fall back to the committed NuttX fallback sizes"
            );
        }
    }

    // Phase 118.E.2 (corrosion compat): scrub env vars that the outer
    // cross-build (typically corrosion-driven CMake) injects globally
    // and which break the nested cargo's host-side proc-macro compiles.
    // `RUSTFLAGS` applies to every rustc invocation under cargo,
    // including host crates like `proc-macro2`; cross-target link-args
    // (`-C link-arg=...`, `-C linker=...`) make those fail. Stripping
    // them is safe for size-probing because:
    //   * rlibs don't link, so link-args don't matter;
    //   * `size_of::<T>()` depends on the target *triple*'s data
    //     layout, not on `-C target-cpu` / `-C target-feature` (those
    //     control codegen, not layout).
    // We keep `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` because it's already
    // target-scoped and won't poison host builds.
    // ISSUE 0022 — strip the make jobserver from the nested probe cargo. This is
    // the SOURCE fix for the cyclone fixture deadlock (every platform's cyclone
    // build goes through `nros`, hence this probe). When the outer build runs
    // under a GNU make jobserver (the fixture builder uses `make
    // --jobserver-style=fifo`), the outer cargo holds jobserver tokens and then
    // BLOCKS in this build script waiting for the nested probe cargo below; if
    // the probe inherited the same jobserver it would wait for a token the outer
    // cargo holds → circular wait (cargo does not release its tokens before
    // blocking on a child cargo — a known recursive-cargo jobserver hazard).
    // Removing `MAKEFLAGS` / `CARGO_MAKEFLAGS` makes the probe use its OWN job
    // budget, so it never competes for the parent's tokens and the deadlock
    // cannot form — on ANY platform, without disabling jobserver coordination
    // for the outer build. DO NOT drop these two without restoring an
    // equivalent jobserver strip (see the issue-0022 box in
    // scripts/build/fixture-make-driver.sh).
    for var in [
        "RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_BUILD_TARGET_DIR",
        "MAKEFLAGS",
        "CARGO_MAKEFLAGS",
        "MAKELEVEL",
    ] {
        cmd.env_remove(var);
    }

    // Phase 118.E.2: derive the feature set for the nested invocation
    // by intersecting the consumer's active features (CARGO_FEATURE_*
    // env vars) with the probed crate's declared features (queried via
    // `cargo metadata --no-deps`). This filter is necessary because
    // consumer crates may carry features the probed crate doesn't
    // (e.g. `unstable-zenoh-api` is exposed by nros-cpp but not by
    // nros), and `cargo build --features <unknown>` errors out.
    // `forwarded` was resolved above — it keys the probe dir as well.
    if !forwarded.is_empty() {
        cmd.arg("--features").arg(forwarded.join(","));
    }

    let output = cmd
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        // Write full stderr to a debug log next to the probe target dir
        // so the user can inspect the actual rustc error message; the
        // `cargo:warning=` carries only a path pointer + short summary.
        let log_path = probe_target_dir.join("nested-cargo-stderr.log");
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&log_path, &output.stderr);

        let stderr = String::from_utf8_lossy(&output.stderr);
        let last = stderr
            .lines()
            .filter(|l| l.starts_with("error") || l.starts_with("  --> ") || l.starts_with("note:"))
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(Error::CargoMetadata(format!(
            "nested cargo build failed (full log: {}): {}",
            log_path.display(),
            if last.is_empty() {
                "(no error-prefixed lines captured)"
            } else {
                &last
            }
        )));
    }

    for line in output.stdout.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if json.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let Some(target_obj) = json.get("target") else {
            continue;
        };
        if target_obj.get("name").and_then(|n| n.as_str()) != Some(crate_name) {
            continue;
        }
        let Some(filenames) = json.get("filenames").and_then(|f| f.as_array()) else {
            continue;
        };
        for fname in filenames {
            let Some(s) = fname.as_str() else { continue };
            if s.ends_with(".rlib") {
                let path = PathBuf::from(s);
                // Validate symbols are present before returning. If the
                // rlib was compiled without RMW features (e.g. workspace
                // default `cargo check`), the probe symbols won't exist
                // and the consumer should fall through to its
                // `unwrap_or(0)` path.
                if let Ok(sizes) = extract_sizes(&path, symbol_prefix)
                    && !sizes.is_empty()
                {
                    emit_probe_watches(&path);
                    return Ok(path);
                }
                // Symbol-less rlib — still return the path so callers
                // can emit a warning and fall back.
                emit_probe_watches(&path);
                return Ok(path);
            }
        }
    }

    Err(Error::RlibNotFound {
        crate_name: crate_name.to_string(),
        searched: vec![probe_target_dir],
    })
}

/// Locate the NuttX-patched `libc` (`third-party/nuttx/libc`) by walking up
/// from `CARGO_MANIFEST_DIR`. Used to inject the board FFI crate's
/// `[patch.crates-io] libc` into the nested build-std probe for NuttX targets
/// (the consumer crates `nros-c` / `nros-cpp` live at
/// `packages/core/<crate>`, so the repo root — and thus `third-party/nuttx/libc`
/// — is a few levels up). Returns `None` when no such directory exists on the
/// ancestry (e.g. the patched-libc submodule isn't checked out).
fn find_nuttx_patched_libc() -> Option<PathBuf> {
    let start = env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut dir = PathBuf::from(start);
    loop {
        let candidate = dir.join("third-party/nuttx/libc");
        if candidate.join("Cargo.toml").is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Phase 118.E.2: intersect consumer's active features with the probed
/// crate's declared features.
///
/// Algorithm:
///
/// 1. Read consumer's `CARGO_FEATURE_<NAME>=1` env vars (via
///    [`forwarded_features`]) — the names the outer cargo activated on
///    the consumer crate.
/// 2. Run `cargo metadata --format-version=1 --no-deps` from
///    `CARGO_MANIFEST_DIR` to list workspace packages. Walk packages
///    for one named `crate_name` (the probed crate) and read its
///    `features` table (the full feature universe declared in its
///    `Cargo.toml`).
/// 3. Return the intersection — features the consumer activated AND
///    the probed crate actually declares. Anything else would cause
///    `cargo build --features <unknown>` to error.
///
/// Returns an empty Vec (not an error) if the probed crate isn't
/// listed in the workspace metadata; isolated-mode callers fall back
/// to identity forwarding in that case via [`forwarded_features`].
fn resolved_features_for(crate_name: &str) -> Result<Vec<String>, Error> {
    use std::collections::HashSet;

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| Error::MalformedMetadata("CARGO_MANIFEST_DIR"))?;

    let output = Command::new(&cargo)
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--no-deps")
        .current_dir(&manifest_dir)
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;

    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    let meta: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::CargoMetadata(format!("invalid JSON: {e}")))?;

    let packages = meta
        .get("packages")
        .and_then(|p| p.as_array())
        .ok_or(Error::MalformedMetadata("packages"))?;

    let declared: HashSet<String> = packages
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(crate_name))
        .and_then(|p| p.get("features"))
        .and_then(|f| f.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    if declared.is_empty() {
        // Crate not listed in `--no-deps` workspace metadata (e.g. a
        // git or registry dep). Caller's fallback handles it.
        return Ok(Vec::new());
    }

    // issue 0665 — forward what the caller's feature table TURNS ON in the
    // probed crate, not just the names the two happen to share.
    //
    // Name-intersection alone silently under-forwards whenever a caller's
    // feature enables a DIFFERENTLY-NAMED feature of the probed crate. That is
    // exactly what `nros-c` does:
    //
    //     std = ["alloc", "nros/std", "nros/env", "nros-node/std", …]
    //
    // Before phase-359 W10, `nros/std` implied `env`, so forwarding the shared
    // name `std` happened to reproduce the caller's configuration. W10 made
    // `env` an independent capability — correctly — and the probe then built
    // the facade WITHOUT it while the consumer linked WITH it. The measured
    // `ExecutorInlineStorage` came out 16 bytes (one fat pointer) short, and
    // `EXECUTOR_OPAQUE_U64S` with it: 11189 where the linked type needs 11191.
    // The `nros-c` assert caught it; on the C side the same number sizes a
    // caller's `_opaque` buffer, where too small is a silent overrun.
    //
    // So also read the caller's own `[features]` table and collect every
    // `<probed-crate>/<feat>` an ACTIVE caller feature enables. Deterministic,
    // local, no second cargo invocation — and it states the rule the manifest
    // already encodes rather than inferring one from names.
    let extra = features_enabled_on_dep(&manifest_dir, crate_name).unwrap_or_default();

    Ok(merge_forwarded(forwarded_features(), extra, &declared))
}

/// The probe's feature list — issue 0665.
///
/// `base` is what the caller's own `CARGO_FEATURE_*` say; `extra` is what its
/// feature table turns on in the probed crate under OTHER names (the half that
/// was missing); `declared` is what the probed crate actually has, because
/// `cargo build --features <unknown>` is an error.
///
/// Split out from `resolved_features_for` so the rule can be tested without a
/// `cargo metadata` call — the first version of this fix was exercised only
/// through the parser, and deleting the wiring left every test green.
fn merge_forwarded(
    base: Vec<String>,
    extra: Vec<String>,
    declared: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut out: Vec<String> = base
        .into_iter()
        .chain(extra)
        .filter(|f| declared.contains(f))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Produce a path-safe slug from `rustc -V` (or `$CARGO_BUILD_RUSTC -V`
/// when set) for use as a probe target dir suffix. Keeps probe
/// artefacts from different rustc versions / channels from colliding.
/// Stable short key for a probe dir: everything that changes the probe's ANSWER
/// other than the toolchain (which the caller adds separately).
///
/// FNV-1a rather than a hashing crate: this crate is deliberately dependency-free
/// so a build script can use it without dragging a graph along.
/// Record the inputs behind a probe directory's key, next to the artifacts.
///
/// See the call site for why (phase-353 W4). Written once and never updated.
fn write_key_provenance(dir: &Path, target: &str, features: &[String]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let mut body = String::new();
    body.push_str("# nros sizes-probe key inputs (phase-353 W4). Diagnostic only.\n");
    body.push_str("# Written once, when this directory was first created.\n");
    body.push_str(&format!("rustc\t{}\n", rustc_version_slug()));
    body.push_str(&format!("target\t{target}\n"));
    let mut sorted: Vec<&str> = features.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    body.push_str(&format!("features\t{}\n", sorted.join(",")));
    for (k, v) in knob_identity() {
        body.push_str(&format!("knob\t{k}={v}\n"));
    }
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dir.join("nros-probe-key-inputs.txt"))
    {
        Ok(mut f) => f.write_all(body.as_bytes()),
        // Already recorded by whoever created the directory first.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

fn probe_key(target: &str, features: &[String]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |b: &[u8]| {
        for byte in b {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(target.as_bytes());
    // Sorted so feature ORDER cannot split an otherwise-identical probe.
    let mut sorted: Vec<&str> = features.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    for f in sorted {
        mix(b"\x1f");
        mix(f.as_bytes());
    }
    // issue 0528 — the SIZING KNOBS are part of the identity too.
    //
    // The probe measures `ExecutorInlineStorage`, whose size depends on
    // `NROS_EXECUTOR_MAX_CBS` and friends. Since issue 0460 those knobs resolve
    // from the env OR from Zephyr's `$DOTCONFIG`, so two leaves can share a
    // (rustc, target, features) key and still compile different-sized
    // executors: `examples/workspaces/features/src/zephyr_rust_*_entry` set
    // `CONFIG_NROS_EXECUTOR_MAX_CBS=16` while `examples/zephyr/rust/*` take the
    // default 4.
    //
    // Sharing a probe dir across that difference is order-dependent corruption:
    // whichever leaf probes FIRST writes the sizes, and if it was the 4-CBS one
    // the 16-CBS leaf then compiles against a constant too small for its own
    // storage and dies on nros-c's `EXECUTOR_OPAQUE_U64S too small` assert. It
    // survives a clean rebuild of the failing leaf because the poisoned dir is
    // the SHARED one, which is why this looked like a latent condition.
    //
    // The comment above this function already says "sharing without this key is
    // worse than not sharing" — this is the other half of the key it was
    // talking about.
    for (k, v) in knob_identity() {
        mix(b"\x1e");
        mix(k.as_bytes());
        mix(b"=");
        mix(v.as_bytes());
    }
    format!("{h:016x}")
}

/// Every input that can change a probed SIZE, resolved the same two ways the
/// consuming crate resolves them (issue 0460): explicit `NROS_*` env, else the
/// matching `CONFIG_NROS_*` line in Zephyr's `$DOTCONFIG`.
///
/// Returned sorted so the key cannot split on iteration order. Deliberately
/// broad — an unknown-but-set `NROS_*` knob keys the probe rather than silently
/// sharing with a build that did not set it.
/// Environment variables that start `NROS_` but CANNOT change a probed size.
///
/// phase-353 W4. `knob_identity()` sweeps every `NROS_*` in the environment,
/// which is the right default for an UNKNOWN knob (issue 0528: a knob that
/// changes a size and does not key the probe is order-dependent corruption).
/// But most of what is set here is BUILD ORCHESTRATION, and some of it is
/// different on every run.
///
/// # The measurement that produced this list
///
/// 25 probe directories from one `build-test-fixtures lane=native`, all on ONE
/// target triple, 19 of them sharing the SAME feature set. 35 knobs varied
/// inside that group of 19 and not one was a sizing knob:
///
/// ```text
/// NROS_BUILD_LOG_DIR    .../logs/20260815-111859-1157807-9133   <- timestamp+pid
/// NROS_WS_RECORDS_FILE  .../ws-linux-20260815-112230-1214903-group-10.records
/// NROS_FIXTURE_ID       11 values
/// NROS_KIND_*           ~20 per-kind marker strings
/// NROS_BUILD_JOBS       24 vs 6
/// ```
///
/// The timestamped ones are the reason the directory grows without bound: they
/// differ on EVERY run, so every fixture build mints probe keys that can never
/// be reused. Measured 2026-08-15: 110 sub-key directories holding 18 distinct
/// `nros-core` identities, 37 GB, where one lane creates only 25.
///
/// This is issue 0491's class one layer up — never fingerprint on a value that
/// is a location or a session, because it is compared as TEXT. There it split
/// cargo fingerprints; here it splits probe directories.
///
/// # The rule
///
/// Anything NOT listed keys the probe exactly as before, so forgetting to add a
/// name here costs a wasted directory, never corruption. Every entry carries an
/// argument, and the four names that must NEVER be excluded are asserted in
/// tests — each points at a FILE whose CONTENT carries sizing knobs, so
/// excluding them would reintroduce issue 0528 by a new route:
/// `NROS_BOARD_TOML`, `NROS_PLATFORMS_DIR`, `NROS_MODEL_DIR`, `NROS_HOME`.
const KNOBS_THAT_CANNOT_CHANGE_A_SIZE: &[(&str, &str)] = &[
    // --- orchestration, measured to vary run-to-run or leaf-to-leaf ---
    (
        "NROS_FIXTURE_ID",
        "which fixture is being built; 11 values in one lane",
    ),
    (
        "NROS_WS_RECORDS_FILE",
        "TIMESTAMPED workspace records path; differs every run",
    ),
    ("NROS_JOBSERVER", "make jobserver handshake flag"),
    ("NROS_COMPILE_CHECK_POOL", "compile-check parallelism"),
    (
        "NROS_METADATA_PROBE_CACHE",
        "whether the metadata probe may reuse its cache",
    ),
    (
        "NROS_SKIP_VERSION_CHECK",
        "a check toggle, not an input to any type",
    ),
    (
        "NROS_CMAKE_EXTRA_DEFS",
        "cmake -D flags for the C/C++ lane; no Rust type reads them",
    ),
    ("NROS_CLI", "path to the built nros binary"),
    ("NROS_CLI_BIN", "path to the built nros binary"),
    ("NROS_REPO_ROOT", "the checkout root"),
    (
        "NROS_CARGO_FLAGS",
        "cargo CLI flags (`--locked`); affects resolution, not layout",
    ),
    ("NROS_REPO_DIR", "the checkout root"),
    // --- stable path plumbing: costs nothing on one host, splits across
    //     checkouts, CI images and $TMPDIR layouts ---
    (
        "NROS_C_INCLUDE",
        "header dir for the C lane; no Rust type reads it",
    ),
    (
        "NROS_CPP_INCLUDE",
        "header dir for the C++ lane; no Rust type reads it",
    ),
    ("NROS_PLATFORM_CFFI_INCLUDE", "platform C header dir"),
    (
        "NROS_PLATFORM_POSIX_SRC",
        "C source dir for the posix platform shim",
    ),
    (
        "NROS_PLATFORM_FREERTOS_SRC",
        "C source dir for the FreeRTOS platform shim",
    ),
    (
        "NROS_PLATFORM_THREADX_SRC",
        "C source dir for the ThreadX platform shim",
    ),
    ("NROS_LAN9118_LWIP_DIR", "vendored driver source dir"),
    ("NROS_VIRTIO_NET_NETX_DIR", "vendored driver source dir"),
    ("NROS_ESP_IDF_WORKSPACE", "ESP-IDF checkout location"),
    ("NROS_ESP_IDF_ENV_SHIM", "path to the ESP-IDF env script"),
    ("NROS_ZEPHYR_BUILD_ROOT", "where Zephyr builds go"),
    ("NROS_ZEPHYR_CCACHE_DIR", "ccache location"),
    ("NROS_ZEPHYR_CCACHE_TEMPDIR", "ccache scratch location"),
];

/// Whole FAMILIES that cannot change a probed size.
///
/// A prefix is used only where every member is generated by one producer and is
/// uniform by construction, so a future member needs no fresh argument. Listing
/// `NROS_KIND_*`'s ~20 members individually would be honest but would also mean
/// the list rots the next time the fixture driver gains a kind.
///
/// A prefix must never shadow one of the protected names above; a test asserts
/// it does not.
const KNOB_PREFIXES_THAT_CANNOT_CHANGE_A_SIZE: &[(&str, &str)] = &[
    (
        "NROS_KIND_",
        "per-build-kind marker strings from the fixture driver (`NROS_KIND_CARGO`, \
         `NROS_KIND_ZENOHD`, …) — each is the literal kind name, never a size",
    ),
    (
        "NROS_BUILD_",
        "build orchestration: job counts and TIMESTAMPED log dirs \
         (`NROS_BUILD_LOG_DIR` carries a timestamp and pid, so it differs every run)",
    ),
];

/// True when `name` is argued not to reach any probed size.
fn knob_is_excluded(name: &str) -> bool {
    KNOBS_THAT_CANNOT_CHANGE_A_SIZE
        .iter()
        .any(|(n, _)| *n == name)
        || KNOB_PREFIXES_THAT_CANNOT_CHANGE_A_SIZE
            .iter()
            .any(|(p, _)| name.starts_with(p))
}

fn knob_identity() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = env::vars()
        .filter(|(k, _)| k.starts_with("NROS_") && !k.starts_with("NROS_SIZES_"))
        // phase-353 W4 — drop the names and families argued above.
        // Everything else still keys.
        .filter(|(k, _)| !knob_is_excluded(k))
        .collect();
    // Zephyr's Kconfig reaches the crate through `$DOTCONFIG`, so it reaches the
    // SIZE too — and a leaf that sets a knob there sets nothing in the env.
    if let Some(dotconfig) = env::var_os("DOTCONFIG")
        && let Ok(text) = std::fs::read_to_string(&dotconfig)
    {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("CONFIG_NROS_")
                && let Some((name, value)) = rest.split_once('=')
            {
                out.push((format!("CONFIG_NROS_{name}"), value.to_string()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn rustc_version_slug() -> String {
    let rustc = env::var_os("CARGO_BUILD_RUSTC")
        .or_else(|| env::var_os("RUSTC"))
        .unwrap_or_else(|| "rustc".into());
    let output = Command::new(&rustc).arg("-V").output();
    let version = output
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_else(|| "unknown".to_string());
    // Sanitize: keep [A-Za-z0-9._-], replace others with '-'.
    let mut slug = String::with_capacity(version.len());
    for c in version.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            slug.push(c);
        } else {
            slug.push('-');
        }
    }
    if slug.is_empty() {
        slug.push_str("unknown");
    }
    slug
}

/// Phase 119.1: merge `new_values` against any matching `#define NAME N`
/// already present in `header_path`, taking the max of each pair. Returns
/// the merged map.
///
/// `header_prefix` is prepended to each `new_values` key when matching
/// against the header's `#define NAME N` lines — e.g. probed key
/// `EXECUTOR_SIZE` matches header define `NROS_EXECUTOR_SIZE` when
/// `header_prefix = "NROS_"`. Pass an empty string for an exact match.
///
/// Rationale: each consumer crate (`nros-c`, `nros-cpp`) writes its
/// generated header into the package source tree. Multiple cmake builds
/// (posix/zenoh, posix/xrce, freertos, threadx-riscv, ...) all run in
/// sequence against the same source tree and overwrite the header. The
/// installed library variants then have target-specific sizes that
/// don't match the last-write-wins header → memory corruption when the
/// C/C++ wrapper allocates opaque storage smaller than what the linked
/// Rust runtime actually writes.
///
/// Taking the max across all variants makes the shared header a safe
/// upper bound — every variant fits. Wastes a few bytes per executor
/// on variants whose actual size is smaller; correctness > frugality
/// for the public include path.
pub fn merge_header_max_values(
    header_path: &Path,
    header_prefix: &str,
    new_values: &HashMap<String, u64>,
) -> HashMap<String, u64> {
    let existing = read_header_defines(header_path).unwrap_or_default();
    let mut merged = new_values.clone();
    // Pull existing header values into the merged map (stripping the
    // header_prefix). Also covers the case where the current probe
    // returned an empty map (e.g. `cargo check --no-default-features`):
    // we still preserve the prior header values.
    for (full_key, &old_val) in &existing {
        let Some(key) = full_key.strip_prefix(header_prefix) else {
            continue;
        };
        let entry = merged.entry(key.to_string()).or_insert(0);
        if old_val > *entry {
            *entry = old_val;
        }
    }
    merged
}

/// Parse `#define NAME N` lines from a C header. Used by
/// [`merge_header_max_values`] to recover prior probe results before
/// overwriting the file.
pub fn read_header_defines(header_path: &Path) -> Result<HashMap<String, u64>, Error> {
    let text = std::fs::read_to_string(header_path)?;
    let mut out = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("#define") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let Some(value_str) = parts.next() else {
            continue;
        };
        let Ok(value) = value_str.parse::<u64>() else {
            continue;
        };
        out.insert(name.to_string(), value);
    }
    Ok(out)
}

/// Phase 119.3: derive a deterministic variant slug from the consumer
/// crate's active cargo features. Used by `nros-c`/`nros-cpp` build
/// scripts to scope per-build generated headers under
/// `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/<slug>/`.
///
/// The slug is the sorted, underscore-joined list of all features
/// (lowercase-with-dashes). Example with rmw-zenoh + platform-posix +
/// ros-humble + std:
///   `platform-posix_rmw-zenoh_ros-humble_std`
///
/// Sorting makes the slug independent of cargo's iteration order.
/// Returns `"default"` when no features are set (workspace-default
/// `cargo check`).
pub fn variant_slug_from_env() -> String {
    let mut features = forwarded_features();
    if features.is_empty() {
        return "default".to_string();
    }
    features.sort();
    features.join("_")
}

/// Collect feature names the consumer build script was invoked with.
///
/// Cargo exposes them as `CARGO_FEATURE_<NAME>=1` env vars with name
/// upper-cased and `-` replaced by `_`. Reverse the transform so the
/// nested invocation sees the original lowercase-with-dashes form.
/// Features of `dep` that the CALLER's active features enable — issue 0665.
///
/// Reads the caller's own `Cargo.toml` `[features]` table and, for every
/// feature cargo says is active (`CARGO_FEATURE_*`), collects the `dep/<feat>`
/// entries it lists. This is the half `forwarded_features()` cannot see: a
/// caller feature named `std` may enable `nros/env`, and no amount of comparing
/// names will reveal it.
///
/// Best-effort by design: an unreadable or unparsable manifest yields nothing
/// extra and the name-intersection behaviour stands, because under-forwarding
/// is what this fixes and a hard error here would break every probe that reads
/// a manifest shape this simple parser does not expect.
fn features_enabled_on_dep(manifest_dir: &str, dep: &str) -> Result<Vec<String>, Error> {
    let text = std::fs::read_to_string(Path::new(manifest_dir).join("Cargo.toml"))
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;

    let table = parse_features_table(&text);
    let prefix = format!("{dep}/");

    // Cargo feature unification is TRANSITIVE, so this walk has to be. A first
    // version collected only the `dep/<feat>` items written DIRECTLY on an
    // active row, which reads `std = ["alloc", "nros/env", ...]` correctly and
    // `std = ["alloc", "env", ...]` — where the caller's OWN `env` feature is
    // what enables `nros/env` — as forwarding nothing.
    //
    // That is not hypothetical either: phase-359 W10 rewrote `nros-c`'s row from
    // the first shape to the second, and the probe silently went back to
    // measuring an env-less `ExecutorInlineStorage` — 16 bytes short, the exact
    // regression issue 0665 exists to prevent, reintroduced by a refactor the
    // gate could not see. Same shape as the one-line-per-feature parser that
    // issue fixed: a parse that depends on how the author chose to spell an
    // equivalent feature set is not a parse.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: Vec<String> = forwarded_features();
    let mut out = Vec::new();
    while let Some(feat) = queue.pop() {
        if !seen.insert(feat.clone()) {
            continue;
        }
        let Some(items) = table.get(&feat) else {
            continue;
        };
        for item in items {
            match item.strip_prefix(&prefix) {
                Some(f) if !f.is_empty() => out.push(f.to_string()),
                // A bare name is one of THIS crate's own features; follow it.
                // Anything with a `/` belongs to some other dependency.
                _ if !item.contains('/') => queue.push(item.clone()),
                _ => {}
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Parse a `[features]` table into `feature -> items`.
///
/// issue 0665 — the original parser took one line per feature, so every
/// manifest that WRAPS its array was silently read as empty. That is not
/// hypothetical formatting: `nros-c`'s `std` happens to be one line and
/// `nros-cpp`'s is not, so the first fix worked for the crate that reported the
/// bug and did nothing for its sibling — which forwards `nros/env` and
/// `nros-node/env` from a wrapped array and carries the same
/// `CPP_EXECUTOR_OPAQUE_U64S` assert. A parse that depends on where somebody
/// put a newline is not a parse.
///
/// Best-effort by design: a shape this simple parser does not expect yields
/// nothing rather than an error, because under-forwarding is what the caller
/// fixes and a hard error here would break every probe.
fn parse_features_table(text: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut table: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut in_features = false;
    // The feature whose array is still open across lines, if any.
    let mut open_row: Option<String> = None;
    for line in text.lines() {
        // Strip a trailing comment; `#` never appears inside these values, and
        // `nros-cpp` puts several paragraphs of them INSIDE the array.
        let raw = match line.find('#') {
            Some(i) => &line[..i],
            None => line,
        };
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if open_row.is_none() && t.starts_with('[') && t.ends_with(']') && !t.contains('=') {
            // A table header (`[features]`, `[dependencies]`), not an array.
            in_features = t == "[features]";
            continue;
        }
        if !in_features {
            continue;
        }

        let (name, body) = match &open_row {
            // Continuation of a wrapped array.
            Some(name) => (name.clone(), t),
            None => {
                let Some((name, rest)) = t.split_once('=') else {
                    continue;
                };
                let name = name.trim().trim_matches('"').to_string();
                // An array is still open when the line has `[` and no closing `]`.
                if rest.contains('[') && !rest.contains(']') {
                    open_row = Some(name.clone());
                }
                (name, rest)
            }
        };

        let entry = table.entry(name).or_default();
        for item in body.split(&['[', ']', ',', '"'][..]) {
            let item = item.trim();
            if !item.is_empty() && item != "=" {
                entry.push(item.to_string());
            }
        }
        if open_row.is_some() && body.contains(']') {
            open_row = None;
        }
    }
    table
}

fn forwarded_features() -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in env::vars() {
        if v != "1" {
            continue;
        }
        let Some(rest) = k.strip_prefix("CARGO_FEATURE_") else {
            continue;
        };
        out.push(rest.to_ascii_lowercase().replace('_', "-"));
    }
    out
}

/// Extract the sizes of every defined symbol with the given prefix from an rlib.
///
/// An rlib is an `ar` archive of object files (plus a `lib.rmeta` metadata
/// member). This walks each object member, iterates its defined symbols, and
/// for every symbol whose name starts with `prefix`, records
/// `(name-without-prefix, ObjectSymbol::size())`.
///
/// The pattern used by the `nros` crate is:
///
/// ```ignore
/// #[used]
/// #[unsafe(no_mangle)]
/// pub static __NROS_SIZE_PUBLISHER: [u8; size_of::<RmwPublisher>()] = [0; _];
/// ```
///
/// Calling `extract_sizes(&rlib, "__NROS_SIZE_")` returns `{ "PUBLISHER" → N, ... }`.
pub fn extract_sizes(rlib: &Path, prefix: &str) -> Result<HashMap<String, u64>, Error> {
    let data = std::fs::read(rlib)?;
    let archive = ArchiveFile::parse(&*data)?;
    let mut out: HashMap<String, u64> = HashMap::new();
    let mut saw_bitcode = false;

    for member in archive.members() {
        let member = member?;
        let name_bytes = member.name();
        // Skip rmeta and non-object members. The rustc metadata lands in
        // `lib.rmeta`; some toolchains also include a `__.SYMDEF` bookkeeping
        // member. Anything that isn't a recognised object file is silently
        // skipped. GNU `ar` terminates member names with `/`, so the rmeta
        // member can arrive as `lib.rmeta/` — strip a trailing slash before
        // matching (234.4) so the skip actually fires.
        let bare_name = name_bytes.strip_suffix(b"/").unwrap_or(name_bytes);
        if bare_name == b"lib.rmeta" || bare_name.starts_with(b"__.SYMDEF") {
            continue;
        }

        let member_data = member.data(&*data)?;
        let object = match ObjectFile::parse(member_data) {
            Ok(o) => o,
            Err(_) => {
                // Fat LTO makes rustc emit LLVM bitcode instead of ELF/COFF
                // objects, in one of two wire forms `object` can't read:
                //   - raw bitcode, magic `BC\xC0\xDE` (`0x4243C0DE`);
                //   - the Darwin "bitcode wrapper", magic `0x0B17C0DE` stored
                //     little-endian as `\xDE\xC0\x17\x0B` (macOS hosts — 234.4).
                // Flag either for the v0-name fallback below.
                if member_data.starts_with(b"BC\xC0\xDE")
                    || member_data.starts_with(b"\xDE\xC0\x17\x0B")
                {
                    saw_bitcode = true;
                }
                continue;
            }
        };

        for symbol in object.symbols() {
            if !symbol.is_definition() {
                continue;
            }
            let Ok(name) = symbol.name() else { continue };
            if let Some(rest) = name.strip_prefix(prefix) {
                // Several object members may define weak copies of the same
                // static; keep the first non-zero size observed.
                let size = symbol.size();
                if size == 0 {
                    continue;
                }
                out.entry(rest.to_string()).or_insert(size);
            }
        }
    }

    // Phase 77.25: if nothing came out of the ELF path and the rlib
    // contains bitcode members, fall back to rustc's bundled `llvm-nm`
    // which *can* read bitcode symbol names. The nros sizes module
    // emits `__nros_size_NAME<const N: usize>` monomorphisations —
    // their v0-mangled symbol names contain both the NAME and the
    // const-generic value N (the `size_of::<T>()` result). A single
    // regex captures `NAME` and `N` from the demangled output.
    if out.is_empty()
        && saw_bitcode
        && let Ok(from_bitcode) = extract_sizes_via_llvm_nm(rlib)
    {
        return Ok(from_bitcode);
    }

    Ok(out)
}

/// Phase 77.25: bitcode-aware extraction via `rustc`-bundled `llvm-nm`.
///
/// Invokes `$(rustc --print sysroot)/lib/rustlib/$TRIPLE/bin/llvm-nm
/// --demangle` against the rlib, then regex-matches lines like
/// `nros::sizes::rmw_sizes::__nros_size_PUBLISHER_SIZE::<48>` — the
/// capture groups are the NAME and the const-generic SIZE value.
/// Returns an empty map on any failure (probe consumers treat that
/// the same as a probe miss — 77.24's stopgap covers it).
fn extract_sizes_via_llvm_nm(rlib: &Path) -> Result<HashMap<String, u64>, Error> {
    let sysroot = rustc_sysroot()?;
    let triple = rustc_host_triple()?;
    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    let llvm_nm = sysroot
        .join("lib/rustlib")
        .join(&triple)
        .join("bin")
        .join(format!("llvm-nm{exe_suffix}"));
    if !llvm_nm.exists() {
        return Err(Error::CargoMetadata(format!(
            "llvm-nm not found at {}",
            llvm_nm.display()
        )));
    }

    let output = Command::new(&llvm_nm)
        .arg("--demangle")
        .arg(rlib)
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);

    // Match `::__nros_size_<NAME>::<<SIZE>>` near the end of a line.
    // Example: "nros::sizes::rmw_sizes::__nros_size_PUBLISHER_SIZE::<48>"
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(marker_idx) = line.find("::__nros_size_") else {
            continue;
        };
        let after = &line[marker_idx + "::__nros_size_".len()..];
        // `after` now looks like "PUBLISHER_SIZE::<48>"
        let Some((name, tail)) = after.split_once("::<") else {
            continue;
        };
        let Some(num_str) = tail.strip_suffix('>') else {
            continue;
        };
        let Ok(size) = num_str.trim().parse::<u64>() else {
            continue;
        };
        out.entry(name.to_string()).or_insert(size);
    }
    Ok(out)
}

fn rustc_sysroot() -> Result<PathBuf, Error> {
    let output = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--print", "sysroot"])
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

/// Resolve the rustc *host* triple (the triple of the toolchain itself).
///
/// `llvm-nm` is bundled at `<sysroot>/lib/rustlib/<host>/bin/`; cross-target
/// directories don't carry the host tools, so this must return the host
/// triple, not the build's `TARGET`. Phase 118.E fixes the prior behavior
/// which preferred `TARGET` and broke on cross-builds.
fn rustc_host_triple() -> Result<String, Error> {
    let output = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("-vV")
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(rest) = line.strip_prefix("host: ") {
            return Ok(rest.trim().to_string());
        }
    }
    // Last-resort fallback: cargo's `HOST` env (set in build scripts).
    if let Ok(t) = env::var("HOST") {
        return Ok(t);
    }
    Err(Error::CargoMetadata(
        "could not determine rustc host triple".into(),
    ))
}

/// Resolve the workspace target directory for the filesystem-fallback path.
///
/// Order: `CARGO_TARGET_DIR` env override → walk `OUT_DIR` for the
/// `<target>/[triple]/<profile>/build/` ancestor → `cargo metadata`.
/// The real target *profile directory* name (e.g. `nros-fast-release`),
/// derived from `OUT_DIR`. Issue 0111: the `PROFILE` env var only ever reports
/// `debug`/`release`, so it cannot name a custom profile's artifact dir.
/// `OUT_DIR` is `<target>/<triple>?/<profile-dir>/build/<pkg>-<hash>/out`, so
/// the path component immediately before `build` is the profile dir. Returns
/// `None` when `OUT_DIR` is unset or has no `build` ancestor.
fn profile_dir_name() -> Option<String> {
    let out = PathBuf::from(env::var("OUT_DIR").ok()?);
    let mut p = out.as_path();
    while let Some(parent) = p.parent() {
        if parent.file_name().and_then(|s| s.to_str()) == Some("build") {
            return parent
                .parent()
                .and_then(|d| d.file_name())
                .and_then(|s| s.to_str())
                .map(String::from);
        }
        p = parent;
    }
    None
}

pub fn cargo_target_dir() -> Result<PathBuf, Error> {
    if let Ok(dir) = env::var("CARGO_TARGET_DIR")
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }

    if let Ok(out) = env::var("OUT_DIR") {
        let out = PathBuf::from(out);
        let mut p = out.as_path();
        while let Some(parent) = p.parent() {
            if parent.file_name().and_then(|s| s.to_str()) == Some("build")
                && let Some(profile_dir) = parent.parent()
                && let Some(triple_or_target) = profile_dir.parent()
                && let Some(name) = triple_or_target.file_name().and_then(|s| s.to_str())
            {
                if name.contains('-') {
                    if let Some(target) = triple_or_target.parent() {
                        return Ok(target.to_path_buf());
                    }
                } else {
                    return Ok(triple_or_target.to_path_buf());
                }
            }
            p = parent;
        }
    }

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| Error::MalformedMetadata("CARGO_MANIFEST_DIR"))?;
    let output = Command::new(&cargo)
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--no-deps")
        .current_dir(&manifest_dir)
        .output()
        .map_err(|e| Error::CargoMetadata(e.to_string()))?;
    if !output.status.success() {
        return Err(Error::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let meta: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::CargoMetadata(format!("invalid JSON: {e}")))?;
    meta.get("target_directory")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or(Error::MalformedMetadata("target_directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test mutates process-global env state, so they share a mutex to
    /// avoid clobbering each other under `cargo test`'s default parallelism.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// issue 0665 — the WIRING, not just the parser. Removing the `extra`
    /// merge must fail a test; the first version of this fix only exercised
    /// `features_enabled_on_dep` directly, so deleting its call site left
    /// everything green.
    #[test]
    fn merge_keeps_dep_features_the_caller_enables_under_another_name() {
        let declared: std::collections::HashSet<String> = ["std", "env", "alloc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let got = merge_forwarded(vec!["std".to_string()], vec!["env".to_string()], &declared);
        assert_eq!(got, vec!["env".to_string(), "std".to_string()]);

        // Undeclared names are dropped — `cargo build --features <unknown>` errors.
        let got = merge_forwarded(
            vec!["std".to_string()],
            vec!["never-declared".to_string()],
            &declared,
        );
        assert_eq!(got, vec!["std".to_string()]);
    }

    /// issue 0665 — a WRAPPED feature array must be read like any other.
    ///
    /// The first parser took one line per feature. `nros-c`'s `std` is one line,
    /// so the fix worked for the crate that reported the bug; `nros-cpp`'s is
    /// wrapped, so for its sibling the fix did nothing at all — and `nros-cpp`
    /// forwards `nros/env` + `nros-node/env` from that wrapped array and carries
    /// the same `CPP_EXECUTOR_OPAQUE_U64S` assert. The shape below is
    /// `nros-cpp`'s, comments and all.
    #[test]
    fn a_wrapped_feature_array_is_read_like_a_single_line_one() {
        let _guard = env_lock();
        let dir = std::env::temp_dir().join(format!("nros-probe-wrap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"caller\"\n\n[features]\n\
             default = [\"panic-spin\"]\n\
             std = [\n\
             \x20   \"alloc\",\n\
             \x20   # phase-359 W10 — `env` rides with `std` HERE, and only here.\n\
             \x20   #\n\
             \x20   # A paragraph of prose INSIDE the array, as the real one has.\n\
             \x20   \"nros/env\",\n\
             \x20   \"nros-node/env\",\n\
             \x20   \"nros/std\",\n\
             ]\n\
             alloc = [\"nros/alloc\"]\n\
             unused = [\n    \"nros/never-forwarded\",\n]\n",
        )
        .unwrap();

        unsafe {
            std::env::set_var("CARGO_FEATURE_STD", "1");
            std::env::remove_var("CARGO_FEATURE_ALLOC");
            std::env::remove_var("CARGO_FEATURE_UNUSED");
        }
        let got = features_enabled_on_dep(dir.to_str().unwrap(), "nros").unwrap();
        unsafe { std::env::remove_var("CARGO_FEATURE_STD") };
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            got.contains(&"env".to_string()),
            "`nros/env` sits on its own line inside a wrapped array — got {got:?}"
        );
        assert!(got.contains(&"std".to_string()), "got {got:?}");
        assert!(
            !got.contains(&"never-forwarded".to_string()),
            "an INACTIVE feature's wrapped array must not be read either — got {got:?}"
        );
    }

    /// issue 0665 — a caller feature that enables another of the caller's OWN
    /// features, which is what enables the `dep/<feat>` entry.
    ///
    /// This is `nros-c`'s shape after phase-359 W10 rewrote
    /// `std = ["alloc", "nros/env", ...]` into `std = ["alloc", "env", ...]`
    /// with `env = ["nros/env", ...]` beside it. Cargo unifies features
    /// transitively, so the two spellings enable exactly the same set; a
    /// one-level walk reads the first as forwarding `nros/env` and the second as
    /// forwarding nothing, and the probe silently went back to measuring an
    /// env-less `ExecutorInlineStorage` — 16 bytes short, this issue's own
    /// regression, reintroduced by a refactor the gate could not see.
    #[test]
    fn a_feature_reached_through_another_of_the_callers_own_features_is_forwarded() {
        let _guard = env_lock();
        let dir = std::env::temp_dir().join(format!("nros-probe-indirect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"caller\"\n\n[features]\n\
             env = [\"nros/env\", \"nros-node/env\"]\n\
             std = [\"alloc\", \"env\", \"nros/std\"]\n\
             alloc = [\"nros/alloc\"]\n\
             unused = [\"never = [\", \"nros/never-forwarded\"]\n",
        )
        .unwrap();

        unsafe {
            std::env::set_var("CARGO_FEATURE_STD", "1");
            std::env::remove_var("CARGO_FEATURE_ALLOC");
            std::env::remove_var("CARGO_FEATURE_UNUSED");
        }
        let got = features_enabled_on_dep(dir.to_str().unwrap(), "nros").unwrap();
        unsafe { std::env::remove_var("CARGO_FEATURE_STD") };
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            got.contains(&"env".to_string()),
            "`std` enables the caller's own `env`, which enables `nros/env` — got {got:?}"
        );
        assert!(got.contains(&"std".to_string()), "got {got:?}");
        assert!(
            got.contains(&"alloc".to_string()),
            "`std` enables `alloc`, which enables `nros/alloc` — got {got:?}"
        );
        assert!(
            !got.contains(&"never-forwarded".to_string()),
            "an INACTIVE feature must not be reached — got {got:?}"
        );
    }

    /// issue 0665 — the same rule against the REAL manifests, not a fixture.
    ///
    /// The two crates that carry an `*_EXECUTOR_OPAQUE_U64S` assert both forward
    /// `nros/env` from their `std`, one on a single line and one wrapped. A
    /// synthetic fixture cannot notice when somebody reformats a manifest or
    /// adds a forward; reading the files can. If this test ever has to be
    /// relaxed, the parser is wrong — not the manifest.
    #[test]
    fn the_real_c_and_cpp_manifests_forward_env_to_the_probed_facade() {
        let _guard = env_lock();
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root above packages/tooling/nros-sizes-build");

        unsafe {
            std::env::set_var("CARGO_FEATURE_STD", "1");
            std::env::set_var("CARGO_FEATURE_ALLOC", "1");
        }
        let mut results = Vec::new();
        for krate in ["nros-c", "nros-cpp"] {
            let dir = repo.join("packages/api").join(krate);
            if !dir.join("Cargo.toml").is_file() {
                continue;
            }
            let got = features_enabled_on_dep(dir.to_str().unwrap(), "nros").unwrap_or_default();
            results.push((krate, got));
        }
        unsafe {
            std::env::remove_var("CARGO_FEATURE_STD");
            std::env::remove_var("CARGO_FEATURE_ALLOC");
        }

        assert_eq!(
            results.len(),
            2,
            "both API crates must be present: {results:?}"
        );
        for (krate, got) in results {
            assert!(
                got.contains(&"env".to_string()),
                "{krate}: `std` forwards `nros/env`, so the probe must build the \
                 facade WITH it — otherwise `ExecutorInlineStorage` is measured 16 \
                 bytes short and the emitted opaque size with it (issue 0665). \
                 got {got:?}"
            );
        }
    }

    /// issue 0665 — a caller feature that enables a DIFFERENTLY-NAMED feature of
    /// the probed crate must be forwarded. `nros-c`'s `std` enables `nros/env`,
    /// and name-intersection alone forwarded only `std`, so the probe measured
    /// an env-less `ExecutorInlineStorage`: 16 bytes short of the linked one,
    /// and `EXECUTOR_OPAQUE_U64S` short with it.
    #[test]
    fn dep_features_enabled_by_an_active_caller_feature_are_collected() {
        let _guard = env_lock();
        let dir = std::env::temp_dir().join(format!("nros-probe-feat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"caller\"\n\n[features]\n\
             std = [\"alloc\", \"nros/std\", \"nros/env\", \"nros-node/std\"]\n\
             alloc = [\"nros/alloc\"]\n\
             unused = [\"nros/never-forwarded\"]\n",
        )
        .unwrap();

        // Only `std` is active, so the `alloc` and `unused` rows must not be read.
        unsafe {
            std::env::set_var("CARGO_FEATURE_STD", "1");
            std::env::remove_var("CARGO_FEATURE_ALLOC");
            std::env::remove_var("CARGO_FEATURE_UNUSED");
        }
        let got = features_enabled_on_dep(dir.to_str().unwrap(), "nros").unwrap();
        unsafe { std::env::remove_var("CARGO_FEATURE_STD") };
        std::fs::remove_dir_all(&dir).ok();

        assert!(got.contains(&"std".to_string()), "got {got:?}");
        assert!(
            got.contains(&"env".to_string()),
            "the whole point: `env` is enabled BY `std` under another name — got {got:?}"
        );
        assert!(
            !got.contains(&"never-forwarded".to_string()),
            "an inactive caller feature's row must not be read — got {got:?}"
        );
    }

    /// Clear every knob the key reads, so a test states its own inputs rather
    /// than inheriting the developer's shell. Returns what was removed.
    fn clear_knob_env() -> Vec<(String, String)> {
        let prior: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| k.starts_with("NROS_") || k == "DOTCONFIG")
            .collect();
        for (k, _) in &prior {
            unsafe { env::remove_var(k) }
        }
        prior
    }

    fn restore_knob_env(prior: Vec<(String, String)>) {
        for (k, _) in env::vars().filter(|(k, _)| k.starts_with("NROS_") || k == "DOTCONFIG") {
            unsafe { env::remove_var(&k) }
        }
        for (k, v) in prior {
            unsafe { env::set_var(k, v) }
        }
    }

    /// Issue 0563 / phase-400 W5 — both depfile layouts, and the absent case.
    ///
    /// The bug this pins was a lookup that tried ONE of the two spellings. It
    /// found the depfile for an uplifted rlib and missed it for the `deps/` copy,
    /// so the same crate's watch list was complete in some builds and EMPTY in
    /// others — silently, because the miss was an early `return`.
    #[test]
    fn probe_depfile_found_beside_and_uplifted() {
        let dir = std::env::temp_dir().join(format!("nros-0563-{}", std::process::id()));
        let deps = dir.join("release").join("deps");
        std::fs::create_dir_all(&deps).expect("tmp dirs");

        // 1. uplifted layout — depfile sits beside the artifact.
        let uplifted = dir.join("release").join("libnros.rlib");
        let uplifted_d = dir.join("release").join("libnros.d");
        std::fs::write(&uplifted, b"").unwrap();
        std::fs::write(&uplifted_d, b"x: y").unwrap();
        assert_eq!(
            probe_depfile(&uplifted).as_deref(),
            Some(uplifted_d.as_path())
        );

        // 2. `deps/` layout — cargo writes NO depfile here; it belongs to the
        //    uplifted sibling one directory up, named for the crate not the unit.
        let hashed = deps.join("libnros-0a605463647b4af3.rlib");
        std::fs::write(&hashed, b"").unwrap();
        assert!(
            !hashed.with_extension("d").exists(),
            "test premise: cargo does not write a depfile beside the deps/ copy"
        );
        assert_eq!(
            probe_depfile(&hashed).as_deref(),
            Some(uplifted_d.as_path()),
            "a deps/ rlib must resolve to the uplifted depfile — missing this is \
             what made the watch list empty in some builds"
        );

        // 3. genuinely absent -> None, so the caller can fail loudly rather than
        //    emit an empty watch list.
        std::fs::remove_file(&uplifted_d).unwrap();
        assert!(probe_depfile(&hashed).is_none());
        assert!(probe_depfile(&uplifted).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Issue 0528, as a test rather than a memory.
    ///
    /// Two Zephyr leaves at the SAME (target, features) that disagree about
    /// `CONFIG_NROS_EXECUTOR_MAX_CBS` must not share a probe directory. They did,
    /// and the result was order-dependent corruption: whichever probed first
    /// wrote `EXECUTOR_SIZE` for its own MAX_CBS, and in one order the 16-CBS
    /// leaf then compiled against a constant sized for 4 and died on
    /// `EXECUTOR_OPAQUE_U64S too small`. It survived a clean rebuild of the
    /// failing leaf because the poisoned state was in the SHARED dir.
    ///
    /// This is the reproduction any narrowing of `knob_identity()` must keep
    /// passing (phase-353 W4).
    #[test]
    fn zephyr_dotconfig_sizing_knob_splits_the_probe_key() {
        let _g = env_lock();
        let prior = clear_knob_env();

        let dir = std::env::temp_dir().join(format!("nros-0528-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let four = dir.join("dotconfig-4");
        let sixteen = dir.join("dotconfig-16");
        // Only the MAX_CBS line differs; everything else is identical, so a
        // difference in the key can come from nothing else.
        std::fs::write(&four, "CONFIG_NROS_EXECUTOR_MAX_CBS=4\nCONFIG_OTHER=1\n").unwrap();
        std::fs::write(
            &sixteen,
            "CONFIG_NROS_EXECUTOR_MAX_CBS=16\nCONFIG_OTHER=1\n",
        )
        .unwrap();

        unsafe { env::set_var("DOTCONFIG", &four) }
        let key_4 = probe_key("thumbv7m-none-eabi", &["rmw-zenoh".to_string()]);
        unsafe { env::set_var("DOTCONFIG", &sixteen) }
        let key_16 = probe_key("thumbv7m-none-eabi", &["rmw-zenoh".to_string()]);

        assert_ne!(
            key_4, key_16,
            "a 4-CBS and a 16-CBS Zephyr leaf landed on the SAME probe key — \
             issue 0528 is back, and it fails only in one build ORDER"
        );

        let _ = std::fs::remove_dir_all(&dir);
        restore_knob_env(prior);
    }

    /// The env route to the same knob (issue 0460: the crate resolves these from
    /// the env OR from `$DOTCONFIG`, so the key must watch both).
    #[test]
    fn env_sizing_knob_splits_the_probe_key() {
        let _g = env_lock();
        let prior = clear_knob_env();

        let bare = probe_key("x86_64-unknown-linux-gnu", &[]);
        unsafe { env::set_var("NROS_EXECUTOR_MAX_CBS", "16") }
        let with_knob = probe_key("x86_64-unknown-linux-gnu", &[]);
        assert_ne!(
            bare, with_knob,
            "setting NROS_EXECUTOR_MAX_CBS did not change the probe key"
        );

        restore_knob_env(prior);
    }

    /// phase-353 W4 — the narrowing, in the direction that saves disk.
    ///
    /// A knob argued in `KNOBS_THAT_CANNOT_CHANGE_A_SIZE` must NOT split the
    /// key, and must not split it at ANY value: the whole failure was that two
    /// checkouts, or one host's `/tmp` layout versus another's, produced
    /// different probe directories for byte-identical artifacts.
    #[test]
    fn path_plumbing_does_not_split_the_probe_key() {
        let _g = env_lock();
        let prior = clear_knob_env();

        let baseline = probe_key("x86_64-unknown-linux-gnu", &["rmw-zenoh".into()]);
        for (name, _reason) in KNOBS_THAT_CANNOT_CHANGE_A_SIZE {
            unsafe { env::set_var(name, "/some/where/entirely/different") }
            assert_eq!(
                baseline,
                probe_key("x86_64-unknown-linux-gnu", &["rmw-zenoh".into()]),
                "{name} split the probe key — it is argued to be plumbing that \
                 cannot change a size, so either the argument or this list is wrong"
            );
            unsafe { env::remove_var(name) }
        }

        restore_knob_env(prior);
    }

    /// The other half, and the one that keeps issue 0528 shut: a knob NOT on the
    /// list still keys the probe, whatever it is. The default for an unknown
    /// knob stays "split", so forgetting to add a name to the list costs a
    /// wasted directory rather than corruption.
    #[test]
    fn an_unlisted_knob_still_splits_the_probe_key() {
        let _g = env_lock();
        let prior = clear_knob_env();

        let baseline = probe_key("x86_64-unknown-linux-gnu", &[]);
        unsafe { env::set_var("NROS_SOME_KNOB_INVENTED_BY_THIS_TEST", "7") }
        assert_ne!(
            baseline,
            probe_key("x86_64-unknown-linux-gnu", &[]),
            "an unknown NROS_* knob did not key the probe — that is issue 0528's \
             default and it must not depend on the knob being recognised"
        );
        unsafe { env::remove_var("NROS_SOME_KNOB_INVENTED_BY_THIS_TEST") }

        // And a REAL sizing knob, to state the case that actually broke.
        unsafe { env::set_var("NROS_EXECUTOR_ARENA_SIZE", "65536") }
        assert_ne!(
            baseline,
            probe_key("x86_64-unknown-linux-gnu", &[]),
            "NROS_EXECUTOR_ARENA_SIZE did not key the probe"
        );

        restore_knob_env(prior);
    }

    /// Every exclusion carries an argument, and none shadows a protected name.
    ///
    /// The list's whole claim is that each entry was argued. An empty reason is
    /// an exclusion nobody justified. And the four protected names each point at
    /// a FILE whose CONTENT carries sizing knobs, so excluding one — directly or
    /// via a prefix — is issue 0528 by a new route.
    #[test]
    fn every_excluded_knob_carries_an_argument() {
        const PROTECTED: &[&str] = &[
            "NROS_BOARD_TOML",
            "NROS_PLATFORMS_DIR",
            "NROS_MODEL_DIR",
            "NROS_HOME",
        ];

        for (name, reason) in KNOBS_THAT_CANNOT_CHANGE_A_SIZE {
            assert!(name.starts_with("NROS_"), "{name} is not an NROS_* name");
            assert!(reason.len() > 10, "{name} has no stated reason");
        }
        for (prefix, reason) in KNOB_PREFIXES_THAT_CANNOT_CHANGE_A_SIZE {
            assert!(
                prefix.starts_with("NROS_"),
                "{prefix} is not an NROS_* prefix"
            );
            assert!(reason.len() > 10, "{prefix} has no stated reason");
        }
        for p in PROTECTED {
            assert!(
                !knob_is_excluded(p),
                "{p} is excluded from the probe key, but it names a file whose \
                 CONTENT carries sizing knobs — that is issue 0528 by a new route"
            );
        }
    }

    /// The knobs the 2026-08-15 census caught splitting the key run-to-run.
    ///
    /// `NROS_BUILD_LOG_DIR` and `NROS_WS_RECORDS_FILE` carry a timestamp and a
    /// pid, so before this list every fixture build minted probe keys that could
    /// never be reused. That is what grew the directory to 110 sub-keys / 37 GB
    /// while one lane creates 25.
    #[test]
    fn run_scoped_orchestration_does_not_split_the_probe_key() {
        let _g = env_lock();
        let prior = clear_knob_env();

        let baseline = probe_key("x86_64-unknown-linux-gnu", &["rmw-cffi".into()]);
        let measured: &[(&str, &str)] = &[
            (
                "NROS_BUILD_LOG_DIR",
                "/repo/build/logs/20260815-111859-1157807-9133",
            ),
            ("NROS_BUILD_JOBS", "24"),
            (
                "NROS_WS_RECORDS_FILE",
                "/repo/build/ws-linux-20260815-112230-group-10.records",
            ),
            ("NROS_FIXTURE_ID", "c_mixed_workspace"),
            ("NROS_KIND_CARGO", "cargo"),
            ("NROS_KIND_ZENOHD", "zenohd"),
            ("NROS_JOBSERVER", "1"),
        ];
        for (name, value) in measured {
            unsafe { env::set_var(name, value) }
        }
        assert_eq!(
            baseline,
            probe_key("x86_64-unknown-linux-gnu", &["rmw-cffi".into()]),
            "run-scoped orchestration still splits the probe key — the directory \
             will keep growing one key per run"
        );
        for (name, _) in measured {
            unsafe { env::remove_var(name) }
        }

        restore_knob_env(prior);
    }

    /// The CONTROL, and the reason the two tests above are worth anything.
    ///
    /// A key that split on everything would satisfy them and be useless — it is
    /// what produces the 110 directories holding 18 identities that phase-353 W4
    /// exists to reduce. So: identical inputs must COLLIDE, and feature ORDER
    /// must not split.
    #[test]
    fn identical_inputs_share_a_probe_key() {
        let _g = env_lock();
        let prior = clear_knob_env();

        let a = probe_key(
            "x86_64-unknown-linux-gnu",
            &["rmw-zenoh".into(), "std".into()],
        );
        let b = probe_key(
            "x86_64-unknown-linux-gnu",
            &["rmw-zenoh".into(), "std".into()],
        );
        assert_eq!(a, b, "the same inputs produced two different probe keys");

        // Sorted internally, so order is not an input.
        let reordered = probe_key(
            "x86_64-unknown-linux-gnu",
            &["std".into(), "rmw-zenoh".into()],
        );
        assert_eq!(
            a, reordered,
            "feature ORDER split the probe key — that is a directory per spelling"
        );

        // A knob that is not set at all must not be conjured into the key.
        let c = probe_key(
            "x86_64-unknown-linux-gnu",
            &["rmw-zenoh".into(), "std".into()],
        );
        assert_eq!(
            a, c,
            "the key is not stable across calls with identical inputs"
        );

        restore_knob_env(prior);
    }

    #[test]
    fn profile_dir_name_reads_custom_profile_from_out_dir() {
        let _g = env_lock();
        let prior = env::var_os("OUT_DIR");
        // Issue 0111: a custom-profile cross-compiled OUT_DIR. The profile dir
        // (`nros-fast-release`) must win over what `PROFILE` (`release`) reports.
        unsafe {
            env::set_var(
                "OUT_DIR",
                "/w/target/thumbv7m-none-eabi/nros-fast-release/build/nros-cpp-abc123/out",
            );
        }
        assert_eq!(profile_dir_name().as_deref(), Some("nros-fast-release"));
        // Host build (no triple component): still finds the profile dir.
        unsafe {
            // profile-literal-ok: dir vocabulary: test data for profile_dir_name()
            env::set_var("OUT_DIR", "/w/target/release/build/nros-deadbeef/out");
        }
        assert_eq!(profile_dir_name().as_deref(), Some("release"));
        // No `build` ancestor → None.
        unsafe {
            env::set_var("OUT_DIR", "/nowhere/at/all");
        }
        assert_eq!(profile_dir_name(), None);
        unsafe {
            match prior {
                Some(v) => env::set_var("OUT_DIR", v),
                None => env::remove_var("OUT_DIR"),
            }
        }
    }

    #[test]
    fn forwarded_features_reverses_cargo_feature_transform() {
        let _g = env_lock();
        // Clear any pre-existing CARGO_FEATURE_* the test runner may have set.
        let prior: Vec<String> = env::vars()
            .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
            .map(|(k, _)| k)
            .collect();
        for k in &prior {
            unsafe {
                env::remove_var(k);
            }
        }
        unsafe {
            env::set_var("CARGO_FEATURE_RMW_ZENOH", "1");
            env::set_var("CARGO_FEATURE_PLATFORM_POSIX", "1");
            env::set_var("CARGO_FEATURE_SOMETHING_ELSE", "0"); // value != "1" → filtered
        }
        let mut got = forwarded_features();
        got.sort();
        assert_eq!(got, vec!["platform-posix", "rmw-zenoh"]);
        unsafe {
            env::remove_var("CARGO_FEATURE_RMW_ZENOH");
            env::remove_var("CARGO_FEATURE_PLATFORM_POSIX");
            env::remove_var("CARGO_FEATURE_SOMETHING_ELSE");
        }
    }
}
