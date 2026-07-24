# Phase 299 — C headers as the ABI SSoT (implements RFC-0054)

Flip the two hand-mirrored C ABI surfaces (RMW backend contract, platform
port ABI) to the RFC-0054 model: pure C header packages are the single
source of truth; Rust consumes COMMITTED bindgen output; the hand mirrors
and their drift gates retire. Closes the #131/#160/#238/#239 drift class by
construction, including the per-slot signature residual #239 documented.

Design decisions + rationale → [RFC-0054](../design/0054-c-header-abi-ssot.md)
(brainstormed 2026-07-24).

## Work items

### W1 — RMW surface
- [ ] W1.1 Extract `packages/core/nros-rmw-abi/`: move
  `nros-rmw-cffi/include/nros/rmw_*.h` + Doxyfile; add a CMake INTERFACE
  target; repoint every C/C++ include path (cyclonedds, uorb, xrce-cffi
  build.rs, cyclonedds-sys build.rs, zephyr module, CMakeLists).
- [ ] W1.2 `scripts/gen-abi-bindings.sh` + committed
  `nros-rmw-cffi/src/generated.rs` (pinned bindgen-cli; `--use-core`,
  `ctypes-prefix core::ffi`, moduleconsts enums, allowlist `nros_rmw_.*`,
  layout tests on, version stamp).
- [ ] W1.3 `lib.rs` surgery: delete the hand type definitions, re-export
  generated items, add compat aliases (`NrosRmwVtable`, `NrosRmwQos`, …),
  keep constants/helper impls. Migrate doc comments into the headers where
  they aren't already.
- [ ] W1.4 Fix consumer churn: vtable-authoring sites (`nros-rmw-zenoh`,
  xrce/cyclonedds adapters, `rust_adapter.rs`) to the generated fn-ptr
  signatures (`*const c_char` etc.). All 8 dependent crates green.
- [ ] W1.5 Retire the rmw drift gates: `check-rmw-abi-mirror.sh`,
  `tests/abi_offsets.*`, `gen-rmw-abi-offsets.py`, the rmw `abi_layout`
  const block + `abi_layout_check.c` width asserts (keep any that guard
  C-side-only invariants). justfile hooks updated.

### W2 — platform surface
- [ ] W2.1 Generate the `nros-platform-cffi` extern-"C" declaration block
  from `platform*.h` (same committed-bindgen path; allowlist
  `nros_platform_.*`).
- [ ] W2.2 Shrink `check-platform-abi-mirror.sh` to the macro-emission
  half (`nros_platform_export_*!` presence); extern-block parity is now
  by construction.

### W3 — regen-diff gate
- [ ] W3.1 `check-abi-bindings` in the `just check` lane: rerun
  `gen-abi-bindings.sh`, `git diff --exit-code` the generated files;
  loud skip when bindgen-cli absent; CI provisions the pinned version.

### W4 — docs
- [ ] W4.1 RFC-0054 → Stable once W1–W3 land; CLAUDE.md pitfall-index
  one-liner (headers = SSoT, regen script, never hand-edit generated.rs);
  AGENTS.md C/C++-integration note; issue cross-links (#238/#239
  archived notes point here).

## Acceptance
- `rg "pub struct NrosRmw(Vtable|Qos|Session)" packages/core/nros-rmw-cffi/src/lib.rs`
  finds nothing (definitions only in generated.rs).
- `gen-abi-bindings.sh` twice = idempotent; gate fails on an injected
  header edit without regen.
- `just ci` green; ASI + example matrix unaffected (ABI layout unchanged —
  this phase moves definitions, it does not change them).

## Out of scope
- Board ABI (`nros-board-cffi/include/nros/board.h`) — same treatment
  later if this proves out.
- Any ABI layout/semantic change.
