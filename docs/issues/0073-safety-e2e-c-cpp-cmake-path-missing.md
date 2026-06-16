---
id: 73
title: safety-e2e is Rust-only — no C/C++/CMake lowering (CRC dead on C/C++ embedded)
status: open
type: tech-debt
area: build
related: [issue-0072, phase-252, rfc-0031]
---

> **Core capability landed (2026-06-16).** The C/C++ API can now validate CRC:
> - `nros-c` `safety-e2e` feature → forwards to the zenoh backend's `safety-e2e`
>   (`["nros/safety-e2e", "nros-rmw-zenoh?/safety-e2e"]`).
> - `NANO_ROS_SAFETY_E2E` CMake option (zenoh-only; warns otherwise) → appends the feature.
> - **C ABI:** `nros_integrity_status_t { i64 gap; bool duplicate; i8 crc_valid }` +
>   `nros_subscription_try_recv_validated(sub, buf, len, *out_status)` (subscription.rs),
>   backed by `RawSubscription::try_recv_validated` (nros-node handles.rs). cbindgen emits
>   both into `nros_generated.h`; symbol exported in the safety-built `libnros_c.a` (verified
>   by `nm`). Both feature states compile.
>
> **C transport e2e — DONE (2026-06-16).** `examples/native/c/safety-listener` (polls
> `nros_subscription_try_recv_validated`, CMakeLists forces `NANO_ROS_SAFETY_E2E=ON`) +
> `tests/safety_e2e.rs::test_c_safety_listener_validates_crc` (vs the safety talker over
> zenohd). Verified green: **`c safety: 3 crc-ok, 0 crc-fail`** — the C API validates the
> publisher's CRC end-to-end.
>
> **Remaining (follow-up):** (1) config-driven auto-lowering — `[safety]` →
> `-DNANO_ROS_SAFETY_E2E` + a `#define NROS_SYSTEM_SAFETY_E2E` in `system_config.h` (needs a
> `[system].safety` bridge, since phase-250's `[safety]` is an nros.toml overlay block, not
> `SystemHeader`); (2) a C++ path — `nros_cpp_subscription_try_recv_validated` (the nros-cpp
> ABI is separate from nros-c) + a `subscription.hpp` `.take_validated()` wrapper; (3)
> cyclonedds has no safety path at all (document/gate).

## Why

The `safety-e2e` capability axis (E2E message integrity — CRC-32 attach on publish +
validate on receive) is **Rust-only** today. A C or C++ application linking the zenoh
backend does **not** get CRC validation: there is no C/C++ build lowering for the axis and
the CRC machinery is feature-gated inside the **Rust** zpico shim
(`packages/zpico/nros-rmw-zenoh/src/shim/{publisher,subscriber}.rs`, `#[cfg(feature =
"safety-e2e")]`). This is the C/C++ analog of [issue 0072](0072-safety-e2e-backend-feature-not-lowered.md)
(which covered the Rust entry / native-backend / board lowering), split out because it is a
deeper, separate piece.

## Evidence

- Zero `NROS_SAFETY` / `safety_e2e` / `safety-e2e` tokens in any C/CMake/header file
  (`grep` over `*.c` / `*.h` / `*.cmake` / `CMakeLists.txt` — nil).
- RMW lowers to C/C++ via `resolve_rmw`'s `cmake_value` (`-DNANO_ROS_RMW=<x>`) +
  `c_define_token` (`#define NROS_SYSTEM_RMW_<TOK>` in `system_config.h`, emitted by
  `render_system_config_h` in `codegen_system.rs`). The capability registry
  (`cargo-nano-ros/src/capability_resolver.rs`, phase-252) reserves the parallel
  `cmake_token` / `c_define` slots but they are `None` for `safety` today.

## Scope

1. **Decide the C/C++ semantics.** Is `safety-e2e` even reachable for a C/C++ binary that
   links the (Rust) zenoh shim — i.e. does the Rust-side CRC run regardless of the C app, or
   must the C app opt a subscription in? The CRC attach/validate is in the Rust shim, so a
   C/C++ app on the zenoh backend may need the backend built with `safety-e2e` (the phase-252
   board/backend lowering already enables it on the Rust dep) — confirm whether that alone
   makes C/C++ validate, or a C-visible surface is also needed.
2. **C define (if a surface is needed).** Extend `render_system_config_h` to emit
   `#define NROS_SYSTEM_SAFETY_E2E` when a `[safety]` block is declared in the bringup
   `system.toml`, using the registry's `c_define` slot (mirroring `NROS_SYSTEM_RMW_<TOK>`).
3. **CMake (if a link/compile knob is needed).** A `-DNANO_ROS_SAFETY_E2E=ON` analog via the
   registry's `cmake_token`, threaded through the C/C++ codegen (`codegen_system.rs` /
   `NanoRos*.cmake`).
4. **zpico-C gate.** If the C/C++ path needs the CRC at the C layer (not just the Rust shim),
   surface a zpico-C safety gate.
5. **cyclonedds.** The C++ DDS backend has no safety path at all — document or gate.

## References

- RFC-0031 § "Generalization (Phase 250 / issue 0072)" — the capability-axis lowering model,
  with the C/C++ slots called out as reserved.
- phase-252 Wave 5 — names this as its own issue.
