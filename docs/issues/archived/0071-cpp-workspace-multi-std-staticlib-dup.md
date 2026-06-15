---
id: 71
title: native C/C++ workspace Entry fails on CI — two bundled `std` (libnros_cpp.a + per-package FFI staticlib) collide on `rust_begin_unwind`
status: resolved
type: bug
area: cmake
related: [issue-0057, phase-248, phase-249]
resolved_in: 38c3d89fc
---

## Resolution

The native cpp/mixed example-workspace Entry pkgs failed to link **on CI only**
(`host-integration-tests`): `libnros_cpp.a` and the per-message-package FFI
staticlib (`libnano_ros_cpp_ffi_<pkg>.a`) are **two Rust staticlibs each bundling
`std`**, so both define `rust_begin_unwind` → GNU ld `multiple definition`.

**Root cause:** the `host-integration-tests.yml` "Build workspace fixtures" step set
`CARGO_PROFILE_RELEASE_LTO: "off"` (a disk/speed trim). That env **overrode the
generated FFI crate's `[profile.release] lto = true`** — the crate is `panic="abort"`
and relies on **fat LTO** to DCE-strip the redundant unwinding `std` it never calls.
With LTO off the symbol is retained → the dup. This is why it was CI-only: every dev
/ container build used the FFI crate's own `lto=true`.

Bisected on the FFI staticlib (`nm | grep rust_begin_unwind`): `lto=true` → 0,
`thin` → 1, `off` → 1.

**Fix (`38c3d89fc`):** dropped `CARGO_PROFILE_RELEASE_LTO: "off"` from the
**Build workspace fixtures** step (kept on **Build rust core fixtures** — those are
Rust *binaries*, one `std`, no dup). **CI-confirmed:** dispatched run `27574120427`
built + linked the workspace fixtures; host-integration real failures **4 → 1**
(the remaining `qos_overrides_runtime_delivery` is unrelated phase-250 churn).

Long-term hardening (not done, optional): fold the FFI interface crates into a
single `std`-bearing umbrella staticlib so correctness no longer relies on fat-LTO
DCE.
