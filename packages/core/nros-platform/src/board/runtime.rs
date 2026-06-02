//! [`RuntimeCtx`] — Phase 212.N.1.
//!
//! Per-invocation runtime context handed to `BoardEntry::run`'s
//! `setup` callback. Carries the overlay knobs the codegen
//! `run_plan(runtime)` body reads:
//!
//! - **params** — `(key, value)` pairs from launch XML
//!   `<param name="…" value="…"/>` or `--ros-args -p k:=v`.
//! - **remaps** — `(from, to)` topic/service renames.
//! - **env** — environment-style key/value pairs (POSIX `getenv`
//!   shape) accessible from no_std boards via this struct rather
//!   than a `libc::getenv` call.
//!
//! ## no_std-safe shape
//!
//! Slice-of-tuples kept on the boot stack. No allocation, no
//! `core::collections`. Codegen owns the storage and passes a
//! `&mut RuntimeCtx<'_>` whose backing slices live in `static`s.
//!
//! Hosted boards (POSIX) may instead build a longer-lived owned
//! variant on the heap; the trait surface is slice-based so
//! both shapes work.

/// Runtime context handed to `BoardEntry::run(setup)`.
///
/// All three slices may be empty. A board's launch overlay typically
/// populates `params` + `remaps`; `env` is rarely set on embedded.
#[derive(Debug)]
pub struct RuntimeCtx<'a> {
    /// `<param name=… value=…/>` from launch XML, or
    /// `-p name:=value` CLI overrides.
    pub params: &'a [(&'a str, &'a str)],

    /// Topic / service / action remaps: `(from, to)`.
    pub remaps: &'a [(&'a str, &'a str)],

    /// Environment-style key/value pairs (mostly POSIX). Empty on
    /// embedded boards.
    pub env: &'a [(&'a str, &'a str)],
}

impl<'a> RuntimeCtx<'a> {
    /// An empty `RuntimeCtx` — no params, no remaps, no env. Useful
    /// as a placeholder when running a launch-less single-node
    /// example, or in unit tests.
    pub const EMPTY: Self = Self {
        params: &[],
        remaps: &[],
        env: &[],
    };

    /// Lookup a param by name; first match wins. Linear scan
    /// because the slice is typically small (≤ a dozen entries).
    pub fn param(&self, name: &str) -> Option<&'a str> {
        self.params
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
    }

    /// Lookup a remap by the original (`from`) name; returns the
    /// rewritten name when remapped, else `None`.
    pub fn remap(&self, from: &str) -> Option<&'a str> {
        self.remaps
            .iter()
            .find(|(k, _)| *k == from)
            .map(|(_, v)| *v)
    }

    /// Lookup an env entry by name.
    pub fn env_var(&self, name: &str) -> Option<&'a str> {
        self.env.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }
}

impl<'a> Default for RuntimeCtx<'a> {
    fn default() -> Self {
        Self::EMPTY
    }
}
