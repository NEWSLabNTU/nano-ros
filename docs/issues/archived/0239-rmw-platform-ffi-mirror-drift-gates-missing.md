---
id: 239
title: "No FFI-mirror drift gate for the RMW vtable/entity ABI nor an exhaustive one for the platform ABI — the #131/#160 stale-mirror class is open on both"
status: resolved
type: tech-debt
severity: medium
area: testing
related: [issue-0160, issue-0131, issue-0238]
---

## Finding (RMW/platform API audit, 2026-07-21)

Two C-ABI surfaces are maintained as hand-kept Rust↔C mirrors, and neither
is guarded by the field-parity drift gate that closed the #131/#160
stale-mirror class for the cpp-FFI structs.

1. **RMW vtable + entity mirror.** `nros_rmw_vtable_t`
   (`rmw_vtable.h:44`) ↔ `NrosRmwVtable` (`lib.rs:453`, ~40 fn-pointer
   slots), plus every `NrosRmw*` entity struct (`NrosRmwQos`,
   `NrosRmwSession`, `NrosRmwPublisher`, …) ↔ its `nros_rmw_*_t` header
   twin. `scripts/check-ffi-struct-mirrors.sh` covers ONLY
   `nros_cpp_qos_t` / `nros_cpp_integrity_status_t` between `component.h`
   and `nros_cpp_ffi.h` (lines 22-33) — it does NOT touch the RMW mirror.
   The only guard is a size-count test
   (`nros-rmw-cffi/tests/registry.rs:38` counts
   `size_of::<NrosRmwVtable>()/size_of::<ptr>()`), which catches a slot
   COUNT change but not a per-field type/order divergence (exactly what
   issue #238 is).

2. **Platform ABI mirror.** `platform.h` (76 `nros_platform_*` symbols)
   ↔ the hand-written Rust `extern "C"` block in
   `nros-platform-cffi/src/lib.rs` ↔ the `nros_platform_export_*!` macros
   (a 3-way hand-mirror; adding a symbol needs all three edited). The
   guard `nros-platform-cffi/tests/c_stub_platform.rs` references only ~7
   of the 76 symbols — it is NOT exhaustive.

## Fix direction

Extend `check-ffi-struct-mirrors.sh` (or add a sibling) to cover:
- the `NrosRmwVtable`↔`nros_rmw_vtable_t` slot list (name + signature
  parity) and the `NrosRmw*` entity structs (field parity) — a
  cross-include TU that compiles the header against a Rust-emitted layout
  assertion, like the existing check-c cross-include for the cpp FFI;
- the full `nros_platform_*` symbol set (all 76), so the 3-way mirror can't
  drift silently.

The absence of #1 is what let issue #238's event-kind width mismatch sit
undetected.

## Resolution (2026-07-24)

1. **RMW mirror — gated.** New `scripts/check-rmw-abi-mirror.sh` (hooked as
   `check-rmw-abi-mirror` in `just check`'s fast lane, beside the platform
   gate): compares the ORDERED member-name lists of `nros_rmw_vtable_t` ↔
   `NrosRmwVtable` (36 slots, paren-depth-aware so nested `(*cb)` params
   don't count) and all 8 `nros_rmw_*_t` entity structs ↔ their `NrosRmw*`
   twins. Proven to fire on an injected rename. Plus a COMPILER-DERIVED
   offset/size gate: `tests/c_stubs/abi_offsets.c` exports
   `offsetof`/`sizeof` for all 79 members + 9 struct sizes (computed by
   the C compiler from the headers) and `tests/abi_offsets.rs` compares
   each against `core::mem::offset_of!` on the Rust mirror — both sides
   machine-derived, no shared literal table (regenerate via
   `scripts/gen-rmw-abi-offsets.py`). Proven to fire on a same-size field
   swap that name/size checks cannot see; run by the gate script via
   `cargo nextest -p nros-rmw-cffi --features c-stub-test`. Widths within a same-named
   member stay pinned by the #238 asserts.
   The gate immediately surfaced a LIVE drift: the phase-279 QoS
   `tx_express` split (`u16 _reserved0` → `u8 tx_express + u8 _reserved0`)
   was Rust-only — same total size, so the #238 size gate was blind, and C
   consumers could not request express (a C write to `_reserved0` aliased
   it). Header + the 3 QOS_PROFILE macros fixed.
2. **Platform mirror — was ALREADY gated.** `scripts/check-platform-abi-mirror.sh`
   (phase 121.4.b, in `just check`) extracts EVERY `nros_platform_*` decl
   from all three headers and requires both the extern-"C"-block decl and an
   `nros_platform_export_*!` emission — the finding judged only
   `c_stub_platform.rs` (~7 symbols) and missed this script. No new gate
   needed; name+presence parity across the 3-way mirror is exhaustive.

Accepted residual: per-slot SIGNATURE parity is not text-compared on either
surface; it is covered by the #238 width pins + every in-tree C consumer
compiling against the headers.
