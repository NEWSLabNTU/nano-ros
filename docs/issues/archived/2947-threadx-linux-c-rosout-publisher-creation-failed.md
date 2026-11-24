---
id: 2947
title: "threadx-linux C talker: the node's /rosout writer fails with PublisherCreationFailed and is skipped"
status: resolved
type: bug
area: [threadx, rmw-zenoh, c-api]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: []
---

## What happens

`examples/threadx-linux/c/talker` (cmake road), on `main` as of 2026-11-24,
prints at boot:

```
[ERROR] nros: /rosout could not be created for node /talker (Transport(PublisherCreationFailed)); its log records are not published.
```

The talker then publishes normally. The node's log records never reach
`/rosout`.

Measured on host 142 (aarch64) on 2026-11-24, while working the threadx-linux notifier fix (PR #4798). It
happens with and without that fix, so it predates it.

## What it is not (measured)

- **Not a zenoh-pico pool.** Every `ZPICO_ERR_FULL` return in `zpico.c` (nine
  sites, including `zpico_declare_publisher_ex`) was instrumented, and none
  fired.
- **Not the TRANSIENT_LOCAL retention pool, nor its cache queryable.** Both
  refusals in `ZenohPublisher::new_on` log a line naming their knob, and
  neither line appears.
- **Not the silent stack-floor class that PR #4798 fixes**, as far as measured: no ThreadX thread is
  created on this path, and the image's sizing descriptor states
  `publisher_count = 3` against `ZPICO_MAX_PUBLISHERS=3`.

## Where to look next (as filed; superseded by the cause below)

The writer is created by `create_rosout_publisher`
(`nros-node/src/executor/spin.rs`), which maps any error from
`Session::create_publisher` to `PublisherCreationFailed` and so loses the
reason. What is left in `create_publisher` before zenoh-pico is reached:

- `qos::admit` (three refusals; its `refuse` may report only the first
  time);
- `DataKeyexpr::of` (a keyexpr longer than the buffer);
- the `@adv` liveliness keyexpr built in a `heapless::String`, whose
  `push_str` failures map to `()`.

Next step: keep the error's own text in the `/rosout` refusal line instead
of collapsing it, then read which one it is. The same image on the posix
platform is the control.

Not release-blocking: no release cell checks `/rosout` on threadx-linux.

## Cause (measured)

None of the candidates above. With the backend's error carried through, the
refusal read `publisher '/rosout' was refused by the RMW backend:
ConnectionFailed`, and it came from the C-ABI shim's node table
(`nros-rmw-cffi`, `claim_node_slot`), before zenoh-pico was reached.

The table held `('talker', '')` and the `/rosout` writer asked for
`('talker', '/')`. An entity that states no namespace (the session's own
open-time identity) reached the shim as `""`, and a node the executor announces
reached it as `/`. They are the same node, but the shim kept them as two
triples, and a one-node image derives `NROS_RMW_MAX_NODES = 1`, so the second
claim was refused. A larger table would have declared the node twice on the
graph instead.

It is not a regression from 0.6.0-nros11: nros11 created no automatic C
`/rosout` writer (that arrived in phase-556 W13, after the nros11 tag).

## Resolution

- `nros-rmw-cffi` `node_slot` spells the root namespace one way, `/`, so `""`
  and `/` share one slot. Test
  `root_namespace_has_one_spelling_in_the_node_table` (fails with the
  normalisation removed).
- A full node table is now `TransportError::InvalidConfig`, not
  `ConnectionFailed`, and logs which node did not fit and that
  `NROS_RMW_MAX_NODES` sizes the table.
- `nros-node` reports the backend's own error at every entity-creation site
  that used to fold it away (`creation_refused`: the `/rosout` and
  `/parameter_events` writers, raw publishers, publishers and clients), so the
  next refusal names its cause instead of `PublisherCreationFailed`.

Verified on host 142: the rebuilt threadx-linux C talker boots with no
`/rosout` refusal and publishes normally.
