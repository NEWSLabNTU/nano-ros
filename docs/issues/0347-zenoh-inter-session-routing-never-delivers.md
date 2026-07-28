---
id: 347
title: "Two independent zenoh client sessions on the same zenohd never exchange data — same-session pub/sub works against the same router"
status: open
type: bug
severity: high
area: core
related: [issue-0328, issue-0135]
---

## Finding (2026-07-28, while un-ignoring the zenoh integration tests for issue 0328)

`packages/zpico/nros-rmw-zenoh/tests/zenoh_integration.rs::test_pubsub_separate_sessions`
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
