// RFC-0090 / phase-429 — the codegen version, the one token that says whether
// generated code and this runtime can work together.
//
// NO `//!` INNER DOC COMMENTS IN THIS FILE, ON PURPOSE. It is `include!`d
// verbatim by `nros-build-helpers`, and an inner doc comment inside an
// `include!` expansion is a hard error (E0753). The module docs live on the
// `pub mod` in `lib.rs`. Keep this file a dependency-free set of `const`s and
// `const fn`s: anything else breaks the include.
//
// THREE READERS, THREE REASONS — do not "unify" them:
//
//   * `nros-core` itself, and `rosidl-codegen` through a real path dep. Type
//     checked, no parsing. `rosidl-codegen` already path-deps `nros-serdes`, so
//     the edge is precedented there.
//   * `nros-build-helpers`, by `include!`. It is host-only and appears in every
//     tracked leaf lockfile, so a `nros-core` dependency edge would land there
//     too. `include!` costs nothing in the graph, and rustc records the path in
//     the depfile, so editing the constants rebuilds it.
//   * the CLI's guard, by parsing this file as TEXT. It inspects a CONSUMER's
//     tree at run time, where compiling is not available. This is the only
//     parser, and it exists because the other two options do not apply.

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
