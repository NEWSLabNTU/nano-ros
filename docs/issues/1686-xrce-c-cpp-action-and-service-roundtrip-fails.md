---
id: 1686
title: "Native C/C++ over XRCE: the action and service round-trips never complete —
  the client reports the server never appeared, and the server prints nothing"
status: open
type: bug
area: [rmw, testing]
severity: high
found: 2026-10-05
related: [1651, 1684]
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
