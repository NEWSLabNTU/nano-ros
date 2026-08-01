---
id: 347
title: "zpico is single-session per process, but a second open silently wiped the first session's registrations instead of failing"
status: resolved
type: bug
severity: high
area: core
related: [issue-0328, issue-0135]
---

## Finding (2026-07-28, while un-ignoring the zenoh integration tests for issue 0328)

`packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs::test_pubsub_separate_sessions`
opens two `ZenohTransport` client sessions against one zenohd, creates a
subscriber on the first and a publisher on the second, publishes, and polls for
2 s. Nothing arrives.

This is **not** the missing-router precondition the test was originally ignored
for, and not the pre-match best-effort drop it resembles. Both were ruled out:

| variation | result |
| --- | --- |
| router already running on the hardcoded `:7447` | still fails |
| router self-provisioned on an ephemeral port | still fails |
| publisher republishes every 100 ms for **10 seconds** | still fails |
| same-session pub/sub (`test_pubsub_loopback`), same router | **passes** |

So a single session routes to itself correctly, and the router accepts both
sessions, but a sample published on session B never reaches a subscriber on
session A. Ten seconds of retransmission rules out a discovery race — the route
is not slow, it is absent.

## Why this matters

Two sessions in one process is the shape a **bridge** takes, and
`examples/bridges/tt-zenoh-to-*` exists precisely to subscribe on one backend
and republish on another. It is also the shape of any multi-domain or
multi-RMW node. If inter-session routing does not work at the transport layer,
that whole category is resting on paths no test exercises — which is exactly
how this went unnoticed: the test that would have caught it has been
`#[ignore]`d since it was written, under a reason (`requires zenohd router`)
that was true but not the whole truth.

## Reproduce

```sh
cargo nextest run -p nros-rmw-zenoh --features platform-posix \
    --test zenoh_integration test_pubsub_separate_sessions -- --ignored
```

The test now self-provisions its router (issue 0328), so no manual setup is
needed. It is the ONLY remaining `#[ignore]` in that file, and its reason names
this issue.

## Where to start

- Whether both sessions actually complete their zenoh-pico session open against
  the router, or the second silently degrades (the config passed to both is
  identical — same `node_name`, `namespace`, `domain_id`).
- Whether the declared key expressions match across sessions: same-session
  delivery could be succeeding through a local shortcut rather than the router,
  in which case the router path has never worked and the passing test proves
  less than it appears to. Note `Z_FEATURE_LOCAL_SUBSCRIBER` already exists for
  intra-image delivery (CLAUDE.md), which makes this the first hypothesis worth
  eliminating.

## Root cause (2026-07-28)

Not a routing bug. `packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c` holds

```c
static z_owned_session_t g_session;
static bool g_session_open;
static subscriber_entry_t g_subscribers[ZPICO_MAX_SUBSCRIBERS];
```

— **one** session and process-global registration tables. `ZenohTransport::open`
called twice does not create two sessions: the second call re-enters
`zpico_init_with_config`, which `memset`s `g_subscribers` / `g_publishers` /
`g_queryables` and replaces `g_session` — **destroying the first session's
registrations while returning Ok**. The zpico source already says so in passing:
*"zpico is single-session/global already"* (`zpico.rs:344`).

So the old test asserted delivery through a configuration the shim does not
implement, and the failure was silent because `open()` reported success.

### How it was localised

Each step ruled out a hypothesis rather than confirming a guess:

| probe | result | eliminates |
| --- | --- | --- |
| two processes, one session each, via router (stock talker/listener) | **11 published, 11 received** | "routing is broken" |
| same-session pub/sub | works — but via the local loopback (`Z_FEATURE_LOCAL_SUBSCRIBER=1` on host), not the router | the passing test proving anything about routing |
| republish every 100 ms for 10 s | still nothing | pre-match best-effort drop |
| `drive_io()` on both sessions each round | still nothing | "the test never pumps the socket" |
| session ZID generation | `getrandom()` per session on Linux | duplicate-ZID / "peer sees itself" |
| **subscribe on A AFTER opening B** | **delivered** | everything else — the subscription had been erased, not misrouted |

That last row is the proof: the same subscription works when declared after the
memset that would have erased it.

## Fix

`zpico_init_with_config` now refuses when a session is already open
(`ZPICO_ERR_SESSION`) instead of re-initialising global state out from under
the live session.

The condition is deliberately *"a session is currently OPEN"*, not *"init ran
before"*: a **failed** `zpico_open()` leaves `g_session_open == false` and must
still be retryable — issue #64's esp32-c3 connect backoff depends on exactly
that path.

`test_pubsub_separate_sessions` is replaced by
`second_session_open_in_one_process_is_refused`, which asserts the contract and
**passes** — the file now runs 13/13 with nothing ignored, where 5 tests had
never run at all.

**Superseded by [phase-328](../roadmap/phase-328-zpico-multi-session.md) /
[issue 0348](0348-zpico-multi-session-support.md) (2026-08-01):** the refusal
was the honest stop-gap; 0348 added real multi-session support (per-session
`zpico_session_t` pool + handle-passing). The refusal test above is now
`two_sessions_deliver_cross_session_through_router`, which asserts delivery
across two independent sessions instead of refusal.

## Behaviour change worth knowing

A caller that configured two zenoh sessions in one process (e.g. two
`[[domain]]` entries both `rmw = "zenoh"`, reaching
`CffiRmw::open_with_rmw` for an extra session) now gets an **error at open**
where it previously got silence. That is not a new breakage: such a setup was
already broken, with the first session's subscribers dying invisibly. The error
makes a pre-existing defect visible.

No in-tree configuration does this — checked every tracked `system.toml`; the
bridge workspaces pair zenoh with a *different* backend, which is one zenoh
session and unaffected.

## Still open — real multi-session support: **issue 0348**

Supporting two zenoh sessions means moving `g_session` and every per-session
`g_*` table into a context struct, and giving the ~38 `zpico_*` entry points a
session handle — a breaking change across 51 consuming files. Filed separately
as issue 0348 with the full inventory and options; the honest contract until
then is to refuse.

## Verification note

The `bridge_zenoh_to_cyclonedds` e2e cannot run in this checkout (its
cyclonedds fixtures are not built). It fails **identically with the guard
reverted**, so it is unaffected by this change — but it was not a positive
confirmation, and the bridge path deserves a run on a machine with those
fixtures staged.
