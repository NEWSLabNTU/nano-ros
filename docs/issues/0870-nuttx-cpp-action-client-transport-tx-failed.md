---
id: 870
title: "NuttX C++ action client fails `create_action_client` — the session
  reports `Transport(ConnectionFailed)` against a router the server reached"
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

## Measured: the error was hidden behind THREE layers of collapse

The guesses above (TX buffer sizing, queryable pool capacity) were both wrong,
and so was the title. They were reached by reading return codes that lie. Three
separate seams each replaced a typed error with a less specific one:

1. `nros_cpp_action_client_create` — `Err(_) => NROS_CPP_RET_TRANSPORT_ERROR`,
   discarding a typed `NodeError`. Fixed: it now calls `node_error_to_cpp_ret`,
   which already existed and already prints the variant (issue 0557 built it for
   exactly this collapse, one layer in).
2. That revealed `NodeError::ActionCreationFailed` — itself a flattening of 17
   `session.create_*` sites in `executor/action.rs`, every one
   `map_err(|_| NodeError::ActionCreationFailed)`. `NodeError` ALREADY carries
   `Transport(TransportError)`; nothing needed adding, the error was simply not
   passed on. Swept all 17.
3. Which finally names it:

       [ERROR] nros: NodeError::Transport(ConnectionFailed)

So `-100` was accidentally in the right FAMILY and useless about the cause: not
a TX failure, a **connect** failure. The session cannot establish its link when
the client declares its entities — while the action SERVER, on the same port and
the same router, connected fine and printed its banner.

## What that reframes

The server reaching the router proves the router is up and reachable on that
port, so this is not "the router was not started". Two QEMU guests each connect
outward through their own slirp stack to `10.0.2.2:<port>`; the second one
fails. Candidates, none tested:

* the client's connect deadline is too short for a loaded host (two arm-virt
  QEMUs under `-icount`), so the TCP connect times out and surfaces as
  `ConnectionFailed`;
* something about the second guest's slirp path to the same host port.

Note this is NOT the 0867 ordering bug — that fix (start the client only after
the server's banner) is in, and this cell still fails. If anything the ordering
makes the client start LATER, so a connect-deadline theory has to explain why
later is not better.

## Acceptance

* The cell passes on its FIRST attempt, repeatably, on an idle host.
* MET ALREADY: the failure names its own cause rather than reporting a generic
  transport error — that half is fixed and is worth keeping independently of the
  connect bug, since it is what made the connect bug findable at all.
