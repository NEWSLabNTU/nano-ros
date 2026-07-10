---
id: 174
title: "Zephyr (native_sim) XRCE C/C++ lanes deliver nothing though the agent starts"
status: resolved
type: bug
area: rmw
related: [issue-0164, issue-0163, phase-286]
resolved_in: phase-286 W3
---

## Resolution (phase-286 W3, 2026-07-10)

Not a delivery bug — a missing agent locator, the C/C++ analog of #163 (which
fixed only the Rust images). The C/C++ XRCE entry opened its session with NO agent
address: `NROS_ENTRY_LOCATOR` (`nros-cpp/main.hpp`) only read
`CONFIG_NROS_ZENOH_LOCATOR` (unset for XRCE) → `""` → the XRCE transport never
connected (`run_components` rc=-100 `TRANSPORT_ERROR`). Fix: `main.hpp` synthesizes
the bare `host:port` from `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` on an XRCE build
(adjacent string-literal concat + stringize) — the form the XRCE session parser
accepts; covers C + C++ (both route through `ZephyrBoard::run_components`). Three
stale test markers surfaced once the transport connected (#164 class): the two
`xrce_*_action` server-ready greps (`"Waiting for goals"`/`"Waiting for goal"` →
`ACTION_SERVER_READY_MARKER`) and `xrce_cpp_action`'s bogus `feedback >= 1` +
literal `"Feedback"` requirement (the Fibonacci server completes with a result and
streams no feedback — gate on `ACTION_RESULT_PREFIX`). **All 6 XRCE C/C++ lanes
green** (`xrce_{c,cpp}_{talker_listener,service,action}`).

## Problem

The Zephyr native_sim **XRCE C and C++** e2e lanes deliver nothing end-to-end,
even though the Micro-XRCE-DDS Agent starts. #163 fixed the pure-**Rust** XRCE
images (they now pass); the `libnros_c` XRCE path is untouched and does not
deliver.

## Evidence (2026-07-09 family re-run, post-#163)

- `test_zephyr_xrce_c_service_e2e` / `test_zephyr_xrce_cpp_service_e2e` —
  `client OK=0, server requests=0` / "got no reply".
- `test_zephyr_xrce_c_talker_listener` / `test_zephyr_xrce_cpp_talker_listener`
  and the `_action` variants — same 0-delivery.
- The agent DOES start (log: "Starting XRCE Agent on port 2038/2028…"), so this
  is NOT an agent-missing / port skip.
- Contrast: `test_zephyr_xrce_rust_{talker_listener,service,action}` now PASS
  (the #163 XRCE `host:port` locator bake + force-link register), and the C/C++
  boot tests pass — so the C/C++ images build, link, and boot; only runtime
  XRCE delivery fails.

## Suspects / direction

- The `libnros_c` XRCE session bring-up on Zephyr native_sim (transport init /
  agent handshake / entity declare) — the Rust and C paths diverge in how the
  XRCE session + streams are created; #163 only touched the Rust side.
- Triage first against the canonical markers (this is post the #164 marker sweep,
  so the greps are correct — the 0-delivery is real, not a stale assertion).
- Compare the C vs Rust XRCE client's agent traffic (the agent's own log / a
  capture) for the same topic to see whether the C client ever completes
  `create_session` / `create_datawriter`.

## References

`packages/testing/nros-tests/tests/zephyr.rs` (`test_zephyr_xrce_{c,cpp}_*`),
issue #164 (re-triage), issue #163 (the Rust-side XRCE fix this does NOT cover),
`packages/xrce/`.
