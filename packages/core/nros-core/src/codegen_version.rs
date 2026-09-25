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
//   * `nros-core` itself, directly.
//   * `nros-build-helpers` (defines the runtime anchors) and `rosidl-codegen`
//     (stamps the emitted artifacts), both by `include!`. Neither can afford a
//     `nros-core` dependency edge for a pair of `const`s: the first is host-only
//     and appears in every tracked leaf lockfile, and the second lives in the
//     separate `packages/cli` workspace, so the edge would add rows to its
//     lockfile too. `include!` costs nothing in the graph, and rustc records the
//     path in the depfile, so editing the constants rebuilds both.
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
pub const NROS_CODEGEN_VERSION: u32 = 8;

/// The oldest codegen version this runtime still accepts.
///
/// Equal to [`NROS_CODEGEN_VERSION`] at introduction: no window, because
/// `generated/` is never committed and regeneration is therefore always
/// available. Raise the floor only when a real migration needs one, and lower
/// it never.
///
/// Held at 1 while [`NROS_CODEGEN_VERSION`] moved to 2 (phase-417, rebased onto
/// phase-429's gate). That surface move is ADDITIVE plus one relocation, so a
/// tree generated against version 1 still runs: the C additions are new entry
/// points, the two C++ changes add `size()`/`empty()` and `std::string` interop
/// to `FixedString`/`HeapString`, and the four `nros_ret_t` declarations that
/// left `action.h` / `client.h` / `parameter.h` / `service.h` were CONSOLIDATED
/// into `nros_generated.h`, which all four still reach through `nros/types.h`
/// — the headers say so in a comment, and keeping that include is what makes
/// `#include <nros/action.h>` continue to compile. Nothing a version-1 tree
/// names was withdrawn, so this is the window the doc above describes rather
/// than a migration.
///
/// Still 2 while [`NROS_CODEGEN_VERSION`] moved to 3 (issues 1148/1149). That
/// move is one ADDED runtime entry point, `nros_cdr_align`, which the
/// header-only `nros_cdr_borrow_le_slice_*` now call so an 8-byte borrowed
/// sequence starts on its stream boundary. A version-2 tree names nothing that
/// was withdrawn: it compiles against the corrected `nros/view.h` and picks the
/// fix up for free, because the view helpers are `static inline` in the header
/// rather than symbols the generated code defines.
///
/// Still 2 while [`NROS_CODEGEN_VERSION`] moved to 4 (phase-427 W7). That move
/// is a SPELLING change and nothing else: `nros::bind_subscription_sized` — the
/// one demanded C++ signature that names the node type — now writes its
/// parameter `::rclcpp::Node&` where it wrote `Node&`, because the class moved
/// to `rclcpp::` and `nros::Node` became a deprecated alias for it. The two
/// spellings are ONE TYPE (`std::is_same<rclcpp::Node, nros::Node>` is asserted
/// by `tests/compile/one_node_type.cpp`), so a version-3 tree — which declares
/// its node `static ::nros::Node __nros_node_0;` — still compiles and still
/// links, with a deprecation warning naming its replacement. Nothing it names
/// was withdrawn, which is the window the doc above describes.
///
/// The version moves anyway because the gate is fail-closed on the TEXT of a
/// demanded declaration and should stay that way: it cannot know that two
/// spellings are one type, and a gate that tried to would be one that misses a
/// real rename.
///
/// Still 2 while [`NROS_CODEGEN_VERSION`] moved to 5 (phase-417 W5.b/c/d). That
/// move is ADDITIVE and, in most of its entries, is not a change to the runtime
/// at all: the rclc preset constructors landed as `static inline` forwarders in
/// `<nros/{publisher,subscription,service,client,action}.h>`, and naming a type
/// inside one of those headers is what puts a `c|type|<header>|<name>` row in
/// the extracted surface — `nros_message_type_t`, `nros_node_t`,
/// `nros_service_type_t` and `nros_action_type_t` were already declared in
/// `nros_generated.h` and still are, reached the same way. The one genuinely
/// new declaration a demanded name resolves to is
/// `nros_service_init_with_qos`, which is an entry point that already existed
/// and is now also visible from `<nros/service.h>`. Nothing a version-4 tree
/// names was withdrawn, so this is the window the doc above describes rather
/// than a migration.
///
/// The version moves anyway for the reason the version-4 paragraph gives: the
/// gate is fail-closed on the TEXT of a demanded declaration and should stay
/// that way. A gate that tried to decide which additions are "really" runtime
/// changes is one that misses a real withdrawal.
///
/// Still 2 while [`NROS_CODEGEN_VERSION`] moved to 6 (phase-417 W4.b). That
/// move is a WITHDRAWAL, and it is the first one — `nros_core::action` stopped
/// re-exporting `ActionServer` and `ActionClient`, two "type-level marker"
/// structs. A version-5 tree still runs, because no generated tree has ever
/// named them: measured before the deletion, they had ZERO references anywhere
/// in this repository, and they were UNREACHABLE through the `nros` facade at
/// all, which re-exports `nros_node`'s live `ActionServer` / `ActionClient`
/// under exactly those spellings. So the floor stays where it is. The gate is
/// fail-closed on the extracted surface and cannot know which names nothing
/// reached, which is the property that makes it worth keeping.
///
/// Still 2 while [`NROS_CODEGEN_VERSION`] moved to 7 (issue 1437, phase-444).
/// ADDITIVE on the surface generated code names, plus one APPEND to a struct
/// nothing generated declares. The additions are six new C entry points
/// (`nros_{publisher,subscription}_get_actual_qos` and the four service /
/// client halves) and two enumerators on each of the four `nros_qos_*_t`
/// enums (`UNKNOWN`, and `SYSTEM_DEFAULT` on three of them), appended so every
/// existing discriminant keeps its value. The append is `_executor` on
/// `nros_subscription_t`, placed after `_opaque` so every existing field
/// offset is unchanged — it grows the struct, which is why this is a surface
/// move at all, and a version-6 tree that only names the struct by pointer is
/// unaffected. Nothing was withdrawn, so the floor stays where it is.
///
/// The gate reports the move as two CHANGED types rather than as the four
/// added prototypes, because a `const struct nros_service_t *` parameter is a
/// tagged reference and keys on the same `c|type|<header>|<name>` row as the
/// struct itself. That is the gate being fail-closed on text, which is the
/// property the version-4 paragraph above argues for keeping.
///
/// Still 2 while [`NROS_CODEGEN_VERSION`] moved to 8 (phase-467 Q3). ADDITIVE,
/// and the first move that adds a TRAIT generated code implements — which is
/// the trigger the doc above names first. `nros_core::SecNanosecMsg` is the
/// Rust stand-in for the C++ `template <typename TimeMsgT> Time::to_msg`: Rust
/// has no structural bound for "has `sec` and `nanosec`", so
/// `rosidl-codegen` emits `impl nros_core::SecNanosecMsg for <Msg>` for every
/// generated message with that exact field set, and
/// `nros_core::Time::to_ros_msg()` returns through it. The surface moved by two
/// rows, both additions: the trait, and the `pub use time::` re-export that
/// publishes it.
///
/// A version-7 tree still runs: it names nothing that was withdrawn, and the
/// impl is something a NEWER tree emits rather than something an older one is
/// missing. What an older tree loses is only the conversion itself —
/// `t.to_ros_msg()` into one of ITS types will not resolve until it is
/// regenerated, which is a compile error at the call site on the day someone
/// writes the call, not a silent mismatch. So the floor stays where it is.
///
/// The range `[NROS_CODEGEN_VERSION_MIN, NROS_CODEGEN_VERSION]` is expressed to
/// C and C++ as a SET OF DEFINED SYMBOLS rather than as a comparison — see
/// `nros-build-helpers`' codegen-version anchor — so there is no range check on
/// that side that could itself be wrong.
pub const NROS_CODEGEN_VERSION_MIN: u32 = 2;

/// Does `emitted` fall in the range this runtime accepts?
///
/// The ONE comparison. Rust call sites reach it through
/// `nros_node::codegen_version_check`, the CLI through `abi_guard`; neither
/// re-spells the bounds.
#[must_use]
pub const fn accepts(emitted: u32) -> bool {
    emitted >= NROS_CODEGEN_VERSION_MIN && emitted <= NROS_CODEGEN_VERSION
}

// The accepted range must be non-empty, checked at COMPILE time because it is a
// property of two constants. A runtime test would be the wrong tool — and
// clippy says so (`assertions_on_constants`) in every crate that `include!`s
// this file, which is how it was found.
const _: () = assert!(
    NROS_CODEGEN_VERSION_MIN <= NROS_CODEGEN_VERSION,
    "the accepted codegen range is empty: nothing could ever be compatible"
);
