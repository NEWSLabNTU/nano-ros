---
id: 2947
title: "threadx-linux C talker: the node's /rosout writer fails with PublisherCreationFailed and is skipped"
status: open
type: bug
area: [threadx, rmw-zenoh, c-api]
severity: medium
found: 2026-11-24
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

## Where to look next

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
