---
id: 1703
title: "A native C++ service CLIENT with a callback never receives its reply (zenoh and Cyclone); the C and Rust clients do"
status: resolved
type: bug
area: [testing, api]
severity: medium
found: 2026-10-06
related: [1700, 1684, 1667]
resolved: 2026-10-06
---

## What was measured

In a `just ci tier1 run` over a complete, freshly built `lane=tier1` fixture
set (`origin/main` 2026-10-06 plus the #1700 fix), three tests in
`native_api` failed. Re-run SOLO (`-j1`) on the same fixtures, two of them
fail deterministically:

| test | result |
| --- | --- |
| `test_native_cyclonedds_service_callback::lang_2_Language__Cpp` | FAIL |
| `test_service_callback_interop_cpp_client_c_server` | FAIL |
| `test_native_cyclonedds_service_callback::lang_1_Language__C` | pass |
| `test_service_callback_interop_c_client_cpp_server` | pass |
| `test_service_callback_interop_rust_client_{c,cpp}_server` | pass |

The third, `test_native_service_communication_callback::lang_2_Language__Cpp`
(zenoh), failed in the sweep and was not part of the solo filter.

Failure text (`native_api.rs:381`):

    Expected the callback-dispatched `Result of add_two_ints: 5` reply cross-language.

So the C++ **client** side fails and the C++ **server** side works (a C client
against a C++ server passes).

## Root cause

The reply never arrived because the request was never sent. Run by hand
against a C server, the C++ client printed `Async send failed with error -3`
(`INVALID_ARGUMENT`) immediately. The harness only grepped for the missing
`Result of` line, which is why the symptom looked like a dropped reply.

phase-476 W0 (`0fe6a17db`, issue 1667) changed the handle an FFI is issued
(`HandleId::to_raw`) from a bare slot index to `(generation << 16) | slot`.
nros-cpp stores that value as `handle_id_` and passes it back on every
call. `nros_cpp_service_client_send_on_handle` gave it to
`Executor::service_client_entry_mut(entry_index)`, which still indexed
`entries` with it directly. A packed value is past the end of the table, so
the result was `None` and the send returned `INVALID_ARGUMENT`. C clients
passed because nros-c stores the bare `slot()` in `arena_entry_index`. Rust
clients passed because they hold a typed handle.

The same mismatch was in all five by-index accessors:
`service_client_entry_mut`, `service_client_handle`, `service_server_handle`,
`subscription_handle` and `action_client_core_mut`. That covers every
nros-cpp granted-QoS read-back and service-available probe, plus nros-c's
`nros_subscription_get_actual_qos`, which also stores the packed value.

## Fix

One helper, `Executor::slot_of_raw`, decodes the argument through
`HandleId::from_raw` and resolves it with `resolve_handle`. All five
accessors use it, so both spellings work:

- A bare slot has generation 0, which `resolve_handle` already treats as an
  owned slot.
- A packed handle resolves with its generation checked.
- A stale handle resolves to `None`, so issue 1667's guarantee now covers
  these accessors too.

## Proof

- **Unit test:** `by_index_accessors_resolve_a_packed_handle_and_a_bare_slot`
  (nros-node). It passes with the fix and fails with the helper mutated back
  to `Some(entry_index)`.
- **By hand:** the C++ callback client against the C server prints
  `Result of add_two_ints: 5` and exits 0.
- **`native_api service_callback`, re-run solo (`-j1`):** both 1703 cases now
  pass (`cyclonedds_service_callback::Cpp`, `interop_cpp_client_c_server`).
  The two Rust-client cases resolved a STALE fixture, because the nros-node
  edit made their prebuilt binaries out of date. They need a fixture rebuild;
  this is not a regression.

Sweep (call sites that pass an FFI-held value to a by-index accessor):
`git grep -n 'service_client_entry_mut(\|subscription_handle(\|service_server_handle(\|service_client_handle(\|action_client_core_mut(' -- packages/api`

## Acceptance

All six `service_callback` cases in `native_api` pass on fresh fixtures.
