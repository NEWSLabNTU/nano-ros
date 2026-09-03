---
id: 867
title: "`test_rtos_action_e2e` nuttx/C fails 3/3 SOLO — the client's goal send
  times out (-2) against a server sitting at its ready banner"
status: resolved
type: bug
area: testing, rmw
related: [issue-0891, issue-0854, issue-0460]
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

## Why this is NOT issue 0891

0891 covers the six nuttx cells that fail in a sweep and pass solo; its fix
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

## Cause, found by booting the pair by hand

Not a resourcing bug at all — an ORDERING one. `start_pair` launched both
instances simultaneously on NuttX, and its comment says why: the NuttX Rust
binaries boot slowly, so giving the listener the usual 20 s head start expired
its session before the talker finished booting. That is correct for PUB/SUB,
where a subscriber joining late still receives the next sample. It was keyed on
the PLATFORM, so it also governed the request/response shapes — where the client
asks ONCE and gives up.

Booted by hand with the client started after the server's banner, the same two
images complete goal -> accept -> feedback -> result every time. Started
together, the client reaches `Sending goal` before the server's queryable is
declared, and the deadline that expires is the app's own.

## Fix

`start_server_then_client` — the client is not spawned until the server's
readiness banner has actually been observed. Keyed on the SHAPE, and waiting on
the banner rather than sleeping a fixed guess at how long the banner takes.
Applied to both request/response shapes (service and action); pub/sub keeps its
parallel start for the reason above, and `start_pair`'s reasoning was folded
into the replacement rather than deleted with it.

Measured: nuttx/C action 3/3 failing at 72-92 s -> passing in 16.2 s. Action
matrix 8/8 on real cells; freertos and threadx-linux unaffected.

## Acceptance

* `test_rtos_action_e2e` nuttx/C passes solo, repeatably. (Met.)

## Still failing in that cell's C++ sibling — issue 0870

nuttx/C++ action intermittently fails `create_action_client` with `-100`
(`NROS_CPP_RET_TRANSPORT_ERROR`), roughly 2 runs in 3, solo on an idle host.
Different failure at a different point — it never finishes constructing the
client, where this issue's client got as far as `Sending goal`. It predates this
fix and survives it. Filed as 0870 rather than folded in here, because the two
were already easy to conflate: the same cell produced `-2` and `-100` on
different runs.

## Resolved by `bb0631e5f` (2026-08-28) — the issue outlived its own fix

The fix landed the day this issue was last edited and nobody closed the file, so
it has read as open ever since. Found during the phase-414 survey.

CAUSE: not in the image at all. `start_pair` launched both NuttX instances at
once, keyed on the PLATFORM — correct for pub/sub, where a subscriber joining
late still gets the next sample, and wrong for request/response, where the
client asks ONCE and gives up. The C action client reached `Sending goal` before
the server's queryable was declared.

FIX: `start_server_then_client` (`packages/testing/nros-tests/tests/rtos_e2e.rs:559`)
waits on the server's readiness BANNER rather than sleeping a guess, and is keyed
on the SHAPE, which is what actually differs. Measured 3/3 failing at 72-92 s →
passing at 16.2 s.

That also explains why the `stabilization_delay()` experiment recorded above
changed nothing: a fixed settle is a guess at the banner's timing, and the banner
is observable.

The code/comment contradiction this issue found — NuttX excluded from
`stabilization_delay()` while the comment says the delay is for QEMU-cold-boot
platforms, which NuttX is — is REAL and still unfixed. It is a cleanliness item,
not a cause (measured: un-excluding it costs 20 s and fixes nothing), and it does
not belong to this issue.

## Not shared with 0870 — the phase-414 question, answered

phase-414 W2/W3 asks whether these two have one cause. **They do not**, and the
evidence is structural rather than a judgement call:

* This fix covers all three languages (`rtos_e2e.rs:922` calls
  `start_server_then_client` once for the whole action matrix), so the ordering
  cause is already neutralised for C++ — and 0870 still fails ~2 in 3.
* The failure POINTS cannot be the same defect. This one failed at `send_goal`,
  after the client's four entity declarations had all succeeded. 0870 fails
  INSIDE those declarations, before any interaction with the server exists, so
  server ordering is causally irrelevant to it.

What they do share is diagnostic blindness on NuttX, not a root cause — see the
`printk` note now in `zpico.c`.
