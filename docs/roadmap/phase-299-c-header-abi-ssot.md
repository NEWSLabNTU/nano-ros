# Phase 299 — C headers as the ABI SSoT (implements RFC-0054)

**Status (2026-07-24): ALL WAVES DONE.** RFC-0054 Stable. Fire-proofs: the
regen-diff gate fails on an un-regenerated header const; bindgen layout
tests removed after the embedded-clippy lane proved them host-only (32-bit
targets failed const-eval on 64-bit literals).

Flip the two hand-mirrored C ABI surfaces (RMW backend contract, platform
port ABI) to the RFC-0054 model: pure C header packages are the single
source of truth; Rust consumes COMMITTED bindgen output; the hand mirrors
and their drift gates retire. Closes the #131/#160/#238/#239 drift class by
construction, including the per-slot signature residual #239 documented.

Design decisions + rationale → [RFC-0054](../design/0054-c-header-abi-ssot.md)
(brainstormed 2026-07-24).

## Work items

### W1 — RMW surface
- [x] W1.1 (2026-07-24) Extract `packages/core/nros-rmw-abi/`: move
  `nros-rmw-cffi/include/nros/rmw_*.h` + Doxyfile; add a CMake INTERFACE
  target; repoint every C/C++ include path (cyclonedds, uorb, xrce-cffi
  build.rs, cyclonedds-sys build.rs, zephyr module, CMakeLists).
- [x] W1.2 (2026-07-24) `scripts/gen-abi-bindings.sh` + committed
  `nros-rmw-cffi/src/generated.rs` (pinned bindgen-cli; `--use-core`,
  `ctypes-prefix core::ffi`, moduleconsts enums, allowlist `nros_rmw_.*`,
  layout tests on, version stamp).
- [x] W1.3 (2026-07-24) `lib.rs` surgery: delete the hand type definitions, re-export
  generated items, add compat aliases (`NrosRmwVtable`, `NrosRmwQos`, …),
  keep constants/helper impls. Migrate doc comments into the headers where
  they aren't already.
- [x] W1.4 (2026-07-24) Fix consumer churn: vtable-authoring sites (`nros-rmw-zenoh`,
  xrce/cyclonedds adapters, `rust_adapter.rs`) to the generated fn-ptr
  signatures (`*const c_char` etc.). All 8 dependent crates green.
- [x] W1.5 (2026-07-24) Retired: check-rmw-abi-mirror.sh, gen-rmw-abi-offsets.py,
  tests/abi_offsets.{rs,c} + build.rs block, the Rust `abi_layout` const
  asserts, the justfile hook. KEPT `abi_layout_check.c` — its
  `_Static_assert`s guard the HEADER's own layout stability (C-side-only
  invariant, not mirror parity). Extras landed with W1: dead
  `nros_rmw_cffi_walk_init_section` decl removed from rmw_vtable.h (walker
  deleted in phase-249 — latent undefined-symbol trap);
  `--default-macro-constant-type signed` so `NROS_RMW_RET_OK` types i32
  (no typed shadow needed). Retire the rmw drift gates (detail superseded above): `check-rmw-abi-mirror.sh`,
  `tests/abi_offsets.*`, `gen-rmw-abi-offsets.py`, the rmw `abi_layout`
  const block + `abi_layout_check.c` width asserts (keep any that guard
  C-side-only invariants). justfile hooks updated.

### W2 — platform surface
- [x] W2.1 (2026-07-24) Generate the `nros-platform-cffi` extern-"C" declaration block
  from `platform*.h` (same committed-bindgen path; allowlist
  `nros_platform_.*`).
- [x] W2.2 (2026-07-24) Shrink `check-platform-abi-mirror.sh` to the macro-emission
  half (`nros_platform_export_*!` presence); extern-block parity is now
  by construction.

### W3 — regen-diff gate
- [x] W3.1 (2026-07-24) `check-abi-bindings` in the `just check` lane: rerun
  `gen-abi-bindings.sh`, `git diff --exit-code` the generated files;
  loud skip when bindgen-cli absent; CI provisions the pinned version.

### W4 — docs
- [x] W4.1 (2026-07-24) RFC-0054 → Stable once W1–W3 land; CLAUDE.md pitfall-index
  one-liner (headers = SSoT, regen script, never hand-edit generated.rs);
  AGENTS.md C/C++-integration note; issue cross-links (#238/#239
  archived notes point here).

### W5 — board surface (2026-07-24, follow-on request)
- [x] W5.1 `board.h` gains `NROS_BOARD_NORETURN` (noreturn is contract;
  bindgen `--enable-function-attribute-detection` carries it as `-> !`).
- [x] W5.2 generated declarations (`nros-board-cffi/src/generated.rs`,
  board section in gen-abi-bindings.sh); hand extern block deleted;
  `nros_board_export!` stays hand-written.
- [x] W5.3 `check-board-abi-mirror.sh` extern half → allowlist-completeness
  vs generated.rs; board file added to the `check-abi-bindings` diff set.

## Acceptance
- `rg "pub struct NrosRmw(Vtable|Qos|Session)" packages/core/nros-rmw-cffi/src/lib.rs`
  finds nothing (definitions only in generated.rs).
- `gen-abi-bindings.sh` twice = idempotent; gate fails on an injected
  header edit without regen.
- `just ci` green; ASI + example matrix unaffected (ABI layout unchanged —
  this phase moves definitions, it does not change them).

## Out of scope
- Any ABI layout/semantic change.
