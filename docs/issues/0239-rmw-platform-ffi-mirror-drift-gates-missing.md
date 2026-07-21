---
id: 239
title: "No FFI-mirror drift gate for the RMW vtable/entity ABI nor an exhaustive one for the platform ABI — the #131/#160 stale-mirror class is open on both"
status: open
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
