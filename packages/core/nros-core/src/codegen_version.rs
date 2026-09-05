//! RFC-0089 / phase-429 — the codegen version, the one token that says whether
//! generated code and this runtime can work together.
//!
//! # What breaks without it
//!
//! nano-ros shipped prebuilt `nros` binaries early and stopped, because a
//! released binary emitted code that had drifted from the runtime. The failure
//! is not loud: drifted generated code *compiles*, and the image is simply
//! wrong in whatever way the generator was wrong. Issue 1018 records the
//! canonical instance — the C emitter transposed sequence-of-strings dimensions
//! in all three emission sites (`char data[256][64]` for `[64][256]`) — caught
//! by a developer's build refusal, which is a thing no user has.
//!
//! The relation that breaks is **generated code ↔ runtime**. The binary is only
//! the thing that produced one side of it, which is why the version lives here,
//! in the runtime, rather than in the CLI.
//!
//! # Why an integer and not a hash
//!
//! A hash cannot say WHICH SIDE IS BEHIND, and that distinction is the whole
//! value of the token: `G < MIN` is "regenerate, silently" and `G > VERSION` is
//! "a human must move a pin". They are not interchangeable remedies. A hash
//! also moves when a doc comment moves, and a check that fires on cosmetic
//! change is one people learn to bypass — `NROS_SKIP_VERSION_CHECK` is right
//! there.
//!
//! The hash still exists, as the ratchet's evidence
//! (`scripts/check-codegen-version-surface.py`), and is never compared at
//! build time.
//!
//! # Not to be confused with the codegen FINGERPRINT
//!
//! `nros codegen-fingerprint` hashes every byte the emitters produce for a
//! compiled-in corpus. It answers *"would this binary emit different bytes?"* —
//! a FRESHNESS question, whose remedy is to regenerate, silently. This constant
//! answers *"can this code work with this runtime?"* — a COMPATIBILITY
//! question, whose remedy is a refusal. Conflating them is why issue 1018's
//! stale-CLI refusal both over-fires on `cmd/doctor.rs` and cannot ship.

/// The codegen version this runtime emits and accepts.
///
/// **Bump this deliberately** when the interface between generated code and the
/// runtime changes: a trait signature generated code implements, a symbol it
/// defines, a layout rule it obeys. Do NOT bump it for a cosmetic template
/// edit — that moves the fingerprint, which is a different question (see the
/// module docs).
///
/// Bumping invalidates every generated tree. That is affordable here because
/// `generated/` is never committed (CLAUDE.md), so regeneration is always
/// available — and it is exactly why the value must not move on cosmetics.
///
/// Gated by `check-codegen-version-surface`, which fails when the surface
/// generated code names changes and this constant does not.
pub const NROS_CODEGEN_VERSION: u32 = 1;

/// The oldest codegen version this runtime still accepts.
///
/// Equal to [`NROS_CODEGEN_VERSION`] at introduction: no window, because
/// `generated/` is never committed and regeneration is therefore always
/// available. Raise the floor only when a real migration needs one, and lower
/// it never.
///
/// The range `[NROS_CODEGEN_VERSION_MIN, NROS_CODEGEN_VERSION]` is expressed to
/// C and C++ as a SET OF DEFINED SYMBOLS rather than as a comparison — see
/// `nros-build-helpers`' codegen-version anchor — so there is no range check on
/// that side that could itself be wrong.
pub const NROS_CODEGEN_VERSION_MIN: u32 = 1;

/// Does `emitted` fall in the range this runtime accepts?
///
/// The ONE comparison. Rust call sites reach it through
/// `nros_node::codegen_version_check`, the CLI through `abi_guard`; neither
/// re-spells the bounds.
#[must_use]
pub const fn accepts(emitted: u32) -> bool {
    emitted >= NROS_CODEGEN_VERSION_MIN && emitted <= NROS_CODEGEN_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_is_never_above_the_ceiling() {
        assert!(
            NROS_CODEGEN_VERSION_MIN <= NROS_CODEGEN_VERSION,
            "the accepted range is empty: nothing could ever be compatible"
        );
    }

    #[test]
    fn accepts_the_range_and_nothing_outside_it() {
        assert!(accepts(NROS_CODEGEN_VERSION));
        assert!(accepts(NROS_CODEGEN_VERSION_MIN));
        assert!(
            !accepts(NROS_CODEGEN_VERSION + 1),
            "code emitted by a NEWER binary must be refused — the runtime is \
             the older side and cannot know what changed"
        );
        assert!(
            !accepts(NROS_CODEGEN_VERSION_MIN.saturating_sub(1)) || NROS_CODEGEN_VERSION_MIN == 0,
            "code below the floor must be refused, not silently accepted"
        );
    }

    /// Version 0 is reserved for "did not say".
    ///
    /// A generated artifact that carries no version reads as 0 through every
    /// path that parses one, and must never be accepted: an artifact that did
    /// not declare its version is exactly the pre-phase-429 artifact this
    /// mechanism exists to catch.
    #[test]
    fn zero_is_never_accepted() {
        assert!(!accepts(0));
    }
}
