---
id: 867
title: "`test_rtos_action_e2e` nuttx/C fails 3/3 SOLO — the client's goal send
  times out (-2) against a server sitting at its ready banner"
status: open
type: bug
area: testing, rmw
related: [issue-0865, issue-0854, issue-0460]
---

## Symptom

`test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C` fails all
three attempts when run **entirely alone**, at ~72–92 s per attempt:

    nros C Action Client (Fibonacci)
    Locator: tcp/10.0.2.2:8320
    Domain ID: 0
    Support initialized
    Node created: fibonacci_action_client
    Action client created: /fibonacci
    Sending goal
    Failed to send goal: -2
    (Is the action server running?)

    thread '...lang_2_Lang__C' panicked: nuttx c action E2E failed:
    accepted=false, completed=false

The server side reaches `Waiting for action goals (Ctrl+C to exit)...` and
then prints nothing — `Server post-boot:` is empty.

## Why this is NOT issue 0865

0865 covers the six nuttx cells that fail in a sweep and pass solo; its fix
(platform-scoped boot budgets + `qemu-nuttx` `max-threads` 9 → 1) recovered
pubsub and service, C and C++. This one fails solo, deterministically, on an
otherwise idle-enough host, and it fails LATE: everything up to and including
`Action client created` succeeds, so it is not discovery of the node, not the
locator, and not the boot budget.

`-2` is `NROS_RET_TIMEOUT`, and the deadline that expires is **inside the
image** — the app's own send-goal wait, not any test-side window. Raising a
test timeout cannot move it. That distinguishes it from 0854
(`action_raw_goal_ships_one_cdr_header`, which times out in-sweep and passes
solo) — the opposite pattern.

## What was ruled out

* **Concurrency / host load** — fails 3/3 alone.
* **Stale artifacts** — survives `rm -rf` on the leaf `build-zenoh` plus a full
  rebuild (this is also what separates it from issue 0820's museum-binary class
  on the riscv-nuttx sibling).
* **Boot budget** — the client gets much further than boot; the server's banner
  arrives.
* **The missing settle delay.** Both service and action skip
  `stabilization_delay()` for NuttX while the comment above the service site
  says the delay exists so the client's "first query doesn't race ahead of the
  server queryable's declaration. Only applies to QEMU-cold-boot platforms" —
  and NuttX is one. Un-excluding it gives a 20 s settle and the test still
  fails 3/3 with the same `-2`. The code/comment contradiction is real and
  worth fixing on its own merits, but it is NOT the cause here, so the change
  was reverted rather than landed (it costs 20 s per test and fixed nothing).

## Where to look next

The server declares its queryable and never answers. Two candidates, neither
yet tested:

* **Queryable capacity.** A service server IS a zenoh queryable, and an action
  server is several; `ZPICO_MAX_QUERYABLES` is 8 embedded, with `[param_services]`
  and `[lifecycle]` claiming slots before the app declares anything (issue 0460).
  An action server that silently fails to declare one of its per-channel
  queryables would present exactly as "server is up, goal query times out".
  Worth dumping the declared keyexprs on the server side first.
* **The C++ sibling errors DIFFERENTLY on the same board** —
  `node.create_action_client(client, "/fibonacci") -> -100` — and is flaky
  rather than dead. Two different failures on one platform for one variant
  suggests the action path's embedded resourcing, not the C bindings.

## Acceptance

* `test_rtos_action_e2e` nuttx/C passes solo, repeatably, and the diagnosis
  names which resource ran out rather than reporting a bare `-2`.
