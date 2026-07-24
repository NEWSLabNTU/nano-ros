---
rfc: 0054
title: "C headers as the ABI SSoT — pure header packages + committed-bindgen Rust bindings"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-299]
supersedes: []
superseded-by: null
---

# RFC-0054 — C headers as the ABI SSoT

## Problem

The two hand-written C ABI surfaces — the RMW backend contract
(`rmw_{vtable,entity,event,ret,transport}.h`) and the platform port ABI
(`platform{,_net,_timer}.h`) — were maintained as **hand-kept Rust↔C
mirrors**. The mirror class produced real drift three times (#131/#160
cpp-FFI appends, #238 event-kind width, #239's live QoS `tx_express`
split), each patched with a progressively stronger *check* (size asserts,
name/order parity, compiler-derived offset parity). Checks shrink the
drift window; only a single definition removes it. The residual even after
#239 — per-slot function-pointer *parameter types* at identical
name+offset — is exactly the part text checks cannot see.

## Decision

**The C headers are the single source of truth. Rust consumes committed
bindgen output. The hand mirror is deleted.**

Three sub-decisions (brainstormed 2026-07-24):

1. **Rust consumption = committed bindgen.** `bindgen` runs OFFLINE
   (maintainer/CI), its output is committed as `src/generated.rs`, and a
   regen-diff gate keeps it honest. Rationale against the alternatives:
   build-time bindgen makes libclang a hard dependency of every
   embedded/cross build and inherits host-libclang nondeterminism;
   keeping the gated hand mirror leaves the signature residual open.
   With committed output, embedded builds see plain checked-in Rust.
2. **Two packages, not one.** A new pure-header
   `packages/core/nros-rmw-abi/` (headers extracted from
   `nros-rmw-cffi`); `packages/core/nros-platform-api/` remains the
   platform header home (it already carries the headers). One package
   per contract keeps consumer include-paths and ownership stories
   separate.
3. **Generated bindings ARE the public types.** `nros-rmw-cffi`
   re-exports the generated items; compat `pub type` aliases
   (`NrosRmwVtable = nros_rmw_vtable_t`, …) keep most of the 8 dependent
   crates compiling; churn concentrates at FFI-string params
   (`*const u8` → `*const core::ffi::c_char`) in vtable-authoring sites.
   Doc comments live in the headers; bindgen carries `/** */` through to
   rustdoc, so docs are single-sourced too.

## Design

### Packages

- `packages/core/nros-rmw-abi/` — `include/nros/rmw_*.h`, a CMake
  INTERFACE library (`NanoRos::RmwAbi`), the RMW Doxyfile. No build
  logic, no Rust.
- `packages/core/nros-platform-api/` — unchanged home for
  `include/nros/platform*.h` (plus its existing small Rust crate).

### Rust side

- `nros-rmw-cffi/src/generated.rs` — committed bindgen output.
  Flags: `--use-core`, `--ctypes-prefix core::ffi`,
  `--default-enum-style moduleconsts` (matches the existing
  `NROS_RMW_RET_*` const style), allowlist `nros_rmw_.*`, layout tests
  ON (bindgen's generated `#[test]` layout asserts remain as
  belt-and-braces). Version-stamped header comment records the exact
  bindgen version.
- `nros-rmw-cffi/src/lib.rs` — keeps constants, helper impls and the
  idiomatic compat aliases; no type *definitions* for mirrored items.
- `nros-platform-cffi` — the hand `unsafe extern "C" { … }` declaration
  block is replaced by generated declarations the same way. The
  `nros_platform_export_*!` macros stay hand-written: they *emit*
  definitions (the port side), which codegen cannot do.

### Regen + gates

- `scripts/gen-abi-bindings.sh` — regenerates both `generated.rs` files
  with a PINNED bindgen-cli version; fails if the installed version
  differs from the pin.
- **Regen-diff gate** (`check` lane): rerun bindgen, `git diff
  --exit-code` the generated files. Skips with a loud warning when
  bindgen-cli is absent (CI installs it; embedded consumers never need
  it).
- **Retired once each wave lands**: `check-rmw-abi-mirror.sh` (name
  parity + offset test — nothing left to drift against),
  the rmw half of the #238 `abi_layout` asserts, and the extern-block
  half of `check-platform-abi-mirror.sh` (its macro-emission half
  survives).

## Consequences

- The #131/#160/#238/#239 drift class is closed by construction for both
  surfaces; signature parity included.
- Adding an ABI member = edit the header, run one script, fix what the
  compiler now flags. One definition, one diff.
- Costs: bindgen version-pin discipline (mitigated by the stamp + diff
  gate); one-time `c_char` churn in Rust backends; C-side naming
  (`nros_rmw_*_t`) becomes the canonical Rust naming with idiomatic
  aliases on top.
