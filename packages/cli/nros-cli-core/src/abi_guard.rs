//! The compatibility guard between the `nros` binary that EMITS code and the
//! nano-ros runtime that will COMPILE and LINK it.
//!
//! Phase 218.E built this; phase-429 W2 re-tokened it. The plumbing —
//! [`check_workspace`] / [`check_workspaces`], the [`Verb`] that names which
//! command tripped it, the actionable error, the visible
//! [`SKIP_ENV`] opt-out — is the phase-218 shape and is unchanged. What
//! changed is the VALUE being compared, and the source each side reads it
//! from. The original header predicted this refactor and said the call sites
//! would not move; they did not.
//!
//! ## The token
//!
//! [`EMITTED_VERSION`] (`E`) — the CODEGEN VERSION this binary emits — against
//! the runtime's accepted range `MIN..=N` ([`AcceptedRange`]). Both are
//! `nros_core::codegen_version` numbers; the rule is that crate's `accepts`.
//!
//! It used to be the CLI's `CARGO_PKG_VERSION` compared by strict SemVer
//! equality against the `nros-core` version in the consumer's `Cargo.lock`.
//! Three things were wrong with that, and each is a reason the TOKEN changes
//! rather than the plumbing:
//!
//! 1. **A release version is not a compatibility statement.** Two releases can
//!    emit identical code and two builds of one version need not, so equality
//!    fired on bumps that changed no emitter. A guard that cries wolf teaches
//!    people to set `NROS_SKIP_VERSION_CHECK` and leave it set, which is worse
//!    than no guard.
//! 2. **A `Cargo.lock` is a Rust artifact.** A C or C++ consumer has none, and
//!    a missing lock was warn-and-continue — so the guard was silently absent
//!    for exactly the users a prebuilt binary exists for. The runtime is now
//!    located by the SOURCE TREE (below), which every language has.
//! 3. **A point is not a range.** `MIN..=N` lets emitter and runtime move
//!    independently for as long as the contract holds.
//!
//! ## Where each side reads its number
//!
//! * `E` is compiled in, from the `nros-core` this binary's emitter
//!   (`rosidl-codegen`) was built against. No I/O; always available; printed
//!   by `nros --codegen-version`.
//! * `MIN..=N` is read from the nano-ros RUNTIME TREE the consumer will link,
//!   located by [`runtime_root`] and parsed by [`accepted_range_in_tree`] out
//!   of `packages/core/nros-core/src/codegen_version.rs`. Source, not a build
//!   artifact and not a lockfile, so a C-only or C++-only consumer is checked
//!   on exactly the same terms as a Rust one.
//!
//! ## When the guard cannot answer
//!
//! [`runtime_root`] can fail — a prebuilt `nros` invoked by a consumer with no
//! nano-ros checkout above it and no `NROS_REPO_DIR` set. So can the parse, if
//! the located tree predates phase-429 and has no `codegen_version.rs`. Both
//! are note-and-continue, not refusal: a guard that cannot see the runtime has
//! no evidence of a mismatch, and refusing on absence of evidence is the same
//! trained-to-bypass failure as (1). Point it at a tree with `NROS_REPO_DIR`
//! to get the check back.
//!
//! ## Opt-out
//!
//! `NROS_SKIP_VERSION_CHECK=<non-empty>` bypasses the check with a `warning:`
//! line on stderr, so the bypass is visible in CI logs.

use std::{
    env,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail};

/// The CODEGEN VERSION this binary EMITS (`E`).
///
/// Baked at compile time from the `nros-core` the emitter was built against
/// (`rosidl_codegen::EMITTED_CODEGEN_VERSION`). This is the number
/// `nros --codegen-version` prints and the one the mismatch message names.
pub const EMITTED_VERSION: u32 = rosidl_codegen::EMITTED_CODEGEN_VERSION;

/// The CLI binary's release version. NOT the compatibility token any more —
/// carried in the error message only, because "which build of `nros` is this"
/// is what a user needs to answer before they can fix a mismatch.
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Env-var name that bypasses the guard (any non-empty value opts out).
pub const SKIP_ENV: &str = "NROS_SKIP_VERSION_CHECK";

/// Env-var naming the nano-ros runtime tree explicitly. Same spelling
/// `nros sync` already uses for "which nano-ros checkout", so a consumer that
/// has told us once has told us for both.
pub const RUNTIME_ROOT_ENV: &str = "NROS_REPO_DIR";

/// The marker that identifies a nano-ros source tree, relative to its root.
const MONOREPO_MARKER: &str = "packages/core/nros-core/Cargo.toml";

/// The runtime's own declaration of what it accepts, relative to the tree root.
const CODEGEN_VERSION_SRC: &str = "packages/core/nros-core/src/codegen_version.rs";

/// The codegen version range a runtime accepts: `min..=max`.
///
/// `max` is also the version that runtime's own emitter would emit, which is
/// why one number serves both roles in `nros_core::codegen_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedRange {
    pub min: u32,
    pub max: u32,
}

impl AcceptedRange {
    /// The comparison rule — the same predicate as
    /// `nros_core::codegen_version::accepts`, applied to a range read out of
    /// a tree rather than the one compiled into this binary.
    pub fn accepts(self, emitted: u32) -> bool {
        emitted >= self.min && emitted <= self.max
    }
}

/// The command that triggered the check — flows into the error message so the
/// user knows which one tripped the guard.
#[derive(Debug, Clone, Copy)]
pub enum Verb {
    Build,
    Sync,
    Codegen,
    GenerateRust,
    GenerateC,
    GenerateCpp,
    CodegenSystem,
}

impl Verb {
    fn as_str(self) -> &'static str {
        match self {
            Verb::Build => "nros build",
            Verb::Sync => "nros sync",
            Verb::Codegen => "nros codegen",
            Verb::GenerateRust => "nros generate-rust",
            Verb::GenerateC => "nros generate c",
            Verb::GenerateCpp => "nros generate cpp",
            Verb::CodegenSystem => "nros codegen-system",
        }
    }
}

/// Walk up from `start` to find the nano-ros source-tree root — the directory
/// containing [`MONOREPO_MARKER`]. Returns `None` when `start` is not inside
/// such a tree.
pub fn find_monorepo_root(start: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    };
    while let Some(dir) = cur {
        if dir.join(MONOREPO_MARKER).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// Locate the nano-ros runtime tree whose acceptance range applies to a
/// consumer anchored at `start`.
///
/// Order, most-specific first:
///
/// 1. A nano-ros tree ABOVE the consumer. In-tree examples, fixtures and
///    workspaces link the runtime they sit inside, whatever language they are
///    written in — this is the arm that covers a C or C++ consumer with no
///    `Cargo.lock`. It beats the environment on purpose: `NROS_REPO_DIR` is
///    ambient after `source activate.sh`, so a contributor with two checkouts
///    open would otherwise have every consumer in the second one measured
///    against the first.
/// 2. `NROS_REPO_DIR` — the consumer said which checkout it links, and is not
///    inside one. Honoured as given, so a wrong value fails at the parse
///    rather than being silently ignored.
/// 3. A nano-ros tree above THIS BINARY. A contributor's `nros` built by
///    `just setup-cli` lives in the tree whose runtime an out-of-tree
///    consumer is nearly always pointed at by `[patch.crates-io]` or by
///    `find_package(NanoRos)`. It is the optimistic arm — an external consumer
///    resolving `nros-core` from elsewhere would be checked against the wrong
///    tree — but the alternative is no check at all, and a prebuilt `nros`
///    copied out of its tree (the case that arm would get wrong most often)
///    has no tree above it and falls through to `None`.
pub fn runtime_root(start: &Path) -> Option<PathBuf> {
    if let Some(root) = find_monorepo_root(start) {
        return Some(root);
    }
    if let Some(explicit) = env::var_os(RUNTIME_ROOT_ENV).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    let exe = env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    find_monorepo_root(&exe)
}

/// Read a runtime tree's accepted codegen-version range out of its own
/// `codegen_version.rs`.
///
/// Returns `Ok(None)` when the file is absent — a tree older than phase-429
/// declares no range, and "I could not find out" is not "they disagree".
///
/// The parse is deliberately line-shaped rather than a Rust parse: the two
/// constants are the crate's public contract and are written on one line each,
/// and a source-level dependency on `syn` to read two integers would be a
/// second mechanism.
pub fn accepted_range_in_tree(root: &Path) -> Result<Option<AcceptedRange>> {
    let src = root.join(CODEGEN_VERSION_SRC);
    if !src.is_file() {
        return Ok(None);
    }
    let body = std::fs::read_to_string(&src)
        .wrap_err_with(|| format!("read codegen version from {}", src.display()))?;
    let max = const_u32_in(&body, "NROS_CODEGEN_VERSION");
    let min = const_u32_in(&body, "NROS_CODEGEN_VERSION_MIN");
    match (min, max) {
        (Some(min), Some(max)) => Ok(Some(AcceptedRange { min, max })),
        _ => bail!(
            "{} exists but does not declare both `NROS_CODEGEN_VERSION_MIN` and \
             `NROS_CODEGEN_VERSION` as `pub const … : u32 = <n>;` — the guard \
             cannot read the runtime's accepted range",
            src.display()
        ),
    }
}

/// Pull `pub const <name>: u32 = <n>;` out of Rust source.
///
/// Matches on the whole declaration prefix, so the many prose mentions of
/// these names in the module's doc comments cannot be mistaken for the
/// declaration — and `NROS_CODEGEN_VERSION` cannot match the `…_MIN` line,
/// because the `:` has to follow the name immediately.
fn const_u32_in(body: &str, name: &str) -> Option<u32> {
    let decl = format!("pub const {name}: u32 =");
    for line in body.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix(&decl) else {
            continue;
        };
        return rest.trim().trim_end_matches(';').trim().parse().ok();
    }
    None
}

/// Run the guard against a single anchor.
///
/// `start` is either a file (the `package.xml` / `args-file` / `system.toml`
/// the verb received) or a directory (the workspace root); the runtime tree is
/// resolved from it by [`runtime_root`].
///
/// Returns `Ok(())` on accept, on opt-out, and when the runtime cannot be
/// located. Returns `Err(...)` on refusal, with an actionable message.
pub fn check_workspace(start: &Path, verb: Verb) -> Result<()> {
    check_workspaces(std::slice::from_ref(&start), verb)
}

/// Multi-anchor variant — checks each in turn and fails on the first refusal.
/// Used by verbs that resolve inputs from more than one workspace (e.g.
/// `nros codegen-system` reading a bringup pkg from workspace A + member pkgs
/// from workspace B).
pub fn check_workspaces(starts: &[&Path], verb: Verb) -> Result<()> {
    if env::var(SKIP_ENV).map(|v| !v.is_empty()).unwrap_or(false) {
        warn_bypass(verb);
        return Ok(());
    }

    let mut checked: Vec<PathBuf> = Vec::new();
    for start in starts {
        let Some(root) = runtime_root(start) else {
            warn_no_runtime(start, verb);
            continue;
        };
        if checked.iter().any(|prev| prev == &root) {
            continue;
        }
        let Some(range) = accepted_range_in_tree(&root)? else {
            warn_no_range(&root, verb);
            checked.push(root);
            continue;
        };
        if !range.accepts(EMITTED_VERSION) {
            bail!("{}", mismatch_message(verb, &root, range, EMITTED_VERSION));
        }
        checked.push(root);
    }
    Ok(())
}

fn warn_bypass(verb: Verb) {
    let _ = writeln!(
        std::io::stderr(),
        "warning: {} ABI version guard bypassed via {SKIP_ENV}=1 \
         (CLI emits codegen version {EMITTED_VERSION})",
        verb.as_str(),
    );
}

/// Note-only: quiet unless the caller asked for tracing or stderr is a TTY.
/// Pure-codegen verbs legitimately run outside any nano-ros tree, and one line
/// per invocation would be noise in every build log.
fn note(msg: std::fmt::Arguments<'_>) {
    if env::var("NROS_TRACE_ABI_GUARD")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
        || std::io::stderr().is_terminal()
    {
        let _ = writeln!(std::io::stderr(), "{msg}");
    }
}

fn warn_no_runtime(start: &Path, verb: Verb) {
    note(format_args!(
        "note: {} found no nano-ros runtime tree at or above {} (and none above \
         this binary); skipping the codegen version guard. Set {RUNTIME_ROOT_ENV} \
         to the nano-ros checkout you link to enable it.",
        verb.as_str(),
        start.display(),
    ));
}

fn warn_no_range(root: &Path, verb: Verb) {
    note(format_args!(
        "note: {} found a nano-ros tree at {} that declares no codegen version \
         (no {CODEGEN_VERSION_SRC}); skipping the guard.",
        verb.as_str(),
        root.display(),
    ));
}

fn mismatch_message(verb: Verb, root: &Path, range: AcceptedRange, emitted: u32) -> String {
    let direction = if emitted > range.max {
        "This `nros` is NEWER than the runtime: it emits code the runtime does \
         not understand yet. Update the runtime tree, or use the `nros` that \
         matches it."
    } else {
        "This `nros` is OLDER than the runtime: it emits code the runtime has \
         dropped support for. Rebuild the CLI from the runtime tree."
    };
    format!(
        "{verb} aborted: ABI version mismatch between the `nros` CLI binary \
         and the nano-ros runtime your build links.\n  \
         CLI emits codegen version:    {emitted}\n  \
         Runtime tree:                 {root}\n  \
         Runtime accepts:              {min}..={max}\n  \
         (CLI binary release version:  {cli})\n\n\
         {direction}\n\n\
         The CLI emits Rust / C / C++ that targets a specific runtime contract; \
         a mismatch can manifest as link errors, struct-layout mismatches, or \
         silent runtime UB. Rebuild the CLI against this runtime:\n  \
         cargo build --release --manifest-path {root}/packages/cli/Cargo.toml --bin nros\n\
         (or `./scripts/bootstrap.sh` / contributors' `just setup-cli` if the target workspace IS nano-ros itself).\n\n\
         `nros --codegen-version` prints the CLI's number; the runtime's range is \
         in {src}.\n\n\
         To bypass this guard for an intentional cross-version workflow, set \
         {SKIP_ENV}=1.",
        verb = verb.as_str(),
        root = root.display(),
        min = range.min,
        max = range.max,
        cli = CLI_VERSION,
        src = CODEGEN_VERSION_SRC,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir_path(tag: &str) -> std::path::PathBuf {
        crate::test_support::scratch_path(&format!("phase-429-w2-{tag}"))
    }

    /// A synthetic runtime tree declaring `min..=max`.
    fn write_runtime_tree(root: &Path, min: u32, max: u32) {
        std::fs::create_dir_all(root.join("packages/core/nros-core/src")).unwrap();
        std::fs::write(root.join(MONOREPO_MARKER), "# stub\n").unwrap();
        std::fs::write(
            root.join(CODEGEN_VERSION_SRC),
            format!(
                "//! Mentions `NROS_CODEGEN_VERSION` and `NROS_CODEGEN_VERSION_MIN` \
                 in prose first.\n\
                 pub const NROS_CODEGEN_VERSION: u32 = {max};\n\
                 pub const NROS_CODEGEN_VERSION_MIN: u32 = {min};\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn a_range_accepts_its_endpoints_and_refuses_outside() {
        let r = AcceptedRange { min: 2, max: 4 };
        assert!(r.accepts(2));
        assert!(r.accepts(3));
        assert!(r.accepts(4));
        assert!(!r.accepts(1), "older than MIN must refuse");
        assert!(!r.accepts(5), "newer than N must refuse");
    }

    #[test]
    fn the_emitted_version_is_the_runtimes_own_number() {
        // The binary emits what the nros-core it was built against declares.
        // If this ever disagrees, `E` has acquired a second source.
        assert_eq!(EMITTED_VERSION, rosidl_codegen::EMITTED_CODEGEN_VERSION);
    }

    #[test]
    fn this_checkout_accepts_this_binary() {
        // The in-tree pairing must hold, or every in-tree build refuses.
        let repo = find_monorepo_root(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("the CLI crate is inside the nano-ros tree");
        let range = accepted_range_in_tree(&repo)
            .expect("parse this tree's codegen_version.rs")
            .expect("this tree declares a codegen version");
        assert!(
            range.accepts(EMITTED_VERSION),
            "in-tree pairing must hold: emits {EMITTED_VERSION}, tree accepts {}..={}",
            range.min,
            range.max,
        );
    }

    #[test]
    fn range_is_parsed_from_a_trees_source_not_a_lockfile() {
        let tmp = tempdir_path("parse_range");
        write_runtime_tree(&tmp, 3, 7);
        // Deliberately NO Cargo.lock anywhere: this is the C/C++ consumer case.
        assert!(!tmp.join("Cargo.lock").exists());
        let range = accepted_range_in_tree(&tmp).unwrap().unwrap();
        assert_eq!(range, AcceptedRange { min: 3, max: 7 });
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn the_min_constant_does_not_shadow_the_max_one() {
        // `NROS_CODEGEN_VERSION` is a prefix of `NROS_CODEGEN_VERSION_MIN`; a
        // substring match would read MIN into both fields.
        let body = "pub const NROS_CODEGEN_VERSION_MIN: u32 = 9;\n\
                    pub const NROS_CODEGEN_VERSION: u32 = 11;\n";
        assert_eq!(const_u32_in(body, "NROS_CODEGEN_VERSION"), Some(11));
        assert_eq!(const_u32_in(body, "NROS_CODEGEN_VERSION_MIN"), Some(9));
    }

    #[test]
    fn a_tree_without_the_file_declares_nothing_rather_than_erroring() {
        let tmp = tempdir_path("no_decl");
        std::fs::create_dir_all(tmp.join("packages/core/nros-core")).unwrap();
        std::fs::write(tmp.join(MONOREPO_MARKER), "# stub\n").unwrap();
        assert_eq!(accepted_range_in_tree(&tmp).unwrap(), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn runtime_root_walks_up_from_a_consumer_with_no_lockfile() {
        let tmp = tempdir_path("root_walk");
        write_runtime_tree(&tmp, 1, 1);
        // A C consumer: package.xml + CMakeLists, no Cargo.lock in sight.
        let consumer = tmp.join("examples/native/c/talker");
        std::fs::create_dir_all(&consumer).unwrap();
        std::fs::write(consumer.join("package.xml"), "<package/>\n").unwrap();
        assert_eq!(runtime_root(&consumer).as_deref(), Some(tmp.as_path()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn check_refuses_a_tree_that_rejects_this_binary_and_accepts_one_that_does_not() {
        // Both directions, on the same plumbing the verbs call.
        let reject = tempdir_path("check_reject");
        write_runtime_tree(&reject, EMITTED_VERSION + 5, EMITTED_VERSION + 9);
        let err = check_workspace(&reject, Verb::Build)
            .expect_err("a runtime that accepts only newer emissions must refuse");
        let msg = format!("{err}");
        assert!(msg.contains("ABI version mismatch"), "{msg}");
        assert!(msg.contains("nros build"), "{msg}");
        assert!(msg.contains("OLDER than the runtime"), "{msg}");

        let accept = tempdir_path("check_accept");
        write_runtime_tree(&accept, EMITTED_VERSION, EMITTED_VERSION);
        check_workspace(&accept, Verb::Build).expect("an exact-match runtime must proceed");

        let _ = std::fs::remove_dir_all(&reject);
        let _ = std::fs::remove_dir_all(&accept);
    }

    #[test]
    fn a_binary_newer_than_the_runtime_says_so() {
        let tmp = tempdir_path("check_newer");
        // Runtime stuck at 0..=0 while this binary emits >= 1.
        write_runtime_tree(&tmp, 0, 0);
        let err = check_workspace(&tmp, Verb::Sync).expect_err("must refuse");
        let msg = format!("{err}");
        assert!(msg.contains("NEWER than the runtime"), "{msg}");
        assert!(msg.contains("nros sync"), "{msg}");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
