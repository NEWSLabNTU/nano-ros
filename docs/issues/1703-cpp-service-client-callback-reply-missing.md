---
id: 1703
title: "A native C++ service CLIENT with a callback never receives its reply (zenoh and Cyclone); the C and Rust clients do"
status: open
type: bug
area: [testing, api]
severity: medium
found: 2026-10-06
related: [1700, 1684]
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

## Not diagnosed

It is not in issues 1686–1692 and was not in the 2026-10-05 tier-1 verdict
(run 37252649866), so it is probably recent. Candidates on `main` since then
are #1682 (`rclcpp::Logger` owns its name) and the #1631 executor-release
series. Bisect against the C++ service-client example.

## Acceptance

All six `service_callback` cases in `native_api` pass on fresh fixtures.
