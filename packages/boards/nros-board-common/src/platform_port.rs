//! Issue 1779 — ONE `nros_platform_*` provider per linked graph.
//!
//! The platform ABI (`<nros/platform.h>`) is a set of free C symbols bound at
//! link time, so every binary must link exactly one port that defines them.
//! Two crates can compile one:
//!
//! * `nros-platform-cffi`, whose `posix-c-port` feature compiles the POSIX
//!   port (and whose `c-stub-test` compiles counting stubs);
//! * a board crate whose build script compiles its RTOS port —
//!   `nros-board-threadx` (`nros_platform_threadx`), `nros-board-freertos`
//!   (`nros_platform_freertos`), and the NuttX boards through
//!   [`crate::nuttx_platform_build`] (`nros_platform_nuttx`).
//!
//! An image never asks for two. Cargo's feature unification can still hand it
//! two: `cargo test --workspace` builds every member with ONE feature set per
//! crate, so `nros-board-linux`'s `nros-platform-cffi/posix-c-port` reached
//! `nros-board-threadx`'s host test binary, which also linked the ThreadX port
//! whole-archive. lld then reported every `nros_platform_*` symbol twice.
//! It was visible only where `third-party/threadx/kernel` was initialised,
//! because the board skips its C build without the sources. So the same
//! command was green in a CI worktree and red on a developer's checkout.
//!
//! The rule, in one place: `nros-platform-cffi` STATES the provider it compiled
//! (`links = "nros_platform_cffi"`, `cargo:abi_provider=<posix|stubs|none>`),
//! and a board build script that is about to compile a second port ASKS first,
//! through [`defer_to_graph_provider`]. When the graph already has one, the
//! board skips its C port, the same skip it takes when its sources are absent.
//! The Rust surface still compiles, and a real image of that board still fails
//! loudly at link (its kernel symbols are missing), with the warning in the
//! same log.
//!
//! Gate: `check-platform-port-single-provider` (every build-time compile of an
//! `nros_platform_*` archive outside `nros-platform-cffi` is preceded by this
//! call).

/// The environment variable cargo derives from `nros-platform-cffi`'s
/// `links = "nros_platform_cffi"` and its `cargo:abi_provider=…` line. Visible
/// to the build script of every crate that depends on `nros-platform-cffi`
/// DIRECTLY — which every port-compiling board does.
pub const PROVIDER_ENV: &str = "DEP_NROS_PLATFORM_CFFI_ABI_PROVIDER";

/// What a port-compiling build script should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortDecision {
    /// No other provider in the graph: compile the port.
    Compile,
    /// `provider` already defines the ABI in this graph: compiling the port
    /// would define every symbol a second time.
    Defer { provider: String },
}

/// The pure rule: `provider` is the value of [`PROVIDER_ENV`] (absent when the
/// crate does not depend on `nros-platform-cffi` directly).
pub fn decide(provider: Option<&str>) -> PortDecision {
    match provider.map(str::trim) {
        None | Some("") | Some("none") => PortDecision::Compile,
        Some(p) => PortDecision::Defer {
            provider: p.to_string(),
        },
    }
}

/// Ask before compiling `port` (e.g. `"ThreadX"`) for `crate_name`.
///
/// Returns `true`, after a `cargo:warning` that names the provider and this
/// issue, when another provider is already in the graph; the caller then
/// returns from `main` without compiling its C port. Returns `false` when the
/// port is this graph's provider.
pub fn defer_to_graph_provider(crate_name: &str, port: &str) -> bool {
    println!("cargo:rerun-if-env-changed={PROVIDER_ENV}");
    let value = std::env::var(PROVIDER_ENV).ok();
    match decide(value.as_deref()) {
        PortDecision::Compile => false,
        PortDecision::Defer { provider } => {
            println!(
                "cargo:warning={crate_name}: this build already links the `{provider}` \
                 nros_platform_* provider (nros-platform-cffi, reached through cargo feature \
                 unification, e.g. `cargo test --workspace`); skipping the {port} C port, which \
                 would define every nros_platform_* symbol a second time (issue 1779). The crate \
                 still compiles as a Rust shell; an image of this board must not enable \
                 `nros-platform-cffi/posix-c-port`."
            );
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_provider_compiles_the_port() {
        assert_eq!(decide(None), PortDecision::Compile);
        assert_eq!(decide(Some("none")), PortDecision::Compile);
        assert_eq!(decide(Some("")), PortDecision::Compile);
    }

    #[test]
    fn a_provider_in_the_graph_defers_the_port() {
        // The issue-1779 case: feature unification gave nros-platform-cffi its
        // POSIX port, and the ThreadX board was about to compile a second one.
        assert_eq!(
            decide(Some("posix")),
            PortDecision::Defer {
                provider: "posix".into()
            }
        );
        assert_eq!(
            decide(Some("stubs")),
            PortDecision::Defer {
                provider: "stubs".into()
            }
        );
    }
}
