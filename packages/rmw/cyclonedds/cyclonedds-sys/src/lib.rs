//! `cyclonedds-sys`.
//!
//! Build-only sys crate. Vendors `third-party/dds/cyclonedds` via the
//! `cmake` build-script crate. Downstream crates do **not** import any
//! Rust API from here — they pick up `libddsc.a`, the Cyclone include
//! dir, and the host `idlc` binary via Cargo's `DEP_DDSC_*` env vars
//! (emitted from `build.rs` as `cargo:include=…` / `cargo:idlc=…`,
//! plus the canonical `cargo:rustc-link-search=…` + `cargo:rustc-link-lib=…`).

#![no_std]
