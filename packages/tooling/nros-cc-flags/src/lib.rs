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

/// Keep cc-rs from handing gcc a clang-only frame-pointer flag (issue 0478).
///
/// When cc-rs decides to force a frame pointer — which it does off cargo's
/// `DEBUG`, and our `nros-relwithdebinfo` carries `debug = 1` — it emits BOTH
/// `-fno-omit-frame-pointer` and `-mno-omit-leaf-frame-pointer`. The second is
/// a **clang** spelling. gcc does not have it and does not merely warn:
///
/// ```text
/// arm-none-eabi-gcc: error: unrecognized command-line option
///   '-mno-omit-leaf-frame-pointer'; did you mean '-fno-omit-frame-pointer'?
/// ```
///
/// which failed every `freertos` fixture row while the other six platform
/// modules passed. Nothing in this repo passed the flag; it arrived when the
/// generated, untracked workspace lock re-resolved `cc` to a newer version, so
/// no commit is behind it.
///
/// The fix keeps the INTENT rather than dropping frame pointers: turn cc-rs's
/// automatic pair off, then re-add the half gcc actually understands, so a
/// debug build still gets a frame pointer. clang and MSVC are left alone —
/// under clang the flag is legal and useful, so a blanket disable would give up
/// leaf frame pointers on the toolchain that supports them.
pub fn gcc_safe_frame_pointer(build: &mut cc::Build) -> &mut cc::Build {
    let Ok(tool) = build.try_get_compiler() else {
        // Same reasoning as `strict_decls`: outside a build script every cc
        // call fails anyway. Do nothing rather than guess at a compiler.
        return build;
    };
    if tool.is_like_clang() || tool.is_like_msvc() {
        return build;
    }
    let wants_frame_pointer = std::env::var("DEBUG").as_deref() == Ok("true");
    build.force_frame_pointer(false);
    if wants_frame_pointer {
        build.flag("-fno-omit-frame-pointer");
    }
    build
}

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
    // issue 0478 — applied HERE, not at each call site, for the same reason the
    // strict diagnostics are: this function is already the one thing every
    // nano-ros C compile calls, so routing the frame-pointer policy through it
    // fixes ~20 sites at once and leaves no site for the next one to miss. The
    // escape hatch below deliberately does NOT cover it: `NROS_CC_STRICT_DECLS=0`
    // is for bisecting a diagnostics failure, and a build that cannot pass a
    // flag its compiler rejects is broken either way.
    gcc_safe_frame_pointer(build);
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
