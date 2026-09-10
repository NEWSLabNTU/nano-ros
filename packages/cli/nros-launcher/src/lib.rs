//! The launcher — RFC-0097 D4, over phase-440 W7's `dispatch.rs`.
//!
//! ## Why this is a crate and not a module
//!
//! W7 put the launcher logic in `nros-cli-core`, so the binary that SELECTS a
//! version was shipped by the version being selected. RFC-0097 measured what
//! that costs: `packages/cli` takes **710 commits per 60 days**, and a launcher
//! that shares an artifact with it re-releases on every one of them, while
//! carrying no compatibility promise of its own.
//!
//! rustup's `rustup` and `cargo` are separate artifacts for the same reason: *a
//! proxy that outlives what it proxies cannot share a release with it.*
//!
//! So the split is by ARTIFACT, and this crate is the small half:
//!
//! ```text
//!   $NROS_STORE/bin/nros              the launcher   (this crate's bin)
//!     read nros-toolchain.toml
//!     ensure toolchains/<pinned>
//!     exec  toolchains/<pinned>/bin/nros "$@"
//!
//!   $NROS_STORE/toolchains/<v>/bin/nros   the toolchain (packages/cli/nros-cli)
//! ```
//!
//! ## What moved here, and what did NOT get copied
//!
//! [`pin`] and [`dispatch`] are phase-440 W7's files, MOVED (`git mv`), not
//! reimplemented — `nros-cli-core` re-exports them at their old paths, so
//! `orchestration::pin::…` and `orchestration::dispatch::…` still resolve and
//! there is exactly one parser of `nros-toolchain.toml` in the tree. Same for
//! [`store_root::root`] (was `orchestration::store::root`) and
//! [`checkout::find_monorepo_root`] (was `abi_guard::find_monorepo_root`): a
//! launcher needs both, and a second spelling of either is how the launcher and
//! the toolchain come to disagree about which store, or which checkout, they
//! are looking at.
//!
//! [`launch`] is the new part: the launcher's own resolution, which differs
//! from [`dispatch::decide`] in three ways it cannot share (an unpinned project
//! must still resolve to SOMETHING, a store with no toolchain at all is a real
//! state a launcher must report rather than crash in, and CI must refuse rather
//! than choose — RFC-0097 D11).

pub mod checkout;
pub mod dispatch;
pub mod launch;
pub mod pin;
pub mod session;
pub mod store_root;
/// Test-only helpers, reached by this crate's tests and by `nros-cli`'s
/// launcher tests. `#[doc(hidden)]` — not part of the launcher's surface; see
/// the module header for why it is here and not in `nros-cli-core`.
#[doc(hidden)]
pub mod test_support;

/// This launcher's own version — NOT the toolchain's.
///
/// Reported by `nros --version` on its own line, above whatever the toolchain
/// prints. Without that line the split buys nothing observable: a user looking
/// at one version string cannot tell which artifact answered.
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");
