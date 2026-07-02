---
id: 135
title: "Native zenoh service/action query path broken — client get returns Transport(Timeout) instantly, server never receives; reproduced at origin/main"
status: open
type: bug
area: rmw-zenoh
related: [phase-277, phase-268, issue-0096]
---

## Summary

During phase-277 W5 (service/action demo parity), every native zenoh
service round-trip on the dev box fails: the client's query returns
`Transport(Timeout)` essentially instantly (all retries within the same
second, well under the 5 s Promise budget) and the server's queryable never
receives the request. Actions fail the same way (send_goal is a service call
under the hood). This blocks the native zenoh service/action e2e tests
(`services.rs`, `actions.rs`, the zenoh halves of `native_api.rs`).

**Not a W5 regression**: the identical failure reproduces with the
**pre-W5 examples built from `origin/main` (9840b03a6)** in a clean worktree
(0/4 calls, instant timeouts, server logs nothing). Chatter pub/sub over the
same zenohd works; the same W5 examples pass end-to-end over **XRCE** and
**Cyclone DDS** (verified live: `Result of add_two_ints: 5/42`, full
fibonacci feedback + `Result received: [...]`).

## Evidence

- `origin/main` worktree, `service-server`/`service-client` built
  `--features rmw-zenoh`, zenohd 1.7.2 from `build/zenohd/`:
  client logs `Service call failed: Transport(Timeout)` ×4 in <2 s;
  server logs only `Waiting for service requests...`.
- Client debug log shows a sane query keyexpr
  (`0/add_two_ints/example_interfaces::srv::dds_::AddTwoInts_/*`) and the
  SC liveliness token declared; the reply never arrives.
- `wait_for_service` returns `Ok(true)` immediately **even with no server
  running** (the CFFI zenoh path has no real readiness probe), so the
  failure always surfaces as the per-call timeout.
- Two independent phase-277 W5 sub-sessions reproduced the same 0/N result
  with unmodified pre-W5 clients.

## Suspected area

The zenoh query path regressed somewhere on main while pub/sub stayed
healthy. Candidates to bisect:
- `8e6a5cf2a` fix(0096): host zenoh-pico same-session loopback for
  in-process delivery (touches the get/queryable path).
- phase-268 W2/W2b per-entity node identity threading
  (`6601c7e52`, per-call CFFI session views) — services now tag entities
  with per-entity node names.
- The instant (not budget-length) timeout suggests the zenoh-pico `z_get`
  completes with no repliers immediately rather than waiting — i.e. the
  queryable is not matched at the router at query time.

## Impact

- Native zenoh service/action e2e tests fail locally (and presumably in
  remote CI if it runs them on this code): `test_service_request_response`,
  `test_service_multiple_sequential_calls`, `test_service_server_multiple_clients`,
  `test_action_server_client_communication`, plus the zenoh service/action
  halves of `native_api.rs` / `nano2nano.rs`.
- XRCE and Cyclone service/action paths are unaffected (verified live).

## Next step

Bisect main between the last known-green native zenoh service e2e and
`9840b03a6` with the two candidate commits first; instrument the zenoh shim
`get` to log the closing reason (no-replier vs timeout).
