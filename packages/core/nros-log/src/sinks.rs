//! Sinks the FACADE itself can offer.
//!
//! issue 0710 — `PlatformSink` used to live here, and with it a hand-declared
//! `unsafe extern "C" { nros_platform_log_write }`. Both moved to
//! `nros_platform_cffi::log`, for a reason worth stating where the old code
//! was:
//!
//! this crate is a facade. `LogSink` + [`crate::init`] exist so delivery is
//! PLUGGABLE, and a sink that speaks the platform ABI is a LINK-TIME
//! requirement on the final binary. While it lived here, "does this binary need
//! that ABI?" was answerable only by a Cargo feature — and feature unification
//! makes that unanswerable in a workspace build (`nros-platform-cffi` and
//! `nros-rmw-bridge` enable `nros-node/rmw-cffi` unconditionally, so any
//! forwarded gate is ON for every member). A dependency IS a property of the
//! binary; a feature is a property of the build.
//!
//! It also means the extern is declared exactly once, in
//! `nros-platform-cffi`'s bindgen output from `<nros/platform.h>` — the SSoT
//! RFC-0054 names — rather than a second time by hand over here.
//!
//! Callers want:
//!
//! ```ignore
//! nros_log::init(nros_platform_cffi::log::default_sinks());
//! ```
//!
//! Records raised before any `init` are not lost: [`crate::early`] holds them
//! and `init` drains them into whatever sinks arrive.
