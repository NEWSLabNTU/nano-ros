---
id: 1686
title: "Native C/C++ over XRCE: the action and service round-trips never complete —
  the client reports the server never appeared, and the server prints nothing"
status: resolved
type: bug
area: [rmw, testing]
severity: high
found: 2026-10-05
related: [1651, 1684, 1087, 1710, phase-480]
---

## Failing tests (four, one class)

- `nros-tests::c_xrce_api test_c_xrce_action_fibonacci`
- `nros-tests::native_example_reqresp_e2e native_example_reqresp::case_09_cpp_xrce_service`
- `nros-tests::native_example_reqresp_e2e native_example_reqresp::case_17_c_xrce_action`
- `nros-tests::native_example_reqresp_e2e native_example_reqresp::case_18_cpp_xrce_action`

The XRCE pub/sub cells in the same files pass, so the Agent and the transport
are up; what fails is every request/response round-trip from C and C++.

## Evidence

- CI: workflow_dispatch run 37252649866 (tree of commit
  dc5691a584cfb9f2a53b4b92372370d81ee8cd38), all four red in the full
  `test-all`.
- Local, SOLO (`cargo nextest -j1`, nothing else on the bus), on fixtures
  built 2026-10-05 02:43–03:24 UTC by `just build-test-fixtures lane=tier1`
  from that same tree — so neither load nor a stale fixture (issues
  0859–0862). All four red again.
- `test_c_xrce_action_fibonacci`, client side:

      Action client created: /fibonacci
      Action server did not appear within 10s: -2
      (Is the action server running?)

  and the server's captured output (`--- server ---`) is EMPTY — it printed no
  banner at all in the window, which points at the server process rather than
  at discovery.
- Agent: `~/.nros/sdk/xrce-agent/2.4.3-nros1` (the `nros setup` pin).

Not bisected: this lane had not run `test-all` since 2026-06-17 (issue 1651),
so the window is wide. Start by running the C XRCE action server example by
hand against the pinned Agent and checking whether it reaches its banner.

## Acceptance

The four tests pass solo and in the tier-1 `test-all`.

## Resolution — 2026-10-06 (phase-480 W2)

**The server was fine; the client gave up.** Run by hand against the pinned
Agent (`~/.nros/sdk/xrce-agent/2.4.3-nros1`, `udp4`), the C XRCE action server
reached its banner and served (`Action server created: /fibonacci`, "Waiting
for action goals"), while the client printed

    Action server did not appear within 10s: -2
    (Is the action server running?)

and exited. The test's `--- server ---` block was empty only because
`wait_for_output_pattern` had already consumed the server's banner before
`collect_until` ran, so the lead in this issue pointed at the wrong process.

**Root cause.** XRCE has no server-availability probe by design: `vtable.c`
keeps `service_server_is_available` NULL because the Agent owns the DDS graph,
so every probe answers `Err(Unsupported)`. Issue 1087 made all six wait loops
treat that as "keep waiting" — they spent the whole budget and returned
TIMEOUT — and every C/C++ client example reads TIMEOUT as "no server" and
exits. The Rust service clients never hit it because they call the raw probe
and already treat `Err` as "send anyway". The XRCE pub/sub cells pass because
they wait for nothing.

**Fix** (`fix/1686-xrce-c-cpp-reqresp`):

- One classification for all six loops,
  `nros_node::executor::ServerVisibility::of(probe)`: `Unsupported` means
  "can never answer" and ends the wait at once as `Err(Unsupported)` /
  `NROS_RET_UNSUPPORTED` / `NROS_CPP_RET_UNSUPPORTED` (rcl reports this case
  as an error from `rcl_service_server_is_available`, not as "no"; the graph
  waits already decided the same in phase-444). The loops:
  `handles.rs` `wait_for_service` / `wait_for_action_server`, nros-c
  `nros_client_wait_for_service` / `nros_action_client_wait_for_action_server`,
  nros-cpp's two.
- The one TRANSIENT "cannot say" in the tree — a zenoh graph cache that has
  dropped tokens — is now `WouldBlock`, which still waits, so zenoh keeps its
  behaviour.
- All 16 C/C++ example clients that gate on a wait (native, freertos, nuttx,
  threadx-linux, rv-virt-threadx, baremetal) proceed on UNSUPPORTED; the
  request's own timeout is then the probe. Headers regenerated.

**Before / after** (fixtures rebuilt from each tree, solo, Agent 2.4.3-nros1):

- before (origin/main 52dcf5b5a2): all four red; by hand, "did not appear
  within 10s: -2" with the server serving.
- after (rebased onto 3a4bf2e191): `test_c_xrce_action_fibonacci` PASS 9.5 s,
  `case_09_cpp_xrce_service` 2.4 s, `case_17_c_xrce_action` 5.5 s,
  `case_18_cpp_xrce_action` 5.1 s. Whole `native_example_reqresp_e2e` file:
  every built zenoh and XRCE case passes (C, C++, Rust-XRCE); all five
  `c_xrce_api` tests pass.
- tests: `server_visibility_classifies_the_three_answers`,
  `server_wait_propagates_unsupported_instead_of_waiting_it_out`, and its
  negative control `server_wait_keeps_waiting_through_a_transient_answer`
  (nros-node `graph_wait_tests`).

Sweep: `grep -rn 'is_server_ready()\|service_is_ready(), Ok(true)' packages
--include='*.rs'` and `grep -rn -A1 'wait_for_action_server(\|wait_for_service('
examples --include='*.c' --include='*.cpp'` (16 of 16 sites).

**Not measured / left behind:**

- Not run in the parallel tier-1 `test-all`.
- The 15 RTOS example clients were only COMPILED (tier 2 build), not run over
  XRCE.
- Cyclone rows of `native_example_reqresp_e2e` were not built here.
- Measuring this needed `touch` on the copied `libnros_{c,cpp}.a` after a
  whitespace-only `rustfmt`, because the fixture staleness probe never accepts
  a byte-identical rebuild — filed as issue 1710.
