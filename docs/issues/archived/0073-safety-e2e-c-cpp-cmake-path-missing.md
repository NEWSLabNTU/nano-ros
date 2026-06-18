---
id: 73
title: safety-e2e is Rust-only — no C/C++/CMake lowering (CRC dead on C/C++ embedded)
status: resolved
type: tech-debt
area: build
related: [issue-0072, phase-252, phase-254, rfc-0031]
resolved_in: "Phase 252/254 — C/C++ safety-e2e lowering"
---

## Resolution (2026-06-16, follow-up phase-254)

The `safety-e2e` axis (CRC-32 attach on publish + validate on receive) now lowers
to the C **and** C++ APIs; verified end-to-end. Scope items, all done:

- **C ABI:** `nros_integrity_status_t { i64 gap; bool duplicate; i8 crc_valid }`
  + `nros_subscription_try_recv_validated(sub, buf, len, *out_status)`
  (`nros-c/src/subscription.rs`), backed by `RawSubscription::try_recv_validated`.
  cbindgen emits both into `nros_generated.h`; symbol exported in the
  safety-built `libnros_c.a` (`nm`-verified).
- **C++ ABI:** `nros_cpp_integrity_status_t` +
  `nros_cpp_subscription_try_recv_validated` (`nros-cpp/src/subscription.rs`) →
  `nros_cpp_ffi.h`; `Subscription<M>::try_recv_validated(msg, status)` wrapper.
- **CMake knob:** `NANO_ROS_SAFETY_E2E` option on nros-c + nros-cpp (zenoh-only —
  warns + ignored on other RMWs); appends the `safety-e2e` feature.
- **Config lowering (phase-254):** `[safety]` is a typed `system.toml` field read
  by both codegen paths; `render_system_config_h` emits
  `#define NROS_SYSTEM_SAFETY_E2E` (+ `NROS_SYSTEM_PARAM_SERVICES`) for C/C++
  conditional compile.
- **cyclonedds / XRCE:** no CRC path → the option warns + is ignored; documented
  in `docs/reference/cyclonedds-known-limitations.md`.

**Proven:** native-C transport e2e `tests/safety_e2e.rs::
test_c_safety_listener_validates_crc` (vs the safety talker over zenohd) green —
`c safety: 3 crc-ok, 0 crc-fail`. The C++ ABI calls the same
`RmwSubscriber::try_recv_validated`. Config knob = the CMake flag, the
C/C++-build analog of `NROS_RMW`.
