//! Build-script helper: resolves repo-relative paths used by every
//! `build.rs` and board crate, without depending on `just`/`.envrc`.
//!
//! Phase 208.B Track A — every panic site of the form
//! `env::var("NROS_PLATFORM_<X>").expect("... direnv allow, or build via just")`
//! becomes a call to a resolver here. The env var stays valid as a
//! user-supplied override (out-of-tree consumers, custom layouts);
//! the in-tree case stops requiring it.
//!
//! The repo root is found by walking up from `CARGO_MANIFEST_DIR`
//! until `nros-sdk-index.toml` is seen (the Phase 195 sentinel). All
//! sub-paths mirror `just/sdk-env.just` — that file is the SSoT for
//! the relative-path values; if a path moves, fix it there AND here.

#![forbid(unsafe_code)]

use std::path::PathBuf;

/// Walk up from `CARGO_MANIFEST_DIR` until `nros-sdk-index.toml` is
/// found. Panics if no such ancestor exists (out-of-tree consumer
/// without a vendored nano-ros checkout — they must set the relevant
/// env vars themselves).
pub fn repo_root() -> PathBuf {
    let start = std::env::var("CARGO_MANIFEST_DIR").expect(
        "nros-build-paths: CARGO_MANIFEST_DIR not set (must be called from a build script)",
    );
    try_repo_root().unwrap_or_else(|| {
        panic!(
            "nros-build-paths: could not locate nros-sdk-index.toml walking up from {start}. \
             Out-of-tree consumer? Set the relevant NROS_PLATFORM_* env vars explicitly."
        )
    })
}

/// [`repo_root`] for callers that have a legitimate out-of-tree fallback.
///
/// phase-343 I1 — the sizes probe needs the repo root to place its SHARED
/// cache, but must still work for an out-of-tree consumer that has no nano-ros
/// checkout to find. Panicking there would be wrong, and re-implementing the
/// walk in the caller would be a second spelling of "where is the repo" — the
/// R3 drift this repo keeps paying for (a private `project_root()` in
/// `qemu.rs` was deleted for exactly this reason).
pub fn try_repo_root() -> Option<PathBuf> {
    let start = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut dir = PathBuf::from(start);
    loop {
        if dir.join("nros-sdk-index.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The file whose presence marks a nano-ros source tree.
///
/// One string, three spellings, because the three build systems cannot call
/// each other — the same reason [`riscv64`] carries a shell and a cmake twin.
/// The others are `nros_launcher::checkout::MONOREPO_MARKER` (the `packages/cli`
/// workspace, which this crate may not depend on: it sits BELOW it) and
/// `NROS_CHECKOUT_MARKER` in `scripts/lib/checkout-paths.sh`.
/// `check-inherited-checkout-paths.py` pins the three to each other.
pub const CHECKOUT_MARKER: &str = "packages/core/nros-core/Cargo.toml";

/// Which nano-ros checkout does `path` belong to? `None` when it belongs to
/// none — which is the answer that keeps a real out-of-tree SDK working.
///
/// Purely LEXICAL: the path need not exist (an unprovisioned SDK dir is still
/// attributable to a checkout) and may name a file rather than a directory. A
/// relative path answers `None` on purpose — it is resolved against the
/// caller's own cwd, so it cannot have been inherited from another checkout.
///
/// Not `.git`: a linked worktree's `.git` is a FILE, not a directory (issue
/// 1336), and `git rev-parse` answers about the caller's repository rather than
/// about an arbitrary path.
#[must_use]
pub fn checkout_root_of(path: &std::path::Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut cur = Some(path);
    while let Some(dir) = cur {
        if dir.join(CHECKOUT_MARKER).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// The issue-1280 rule, as a pure function: an inherited absolute path that
/// belongs to a nano-ros checkout OTHER than `here` is re-rooted onto `here`;
/// everything else is returned unchanged.
///
/// ## Why this outranks "the environment said so"
///
/// Every resolver here is ENV-FIRST, and that order is what makes an
/// out-of-tree SDK usable. It is also how a build in a LINKED GIT WORKTREE
/// compiles the other checkout's sources: a worktree inherits its parent
/// shell's environment, and every SDK path in it is an ABSOLUTE path rooted at
/// the checkout that shell activated. A worktree exists to test a change, so an
/// edit to `packages/platform/nros-platform-freertos/src` here was not what got
/// compiled — the build read the main checkout's copy, linked, passed, and
/// reported the change as verified. CLAUDE.md prescribes worktrees for parallel
/// sessions, so that is the default shape of agent work.
///
/// Both trees are nano-ros checkouts, so the relative sub-path is identical by
/// construction and re-rooting is exact. The three outcomes are the whole rule:
///
/// * outside any checkout → KEEP (a real out-of-tree SDK — what env-first is for)
/// * inside `here` → KEEP (nothing to decide)
/// * inside a different checkout → RE-ROOT, and say so with `cargo::warning`
///
/// Emits no rerun directive and changes no fingerprint (issue 0491): what a
/// build script depends on is the CONTENT it reads, and the caller declares
/// that with [`watch_path`].
#[must_use]
pub fn reroot_foreign(value: &std::path::Path, here: &std::path::Path) -> PathBuf {
    let Some(owner) = checkout_root_of(value) else {
        return value.to_path_buf();
    };
    // Resolve both sides: a checkout reached through a symlinked parent is the
    // same tree under a second name, and comparing spellings would call it
    // foreign (issue 0375's two-names-for-one-tree).
    let owner_real = owner.canonicalize().unwrap_or_else(|_| owner.clone());
    let here_real = here.canonicalize().unwrap_or_else(|_| here.to_path_buf());
    if owner_real == here_real {
        return value.to_path_buf();
    }
    let Ok(rel) = value.strip_prefix(&owner) else {
        return value.to_path_buf();
    };
    here_real.join(rel)
}

/// [`reroot_foreign`] against `try_repo_root()`, announcing any rewrite.
///
/// The announcement is a `cargo::warning` and it fires ONLY in the broken case,
/// so it costs a correct build nothing — and the alternative is the silence
/// that let a worktree certify the wrong tree for a whole phase.
fn reroot_env_value(env_name: &str, value: PathBuf) -> PathBuf {
    // No repo root to re-root ONTO — an out-of-tree consumer with no nano-ros
    // checkout above their crate. Keep whatever they supplied.
    let Some(here) = try_repo_root() else {
        return value;
    };
    let rerooted = reroot_foreign(&value, &here);
    if rerooted != value {
        println!(
            "cargo::warning=nano-ros: ${env_name} named another nano-ros checkout \
             ({}); building this one instead ({}). A linked worktree inherits the \
             parent shell's absolute SDK paths — issue 1280.",
            value.display(),
            rerooted.display()
        );
    }
    rerooted
}

/// Resolve an env-overridable path: if `env_name` is set, use it,
/// otherwise return `repo_root().join(rel)`. The returned path is
/// CANONICAL (see [`canonical`]).
///
/// An env value naming a DIFFERENT nano-ros checkout is re-rooted onto this one
/// — see [`reroot_foreign`] for why that outranks env-first, and why a path
/// outside any checkout still wins.
///
/// Emits NO rerun directive. `rerun-if-env-changed` on a path variable is
/// forbidden (issue 0491 — read [`canonical`] for why); what the build script
/// depends on is the CONTENT it reads, so the caller declares that with
/// [`watch_path`] (a whole first-party dir) or a per-file
/// `cargo:rerun-if-changed`. Watching is the caller's call because the paths
/// behind these variables differ in kind: `packages/platform/…/src` is a small
/// first-party tree that should be watched wholesale, while `NUTTX_DIR` names
/// a vendored SDK that its own build writes INTO — watching that would leave
/// every dependent build script permanently dirty.
pub fn env_or_repo_path(env_name: &str, rel: &str) -> PathBuf {
    let raw = match std::env::var(env_name) {
        Ok(v) if !v.is_empty() => reroot_env_value(env_name, PathBuf::from(v)),
        _ => repo_root().join(rel),
    };
    canonical(&raw)
}

/// Canonicalise a path-valued build input. Emits no directive.
///
/// **Path-valued build inputs are fingerprinted by their CONTENT, never by
/// their env spelling — `cargo:rerun-if-env-changed` on one is a bug**
/// (issue 0491). Gate: `scripts/check-path-env-fingerprints.py`.
///
/// Cargo compares an env var's value as TEXT. One directory has many
/// spellings, and this repo produces at least three for the same first-party
/// source dir:
///
/// * `just` exports it absolute (`just/sdk-env.just`, rooted at
///   `justfile_directory()`);
/// * a leaf `.cargo/config.toml` writes `{ value = "../../../../packages/…",
///   relative = true }`, which cargo resolves against THAT LEAF —
///   `…/rust/talker/../../../../packages/…` vs `…/rust/listener/../…`;
/// * a bare `cargo build` with neither leaves it unset.
///
/// While every leaf had its own `target/` those spellings never met. Sharing
/// one `--target-dir` per identity group (phase-340) put them in one
/// fingerprint namespace, and each sibling then re-ran the board / zpico build
/// scripts and cascaded `UnitDependencyInfoChanged` up to the leaf bin — six
/// FreeRTOS rows that could never all be fresh. Canonicalising cannot fix it
/// from this side: the string cargo compares is the one the CONFIG produced,
/// not the one the build script resolved.
///
/// Watching the directory says what the build script actually depends on — its
/// CONTENTS — and says it identically from every leaf.
///
/// The cost, stated plainly: cargo no longer notices that the variable now
/// names a DIFFERENT directory. Nothing re-runs the script, so it keeps
/// watching the old path (contents of the old dir still trigger correctly).
/// In-tree that cannot happen — the paths are fixed by the checkout. An
/// out-of-tree consumer who repoints one of these vars at another tree must
/// `cargo clean` (or touch a source) for that build dir, the same as changing
/// any other build input cargo cannot see.
pub fn canonical(path: &std::path::Path) -> PathBuf {
    // A path that does not exist yet (an SDK not provisioned, an optional
    // overlay dir) keeps its spelling — the caller's own diagnostic is the one
    // that should fire, not a canonicalisation error.
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// [`canonical`] plus `cargo:rerun-if-changed=<canonical path>` — the rerun
/// trigger a path-valued build input is allowed to have.
///
/// Use it for a first-party tree the build script READS
/// (`packages/platform/…/src`, an `include/` dir, a board `config/`). Do NOT
/// point it at a vendored SDK that its own build writes into, or at a build
/// output dir: cargo takes the newest mtime under the path, so watching such a
/// tree leaves every dependent script dirty after each unrelated build of it.
pub fn watch_path(path: &std::path::Path) -> PathBuf {
    let canonical = canonical(path);
    // A trigger on a path that does NOT exist makes the unit permanently dirty
    // (issue 0490 + `scripts/check-build-rs-rerun-paths.py`), which is the same
    // never-fresh outcome this function exists to remove. An absent path is
    // therefore skipped, not declared.
    if canonical.exists() {
        println!("cargo:rerun-if-changed={}", canonical.display());
    }
    canonical
}

/// Declare the input whose ABSENCE made a build script skip its compile
/// (issue 1586). Returns whether an edge could be declared.
///
/// A script that probes for an SDK, finds it missing and `return`s before any
/// `cargo:rerun-if-changed` leaves cargo with no input to watch but the
/// package's own files. Initialising the submodule its warning names then
/// changes nothing cargo looks at: the script stays `Fresh`, cargo replays the
/// CACHED "sources are absent" warning, and the link fails on symbols the skip
/// never compiled. Measured on `nros-board-freertos` — the only cure was a
/// `touch` of `build.rs`, the missing-edge shape.
///
/// Pass the SDK ROOT the probe looked under, not the probed file: an
/// uninitialised submodule is an EMPTY DIRECTORY (git creates it), and
/// populating it moves that directory's mtime, which cargo reads because it
/// scans a watched directory recursively. Watching the absent file itself
/// would be issue 0490's permanently-dirty unit. When the root does not exist
/// at all nothing is declared — watching an ancestor could mean scanning a
/// whole filesystem — and the caller's warning is the only signal.
///
/// The caller must still declare `rerun-if-changed=build.rs`: once a script
/// emits ANY rerun line, cargo stops watching the package by default.
pub fn watch_skip_cause(sdk_root: &std::path::Path) -> bool {
    let canonical = canonical(sdk_root);
    if canonical.is_dir() {
        println!("cargo:rerun-if-changed={}", canonical.display());
        true
    } else {
        false
    }
}

/// An env var that names a path, [`canonical`]ised — for the vars with no
/// in-repo default (`THREADX_DIR`, a board's `*_CONFIG_DIR`, …). Emits no
/// directive; `None` when unset or empty, so the caller keeps its own
/// diagnostic.
///
/// Re-rooted like [`env_or_repo_path`]: having no in-repo DEFAULT does not make
/// an inherited foreign-checkout value any more correct. `NUTTX_EXPORT_DIR`
/// reaches here, and it picks which kernel snapshot an image's headers come
/// from — the 0135/0460 class if it names another tree's.
pub fn env_path(env_name: &str) -> Option<PathBuf> {
    match std::env::var(env_name) {
        Ok(v) if !v.is_empty() => Some(canonical(&reroot_env_value(env_name, PathBuf::from(v)))),
        _ => None,
    }
}

/// [`env_path`] plus the content watch — use it when the variable names a
/// FIRST-PARTY tree (see [`watch_path`] for which paths must not be watched).
pub fn env_path_watched(env_name: &str) -> Option<PathBuf> {
    let p = env_path(env_name)?;
    println!("cargo:rerun-if-changed={}", p.display());
    Some(p)
}

/// A COLON-SEPARATED list of paths, every element resolved like [`env_path`].
/// Empty (or unset) yields an empty vector; emits no directive.
///
/// phase-471 W6 — the two sites issue 1527 left open.
/// `nros-board-threadx`'s `THREADX_EXTRA_INCLUDES` / `NETX_EXTRA_INCLUDES`
/// canonicalised each element and re-rooted none, in a build script whose
/// `THREADX_DIR` two screens up already went through [`env_path`]. A list is
/// not exempt from issue 1280: an element is a path, and the producers of these
/// two put a whole checkout's prefix in front of it —
/// `cmake/board/nano-ros-board-rv-virt-threadx.cmake` writes
/// `set(ENV{THREADX_EXTRA_INCLUDES} "${THREADX_DIR}/ports/…/qemu_virt")` from a
/// bare `$ENV{THREADX_DIR}`, so in a linked worktree the kernel SOURCES came
/// from here and `csr.h` / `plic.h` / `uart.h` / `hwtimer.h` came from the
/// checkout the parent shell had activated. Two trees in one `cc::Build`, which
/// is the 0135/0460 class rather than a tidiness question.
///
/// The three questions a list form has to answer, answered here rather than at
/// whichever call site is edited next:
///
/// * **An empty element means nothing, and is dropped.** `FOO=""` (a board with
///   no extra dir), `"a:"` and `"a::b"` all have to mean "no directory there".
///   The alternative is `PathBuf::from("")`, which as a `-I` argument names the
///   build script's own CWD — the board crate's manifest dir — and would put a
///   whole crate on the include path for a stray colon. This is the one place
///   where a list differs from [`env_path`] in kind, and it is why the list form
///   is a function rather than a `split` at each caller.
/// * **A relative element is KEPT, not re-rooted**, because
///   [`checkout_root_of`] answers `None` for one BY DESIGN: it resolves against
///   the caller's own cwd, so it cannot have been inherited from another
///   checkout and there is no owner to re-root off. It is still
///   [`canonical`]ised, which is exactly what these two sites did before.
/// * **`:` is the separator on every host this builds for.** The producers are
///   `just`, a cmake `set(ENV{…})` and a cargo `[env]` row, all of which write
///   `:`, and `nros-sdk-index.toml` has linux and macos host keys and no
///   windows one — where the separator would be `;` AND a drive letter would
///   make `:` ambiguous. Stated rather than assumed, so that the day a windows
///   host appears this reads as a thing to fix rather than a thing that works.
///
/// No `rerun-if-env-changed`, for [`canonical`]'s reason (issue 0491): the
/// value is a path. A caller that wants the contents watched passes each
/// element to [`watch_path`] — which `nros-board-threadx` already does.
#[must_use]
pub fn env_path_list(env_name: &str) -> Vec<PathBuf> {
    let Ok(raw) = std::env::var(env_name) else {
        return Vec::new();
    };
    split_list(&raw)
        .map(|s| canonical(&reroot_env_value(env_name, PathBuf::from(s))))
        .collect()
}

/// The separator and the empty-element rule, in ONE place — a caller that
/// re-spells `split(':')` is free to forget the filter, which is the drift the
/// list form exists to remove.
fn split_list(raw: &str) -> impl Iterator<Item = &str> {
    raw.split(':').filter(|s| !s.is_empty())
}

// Named resolvers for every var in `just/sdk-env.just`. Use these
// instead of hand-rolling `env::var("NROS_PLATFORM_*")` in every
// build script.

/// Canonical platform-header include dir. RFC-0042 D1 / phase-241 B.2 — the
/// canonical `<nros/platform.h>` (and its `platform_{net,timer,zephyr}.h`
/// siblings) moved from `nros-platform-cffi` to `nros-platform-api` (the lowest
/// crate). The name + the `NROS_PLATFORM_CFFI_INCLUDE` env var are kept for
/// caller/cmake compatibility; both now resolve to `nros-platform-api/include`.
pub fn nros_platform_cffi_include() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_CFFI_INCLUDE",
        "packages/platform/nros-platform-api/include",
    )
}

pub fn nros_platform_posix_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_POSIX_SRC",
        "packages/platform/nros-platform-posix/src",
    )
}

pub fn nros_platform_freertos_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_FREERTOS_SRC",
        "packages/platform/nros-platform-freertos/src",
    )
}

pub fn nros_platform_threadx_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_THREADX_SRC",
        "packages/platform/nros-platform-threadx/src",
    )
}

pub fn nros_lan9118_lwip_dir() -> PathBuf {
    env_or_repo_path("NROS_LAN9118_LWIP_DIR", "packages/drivers/net/lan9118-lwip")
}

pub fn nros_virtio_net_netx_dir() -> PathBuf {
    env_or_repo_path(
        "NROS_VIRTIO_NET_NETX_DIR",
        "packages/drivers/net/virtio-net-netx",
    )
}

pub fn nros_c_include() -> PathBuf {
    env_or_repo_path("NROS_C_INCLUDE", "packages/api/nros-c/include")
}

pub fn nros_cpp_include() -> PathBuf {
    env_or_repo_path("NROS_CPP_INCLUDE", "packages/api/nros-cpp/include")
}

pub fn freertos_dir() -> PathBuf {
    env_or_repo_path("FREERTOS_DIR", "third-party/freertos/kernel")
}

pub fn lwip_dir() -> PathBuf {
    env_or_repo_path("LWIP_DIR", "third-party/freertos/lwip")
}

pub fn freertos_config_dir() -> PathBuf {
    env_or_repo_path(
        "FREERTOS_CONFIG_DIR",
        "packages/boards/nros-board-mps2-an385-freertos/config",
    )
}

pub fn nuttx_dir() -> PathBuf {
    env_or_repo_path("NUTTX_DIR", "third-party/nuttx/nuttx")
}

pub fn nuttx_apps_dir() -> PathBuf {
    env_or_repo_path("NUTTX_APPS_DIR", "third-party/nuttx/nuttx-apps")
}

pub fn threadx_dir() -> PathBuf {
    env_or_repo_path("THREADX_DIR", "third-party/threadx/kernel")
}

pub fn netx_dir() -> PathBuf {
    env_or_repo_path("NETX_DIR", "third-party/threadx/netxduo")
}

pub fn tband_dir() -> PathBuf {
    env_or_repo_path("TBAND_DIR", "third-party/tracing/Tonbandgeraet/tband")
}

/// The NuttX include root whose `nuttx/config.h` describes THIS build's arch.
///
/// Issue 0525 — NuttX is built IN PLACE, so `$NUTTX_DIR/include/nuttx/config.h`
/// belongs to whichever arch the shared checkout was configured for LAST, and
/// one checkout serves both in-tree arches. Anything deriving a compile input
/// from it silently takes the other arch's values when two arches share a tree,
/// which `lane=tier2` does (it builds nuttx-riscv after nuttx).
///
/// That is issue 0511: the ARM image was linked with the RISC-V memory map,
/// whose `CONFIG_FLASH_SIZE` is 0, so ROM had LENGTH 0 and every byte placed in
/// it "overflowed" — read as a 400-500 KB size regression that survived clean
/// rebuilds, because the stale `.config` lives in the submodule rather than in
/// any target dir.
///
/// Lives HERE rather than in `nros-board-common` so every consumer shares ONE
/// spelling; a second copy of this resolution is exactly the drift that produced
/// 0511 in the first place. The crate that forced the split was `nuttx-sys`,
/// which could not depend on the board helpers — phase-400 deleted it as
/// unreferenced, but the rule outlived it: any standalone build script with the
/// same constraint resolves through here rather than respelling it.
///
/// Prefers `nros-nuttx-export-<arch>/include` when it carries a
/// `nuttx/config.h`, else the live tree's `include/` so a pre-phase-339 checkout
/// keeps working. Emits `rerun-if-changed` on BOTH spellings whether or not they
/// exist (issue 0477's rule): the config IS the memory map and the ABI, so a
/// reconfigure must invalidate whatever was derived from it — including on the
/// branch that lost.
pub fn nuttx_include_root(nuttx_dir: &std::path::Path) -> PathBuf {
    let shared = nuttx_dir.join("include");
    println!(
        "cargo:rerun-if-changed={}",
        shared.join("nuttx/config.h").display()
    );
    // issue 0750 (B) — `$NUTTX_EXPORT_DIR` FIRST, matching
    // `nros_board_common::nuttx_export::snapshot_root`'s resolution order.
    //
    // These two functions answer halves of one question: that one picks the
    // `libs/` a kernel image links, this one picks the `include/` its code
    // compiles against. `snapshot_root` honoured the override and this did not,
    // so a caller that pointed at one snapshot for libs still compiled headers
    // from whichever snapshot the TARGET ARCH named — and with two configs of
    // one arch (`arm` and `arm-smp`) that is a silent headers-from-A,
    // libs-from-B split. That is the 0135/0460 class: a config-dependent
    // struct layout differing across two TUs of one image, which does not fail
    // to link, it fails at runtime with garbage.
    //
    // Watched by CONTENT, not fingerprinted as a string (issue 0491): it is a
    // PATH, and cargo compares env values textually.
    if let Some(explicit) = env_path("NUTTX_EXPORT_DIR") {
        let inc = explicit.join("include");
        println!(
            "cargo:rerun-if-changed={}",
            inc.join("nuttx/config.h").display()
        );
        if inc.join("nuttx/config.h").is_file() {
            return inc;
        }
    }
    // Otherwise the snapshot is named for the ARCH being compiled for, which is
    // all a target triple can tell us. A second config of the same arch must
    // therefore pass `NUTTX_EXPORT_DIR` — the triple cannot distinguish them.
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let snapshot_arch = match arch.as_str() {
        "arm" => Some("arm"),
        "riscv32" | "riscv64" => Some("riscv"),
        _ => None,
    };
    if let Some(a) = snapshot_arch {
        let inc = nuttx_dir
            .join(format!("nros-nuttx-export-{a}"))
            .join("include");
        println!(
            "cargo:rerun-if-changed={}",
            inc.join("nuttx/config.h").display()
        );
        if inc.join("nuttx/config.h").is_file() {
            return inc;
        }
    }
    shared
}

/// The version `nros-sdk-index.toml` PINS for `[tool.<tool>]` — issue 1546.
///
/// A provisioned tool lives at `<store>/<tool>/<version>` because `nros setup`
/// read `<version>` from the index; a consumer CONSTRUCTS that path from the
/// same two inputs and never lists the store to pick one. The store is shared
/// between checkouts and accumulates (issue 0500), while the pin is
/// per-checkout, so "the newest version present" is as often a sibling
/// checkout's answer as ours.
///
/// `None` when there is no index to read (an out-of-tree consumer with no
/// checkout above it) or no such section — a caller then has no store rung.
///
/// Twins, because the build systems cannot call each other:
/// `nros_sdk_pin()` in `cmake/NanoRosSdkPin.cmake` and
/// `nros_sdk_pinned_version` in `scripts/lib/sdk-pin.sh`.
pub fn sdk_pinned_version(tool: &str) -> Option<String> {
    let index = try_repo_root()?.join("nros-sdk-index.toml");
    let text = std::fs::read_to_string(index).ok()?;
    pinned_version_in(&text, tool)
}

/// [`sdk_pinned_version`]'s parser, over index TEXT so it is testable without
/// a checkout. The section runs from `[tool.<tool>]` to the next line that
/// STARTS a table — the bound the cmake and shell twins use, so an inline
/// array such as `smoke = [` does not end it. No `toml` dependency: this crate
/// has none, and adding one moves `Cargo.lock` for a single key.
fn pinned_version_in(text: &str, tool: &str) -> Option<String> {
    let header = format!("[tool.{tool}]");
    let mut inside = false;
    for line in text.lines() {
        if line == header {
            inside = true;
            continue;
        }
        if line.starts_with('[') {
            inside = false;
            continue;
        }
        if !inside {
            continue;
        }
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start().strip_prefix('"')?;
        let v = &rest[..rest.find('"')?];
        return (!v.is_empty()).then(|| v.to_string());
    }
    None
}

#[cfg(test)]
mod pinned_version_tests {
    use super::pinned_version_in;

    const INDEX: &str = "\
[tool.corrosion]
version = \"0.6.1-nros1\"

[tool.riscv-none-elf-gcc]
smoke = [
    { run = \"bin/riscv-none-elf-gcc --version\" },
]
version = \"14.2-nros1\"

[tool.riscv-none-elf-gcc.dist.linux-x86_64]
version = \"not-this-one\"
";

    #[test]
    fn reads_the_named_section_only() {
        assert_eq!(
            pinned_version_in(INDEX, "riscv-none-elf-gcc").as_deref(),
            Some("14.2-nros1")
        );
        assert_eq!(
            pinned_version_in(INDEX, "corrosion").as_deref(),
            Some("0.6.1-nros1")
        );
    }

    #[test]
    fn a_missing_section_is_none_not_a_neighbours_version() {
        assert_eq!(pinned_version_in(INDEX, "arm-none-eabi-gcc"), None);
        // A subtable header is not the section it extends.
        assert_eq!(pinned_version_in(INDEX, "riscv-none-elf-gcc.dist"), None);
    }
}

/// The riscv64 bare-metal toolchain, resolved rather than spelled — issue 0657.
///
/// `[board.rv-virt-threadx]` provisions xPack's `riscv-none-elf-gcc`, and
/// it is what `nros setup` installs on every supported host. The build scripts
/// spelled the compiler `riscv64-unknown-elf-*` (Ubuntu's package), so a host
/// provisioned entirely by `nros setup` could not build this platform at all.
///
/// The shell twin is `scripts/build/riscv64-toolchain.sh` and the cmake twin is
/// in `cmake/toolchain/riscv64-threadx.cmake`; all three read the same order and
/// honour `NROS_RISCV64_PREFIX` first. Three spellings exist because the three
/// build systems cannot call each other — not because the rule differs.
pub mod riscv64 {
    use std::path::PathBuf;

    /// Candidate prefixes, most portable first. `riscv-none-elf` leads because
    /// it is the one the SDK index pins and provisioning installs.
    const CANDIDATES: &[&str] = &[
        "riscv-none-elf",
        "riscv64-unknown-elf",
        "riscv64-none-elf",
        "riscv64-elf",
    ];

    fn sdk_store() -> PathBuf {
        if let Ok(s) = std::env::var("NROS_SDK_STORE") {
            return PathBuf::from(s);
        }
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".nros/sdk")
    }

    /// The store's `riscv-none-elf-gcc` at the PINNED version, if provisioned.
    ///
    /// CONSTRUCTED as `<store>/riscv-none-elf-gcc/<pin>/bin`, never found by
    /// listing the store (issue 1546). This used to take the newest version
    /// present, citing issue 0500 — but the store is shared between checkouts,
    /// so "newest" let a sibling checkout's newer install shadow this tree's
    /// pin, which is 0500 with the sign flipped. Other versions are never used;
    /// when only others are present, a `cargo:warning` names them.
    fn store_bin() -> Option<PathBuf> {
        const TOOL: &str = "riscv-none-elf-gcc";
        let dir = sdk_store().join(TOOL);
        let pin = super::sdk_pinned_version(TOOL)?;
        let bin = dir.join(&pin).join("bin");
        if bin.join(TOOL).is_file() {
            return Some(bin);
        }
        // Name what IS there, for a human — nothing is chosen from it, which is
        // why it is not sorted.
        let others: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()) && *v != pin)
            .collect();
        if !others.is_empty() {
            println!(
                "cargo:warning=the SDK store has {TOOL} {} but NOT the pinned {pin} — not used \
                 (issue 1546: only <store>/{TOOL}/<pin> counts). Provision the pin: \
                 nros setup --tool {TOOL}",
                others.join(", ")
            );
        }
        None
    }

    /// `<prefix>-<suffix>` as an absolute path when the toolchain came from the
    /// SDK store, a bare name when it came from `PATH`, `None` when there is no
    /// toolchain at all — the caller decides whether that is a skip or an error,
    /// and this function never guesses on its behalf.
    pub fn tool(suffix: &str) -> Option<String> {
        if let Ok(prefix) = std::env::var("NROS_RISCV64_PREFIX")
            && !prefix.is_empty()
        {
            return Some(format!("{prefix}-{suffix}"));
        }
        if let Some(bin) = store_bin() {
            let p = bin.join(format!("riscv-none-elf-{suffix}"));
            if p.is_file() {
                return Some(p.to_string_lossy().into_owned());
            }
        }
        CANDIDATES
            .iter()
            .map(|p| format!("{p}-{suffix}"))
            .find(|name| which_on_path(name))
    }

    /// `tool()`, or the historical spelling so a caller that cannot skip still
    /// produces the old error message rather than a confusing empty one.
    pub fn tool_or_legacy(suffix: &str) -> String {
        tool(suffix).unwrap_or_else(|| format!("riscv64-unknown-elf-{suffix}"))
    }

    fn which_on_path(name: &str) -> bool {
        std::env::var_os("PATH")
            .map(|paths| {
                std::env::split_paths(&paths).any(|dir| {
                    let p = dir.join(name);
                    p.is_file()
                })
            })
            .unwrap_or(false)
    }
}

/// Issue 1280 — the rule, exercised on real directories.
///
/// Real directories because the rule is a filesystem question: the marker file
/// has to be *there* for a path to be attributed to a checkout, and a test that
/// stubs that away would pass against an implementation that never looks.
///
/// No `tempfile`: this crate has no dev-dependencies and adding one would move
/// `Cargo.lock` for a test helper (issues 0359/0378 — a lock moves when a dev
/// means it). The scratch tree is a few lines.
#[cfg(test)]
mod reroot_tests {
    use super::*;
    use std::path::Path;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "nros-1280-{tag}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// A directory that answers "yes" to [`checkout_root_of`].
        fn checkout(&self, name: &str) -> PathBuf {
            let root = self.0.join(name);
            let marker = root.join(CHECKOUT_MARKER);
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

    /// The bug: an inherited SDK path from ANOTHER checkout is re-rooted here,
    /// sub-path intact. This is the FreeRTOS-source case from the issue's
    /// acceptance — the edit in the worktree is what must get compiled.
    #[test]
    fn a_path_in_another_checkout_is_rerooted_onto_this_one() {
        let s = Scratch::new("foreign");
        let main = s.checkout("main");
        let worktree = s.checkout("worktree");

        let inherited = main.join("packages/platform/nros-platform-freertos/src");
        assert_eq!(
            reroot_foreign(&inherited, &worktree),
            worktree.join("packages/platform/nros-platform-freertos/src")
        );

        // It does not need to EXIST to be attributed — an unprovisioned SDK
        // directory is still that checkout's.
        let sdk = main.join("third-party/nuttx/nuttx");
        assert!(!sdk.exists());
        assert_eq!(
            reroot_foreign(&sdk, &worktree),
            worktree.join("third-party/nuttx/nuttx")
        );

        // The checkout ROOT itself (`NROS_REPO_DIR`), with an empty sub-path.
        assert_eq!(reroot_foreign(&main, &worktree), worktree);
    }

    /// The reason env-first exists, and the row the fix must not regress: a
    /// path OUTSIDE any nano-ros checkout is a real out-of-tree SDK. Kept.
    #[test]
    fn a_path_outside_any_checkout_is_kept() {
        let s = Scratch::new("outside");
        let worktree = s.checkout("worktree");
        let vendor = s.0.join("opt/vendor/nuttx");
        std::fs::create_dir_all(&vendor).unwrap();

        assert_eq!(reroot_foreign(&vendor, &worktree), vendor);
        assert_eq!(checkout_root_of(&vendor), None);
    }

    /// Our own checkout is not foreign to itself — including when it is reached
    /// through a symlinked parent, which is two names for one tree (issue 0375)
    /// rather than two trees.
    #[test]
    fn this_checkout_is_never_foreign_to_itself() {
        let s = Scratch::new("self");
        let here = s.checkout("here");
        let inside = here.join("third-party/freertos/kernel");
        assert_eq!(reroot_foreign(&inside, &here), inside);

        #[cfg(unix)]
        {
            let alias = s.0.join("alias");
            std::os::unix::fs::symlink(&here, &alias).unwrap();
            // Same tree, second spelling: comparing the strings would call it
            // foreign and re-root a path onto itself through the alias.
            assert_eq!(reroot_foreign(&alias.join("third-party"), &here), {
                alias.join("third-party")
            });
        }
    }

    /// A relative value cannot have been inherited from another checkout — it
    /// resolves against the caller's own cwd — so it is never attributed.
    #[test]
    fn a_relative_path_is_not_attributed_to_any_checkout() {
        assert_eq!(checkout_root_of(Path::new("third-party/nuttx")), None);
    }

    /// phase-471 W6 — the list form's own question: an empty element is not a
    /// path. `PathBuf::from("")` as a `-I` argument names the build script's
    /// CWD, so a trailing or doubled `:` would put the board crate's manifest
    /// dir on the include path.
    #[test]
    fn an_empty_list_element_is_dropped_not_turned_into_the_cwd() {
        assert_eq!(split_list("").collect::<Vec<_>>(), Vec::<&str>::new());
        assert_eq!(split_list(":").collect::<Vec<_>>(), Vec::<&str>::new());
        assert_eq!(split_list("a::b:").collect::<Vec<_>>(), vec!["a", "b"]);
    }

    /// Every element gets the rule, and gets it INDEPENDENTLY: one list can
    /// legitimately mix a foreign-checkout element (re-rooted), a real
    /// out-of-tree SDK dir (kept) and a relative one (kept, no owner to re-root
    /// off). A list form that applied one verdict to the whole string would be
    /// wrong for two of those three.
    #[test]
    fn each_list_element_gets_the_rule_on_its_own() {
        let s = Scratch::new("list");
        let main = s.checkout("main");
        let worktree = s.checkout("worktree");
        let vendor = s.0.join("opt/vendor/threadx");
        std::fs::create_dir_all(&vendor).unwrap();

        let raw = format!(
            "{}:{}:{}",
            main.join("third-party/threadx/kernel/ports/risc-v64/gnu/example_build/qemu_virt")
                .display(),
            vendor.display(),
            "ports/linux/gnu/inc",
        );
        let got: Vec<PathBuf> = split_list(&raw)
            .map(|e| reroot_foreign(Path::new(e), &worktree))
            .collect();

        assert_eq!(
            got,
            vec![
                worktree
                    .join("third-party/threadx/kernel/ports/risc-v64/gnu/example_build/qemu_virt"),
                vendor,
                PathBuf::from("ports/linux/gnu/inc"),
            ]
        );
    }

    /// The walk stops at the INNERMOST checkout. Agent worktrees live under
    /// `.claude/worktrees/` INSIDE the main checkout here, so a nested tree is
    /// the normal shape and the outer marker must not win.
    #[test]
    fn the_innermost_checkout_owns_a_nested_path() {
        let s = Scratch::new("nested");
        let outer = s.checkout("outer");
        let inner = s.checkout("outer/.claude/worktrees/agent-x");

        assert_eq!(
            checkout_root_of(&inner.join("third-party/nuttx")).as_deref(),
            Some(inner.as_path())
        );
        // …and a path in the OUTER tree is still foreign to the inner one,
        // even though the outer root is a prefix of it.
        assert_eq!(
            reroot_foreign(&outer.join("third-party/nuttx"), &inner),
            inner.join("third-party/nuttx")
        );
    }
}

/// phase-471 W4 — the one job eleven build scripts were each doing by hand:
/// put a linker script where the linker will look for it.
///
/// `cortex-m-rt`'s `link.x` does `INCLUDE memory.x`, and a `svd2rust` PAC's
/// `device.x` is included the same way. Neither is found unless the file sits
/// in a directory on the link search path, so every image crate for a
/// bare-metal board grew the same fourteen lines: read `OUT_DIR`, write the
/// bytes there, print `rustc-link-search`, print two `rerun-if-changed`.
///
/// The census found **11 scripts carrying 6 distinct bodies** — the largest
/// duplication in the tree that no gate had an opinion about. It had produced
/// no defect, which is why phase-471 rates it lowest priority and why the
/// consolidation is a plain deduplication rather than a fix.
///
/// **Two of the eleven are deliberately left alone.**
/// `packages/reference/stm32f4-porting/{polling,rtic}` are copy-out templates
/// for BSP developers, per their own README. Giving a template a dependency on
/// a crate that exists only in this checkout is RFC-0026's hazard, and it is
/// the same reason the twelve Zephyr leaf shims keep their own copies. They
/// keep their twenty lines.
///
/// **Why this crate and not `nros-board-common`.** Only 2 of the 11 are board
/// crates; the other 9 are testing and reference crates, and a testing binary
/// depending on a board helper crate is backwards. The precedent is recorded
/// rather than invented — `nros-build-helpers/Cargo.toml` says of issue 0657
/// that "the riscv64 toolchain resolver lives in the ZERO-DEP crate, not here:
/// this one pulls cbindgen, and putting a directory lookup behind that dragged
/// cbindgen into the `nros` CLI graph (118 lock lines)." Same class, same
/// answer: this crate has no dependencies at all, so each caller pays one edge
/// and no new compile unit beyond it.
pub mod link_script {
    use std::path::PathBuf;

    /// Write `bytes` into `OUT_DIR` as `dest` and put `OUT_DIR` on the link
    /// search path, watching `source` for changes.
    ///
    /// **Prefer the [`link_script!`](macro@crate::link_script) macro.** Calling this
    /// directly lets `source` name one file while `bytes` come from another,
    /// which is the whole hazard of a two-part statement: the watch would be on
    /// a file the image does not contain, so editing the real script would
    /// change nothing and cargo would report everything fresh. The macro takes
    /// ONE literal and derives both uses from it, so the two cannot disagree.
    ///
    /// `dest` is a file NAME. A separator in it would place the file outside
    /// `OUT_DIR`, where the `rustc-link-search` printed here does not point —
    /// so it is refused rather than silently emitted somewhere the linker will
    /// not look.
    pub fn emit(source: &str, dest: &str, bytes: &[u8]) {
        let out = PathBuf::from(std::env::var_os("OUT_DIR").expect(
            "nros-build-paths: OUT_DIR not set (link_script::emit must be called from a build script)",
        ));
        write_into(&out, dest, bytes);

        println!("cargo:rustc-link-search={}", out.display());
        println!("cargo:rerun-if-changed={source}");
        // The caller's own `build.rs`, relative to its CARGO_MANIFEST_DIR.
        // Every one of the nine scripts this replaced printed it, so it is
        // kept rather than reasoned away: a build script that emits any
        // `rerun-if-changed` gets ONLY the watches it names, and whether
        // recompiling the script binary is enough on its own is a property of
        // cargo nobody here has measured.
        println!("cargo:rerun-if-changed=build.rs");
    }

    /// The part of [`emit`] that touches the filesystem, split out so it can be
    /// tested without setting `OUT_DIR` — a process-global the test harness
    /// shares with every other test in this crate.
    fn write_into(out: &std::path::Path, dest: &str, bytes: &[u8]) {
        assert!(
            !dest.is_empty()
                && !dest.contains('/')
                && !dest.contains('\\')
                && dest != "."
                && dest != "..",
            "nros-build-paths: link-script destination {dest:?} must be a bare file name — \
             the link search path printed here is OUT_DIR itself, so a file written outside \
             it would never be found"
        );

        let path = out.join(dest);
        std::fs::write(&path, bytes).unwrap_or_else(|e| {
            panic!(
                "nros-build-paths: could not write linker script to {}: {e}",
                path.display()
            )
        });
    }

    #[cfg(test)]
    mod tests {
        use super::write_into;

        fn scratch(tag: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!(
                "nros-link-script-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        /// The bytes reach `OUT_DIR` under the name the LINKER wants, which is
        /// not always the name the file has in the tree: `nros-board-mps2-an385`
        /// ships `mps2-an385.x` and `cortex-m-rt`'s `link.x` says
        /// `INCLUDE memory.x`.
        #[test]
        fn the_destination_name_is_the_one_the_linker_looks_for() {
            let out = scratch("rename");
            write_into(&out, "memory.x", b"MEMORY { /* an385 */ }\n");

            assert!(!out.join("mps2-an385.x").exists());
            assert_eq!(
                std::fs::read(out.join("memory.x")).unwrap(),
                b"MEMORY { /* an385 */ }\n"
            );
        }

        /// A destination carrying a separator would land outside `OUT_DIR`,
        /// which is the one directory [`super::emit`] puts on the link search
        /// path — so the file would exist and the linker would still not find
        /// it. Refused rather than written somewhere useless.
        #[test]
        fn a_destination_that_escapes_out_dir_is_refused() {
            let out = scratch("escape");
            for bad in ["../memory.x", "sub/memory.x", "", "..", "."] {
                let r = std::panic::catch_unwind(|| write_into(&out, bad, b"x"));
                assert!(r.is_err(), "destination {bad:?} should have been refused");
            }
            // …and nothing was written on the way to refusing.
            assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
        }
    }
}

/// Emit a linker script from the calling build script's own directory.
///
/// ```ignore
/// // memory.x, beside build.rs, is what the linker must find:
/// nros_build_paths::link_script!("memory.x");
///
/// // …or when the file in the tree and the name the linker wants differ:
/// nros_build_paths::link_script!("mps2-an385.x" => "memory.x");
/// ```
///
/// The literal is resolved relative to the file that INVOKES the macro, not to
/// this one: `include_bytes!` keys on the span of its string argument, and the
/// argument here comes from the caller. That is what lets one macro in one
/// crate embed nine different files.
///
/// See [`crate::link_script::emit`] for why the one-literal form is the sanctioned
/// way in.
#[macro_export]
macro_rules! link_script {
    ($file:literal) => {
        $crate::link_script::emit($file, $file, include_bytes!($file))
    };
    ($source:literal => $dest:literal) => {
        $crate::link_script::emit($source, $dest, include_bytes!($source))
    };
}
