//! The C diagnostics every nano-ros C compile must treat as errors.
//!
//! # The class (issue 0383)
//!
//! C99 removed implicit function declarations. A call whose declaring header is
//! never included therefore has no prototype: the compiler invents
//! `int f()`, and the call is compiled against a signature that may share
//! nothing with the real one — wrong argument passing, a truncated pointer
//! return on LP64, silently. Its sibling, `int-conversion`, is the same failure
//! one step later: an `int` where a pointer belongs.
//!
//! gcc <= 13 and clang <= 14 only *warn*. gcc >= 14 and clang >= 15 reject.
//! Two such calls shipped in vendored zenoh-pico for years and were fatal the
//! first time anyone built on a rolling distro (`system/common/serial.h`,
//! `link/config/custom.h`).
//!
//! # Why a helper and not a flag at each call site
//!
//! The embedded C in this repo is compiled by the PINNED cross toolchain —
//! arm-none-eabi-gcc 13.2 — which only warns, and warnings scroll past in a
//! firmware build. So the exposure is not "some host has a new gcc"; it is that
//! our own default toolchain cannot see the class at all. Turning the two
//! diagnostics into errors makes the pinned toolchain reject what a future one
//! would, instead of deferring the discovery to whoever upgrades first.
//!
//! There are ~30 `cc::Build` sites across the board / platform / RMW crates.
//! Spelling the flags at each is how this repo has historically grown a second
//! idiom and then a third (CLAUDE.md, "fix the CLASS"). One helper, one
//! spelling, one place to widen the set.
//!
//! # Do not add `-w` next to this
//!
//! On gcc <= 13 — i.e. the PINNED arm-none-eabi-gcc 13.2 — `-w` silently wins
//! over `-Werror=implicit-function-declaration`, in either order:
//!
//! ```text
//! arm-none-eabi-gcc -w -Werror=implicit-function-declaration -fsyntax-only x.c  # rc=0
//! arm-none-eabi-gcc -Werror=implicit-function-declaration -w -fsyntax-only x.c  # rc=0
//! arm-none-eabi-gcc -Werror=implicit-function-declaration    -fsyntax-only x.c  # rc=1
//! ```
//!
//! and nothing re-enables the diagnostic afterwards. A gcc >= 14 host hides
//! this, because there the construct is a default error that `-w` cannot
//! suppress — so the gate would look healthy on the host that needs it least.
//!
//! `cc::Build::warnings(false)` is fine and used at most call sites: it only
//! makes cc-rs OMIT `-Wall`/`-Wextra`, it passes no `-w`, and both diagnostics
//! are on by default in gcc. Verified on the compile lines cc-rs actually
//! emits — the only `-W` flags on a vendored lwIP TU are these two.
//!
//! # Scope
//!
//! C only. Both diagnostics are C-language options; a C++ compile rejects the
//! constructs outright, and gcc merely warns that the option does not apply —
//! noise with no signal. Do not call this on a `cc::Build` in C++ mode.
//!
//! # Escape hatch
//!
//! `NROS_CC_STRICT_DECLS=0` disables it, for bisecting a vendored submodule
//! bump that trips the class before the fix is ready. It is a debugging knob,
//! not a supported configuration: a tree that needs it is a tree with a latent
//! miscompile.

/// Diagnostics that must be errors in every nano-ros C compile.
pub const STRICT_DECL_FLAGS: &[&str] = &[
    "-Werror=implicit-function-declaration",
    "-Werror=int-conversion",
];

/// Name of the escape-hatch variable ([`strict_decls`] is a no-op when it is `0`).
pub const DISABLE_ENV: &str = "NROS_CC_STRICT_DECLS";

/// Apply [`STRICT_DECL_FLAGS`] to a C `cc::Build`.
///
/// No-op on MSVC (the flags are gcc/clang spellings) and when
/// [`DISABLE_ENV`] is `0`.
///
/// ```no_run
/// let mut build = cc::Build::new();
/// build.file("src/platform.c");
/// nros_cc_flags::strict_decls(&mut build);
/// build.compile("platform");
/// ```
pub fn strict_decls(build: &mut cc::Build) -> &mut cc::Build {
    println!("cargo:rerun-if-env-changed={DISABLE_ENV}");
    if std::env::var(DISABLE_ENV).as_deref() == Ok("0") {
        return build;
    }
    // `try_get_compiler` needs TARGET/OPT_LEVEL etc. from cargo; outside a build
    // script it fails, and so does every other cc call — treat "unknown" as
    // gcc-like rather than silently dropping the gate.
    let is_msvc = build
        .try_get_compiler()
        .map(|c| c.is_like_msvc())
        .unwrap_or(false);
    if is_msvc {
        return build;
    }
    for flag in STRICT_DECL_FLAGS {
        build.flag(flag);
    }
    build
}
