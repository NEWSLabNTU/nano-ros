---
id: 870
title: "NuttX C++ action client fails `create_action_client` with -100
  (transport TX failed) on roughly two runs in three"
status: open
type: bug
area: rmw, examples
related: [issue-0867, issue-0865, issue-0460]
---

## Symptom

`test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`, run alone
on an idle host (load 0.53), fails its first two attempts and passes the third:

    Waiting for action goals (Ctrl+C to exit)...          # server is up
    [nros] examples/qemu-arm-nuttx/cpp/action-client/src/main.cpp:40
           node.create_action_client(client, "/fibonacci") -> -100

`-100` is `NROS_CPP_RET_TRANSPORT_ERROR` / `_Z_ERR_TRANSPORT_TX_FAILED`
(`nros_cpp_ffi.h:585`, `result.hpp:67`): the transport could not TRANSMIT. So
this is not the client failing to find the server — it is the client's own
declaration failing to leave the box.

## Not issue 0867, and not fixed by it

0867 is the C client's `Failed to send goal: -2` (`NROS_RET_TIMEOUT`), caused by
the client being started alongside the server and asking before the server's
queryable existed; the fix orders the request/response start on the server's
readiness banner. That fix is verified and it does help this cell — nuttx C++
action passed at 23.5 s in one 9-cell run — but the `-100` predates it and
survives it, and it is a different failure at a different point: 0867's client
gets as far as `Sending goal`, this one never finishes construction.

Both were present before either fix, which is why they were easy to conflate:
the same cell produced `-2` and `-100` on different runs.

## What is known

* Reproduces solo on an idle host, so it is not the host-load class of 0865.
* Roughly 2 failures in 3 attempts, and nextest's retries mask it — the cell is
  reported FLAKY rather than failing, so it has been passing CI on its third try.
* The server side is healthy and prints its banner every time.
* C on the same board and the same transport does not hit it.

## Where to look

`_Z_ERR_TRANSPORT_TX_FAILED` on a DECLARATION suggests the zenoh-pico session's
TX path is not ready, or is out of a resource, at the moment the C++ binding
declares the action client's entities. An action client is several entities at
once (goal / cancel / result queries plus feedback and status subscriptions),
declared back-to-back — a burst the C client does not produce identically.

Two candidates, neither yet tested:

* TX buffer / batch sizing on the zenoh-pico session during a declaration burst.
* Queryable and subscriber pool capacity — `ZPICO_MAX_QUERYABLES` is 8 embedded
  and `[param_services]` + `[lifecycle]` claim slots before the app declares
  anything (issue 0460). An exhausted pool surfacing as a TX failure rather than
  as `-6` (`NROS_RET_FULL`) would also explain why the error names the transport.

## Acceptance

* The cell passes on its FIRST attempt, repeatably, on an idle host.
* Whatever ran out is named by the error rather than reported as a generic
  transport failure.
