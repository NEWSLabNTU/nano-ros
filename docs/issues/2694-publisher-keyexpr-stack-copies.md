---
id: 2694
title: "Publisher creation holds each data key expression twice on the stack, at three call levels"
status: open
type: tech-debt
severity: low
area: [rmw, zenoh, memory]
related: [issue-2493, issue-2550]
found: 2026-11-18
---

## What happens

Since every node declares a `/parameter_events` publisher (issue 2493), every
image runs the zenoh publisher-creation path at least once. That path builds
the topic's data key expression in two forms and passes both by value:

- `data_keyexpr` (`nros-rmw-zenoh/src/shim/publisher.rs`) builds a
  `heapless::String<KEYEXPR_STRING_SIZE>` from `TopicInfo::to_key`, copies it
  into a `[u8; KEYEXPR_BUFFER_SIZE]` with a NUL terminator, and returns the
  pair `(String, [u8; N])` by value.
- `declare_data_publisher` keeps both, but uses the `String` only for a
  `std`-only debug log, and passes the byte array to `declare_publisher`.
- `ZenohPublisher::new_on` calls `data_keyexpr` again and keeps both forms
  too.

The defaults are `KEYEXPR_STRING_SIZE` = 256 and `KEYEXPR_BUFFER_SIZE` = 257,
so each level holds about 520 B of key, plus the return slot.

## Measured (mps2-an385 bare-metal Rust talker, `--indirect bind` walk)

Stack frames from the llc objects (`llvm-readobj --stack-sizes`), before and
after the `/parameter_events` change:

| Function | Before | After |
|---|---:|---|
| `ZenohPublisher::new` | 1,840 B | 32 B |
| `ZenohPublisher::new_on` | — | 2,352 B |
| `declare_data_publisher` | — | 1,336 B |
| `data_keyexpr` | — | 1,088 B |
| `ZenohSession::share_data_publisher` | — | 592 B |

The row `qemu_bsp_talker_bringup@mps2-an385-baremetal` rose 812 B, from
92,504 to 93,316 B, when the change was first measured. On main of
2026-11-18 (after the `/diagnostics` reporter landed) the same change walks
94,704 to 97,084 B on one host (+2,380 B): the same frames, and a
different deepest path through the same graph (issue 2550).

## Fix

Hold the key once, in the form the C call needs:

- Build it NUL-terminated in a single buffer: push `'\0'` onto a
  `heapless::String<KEYEXPR_BUFFER_SIZE>`, or write into a
  `&mut [u8; KEYEXPR_BUFFER_SIZE]` the caller owns.
- Pass a reference down rather than returning the pair by value.
- Take the debug-log string from that same buffer.

That drops one copy at each level and the by-value return slot.

How much of that rise this gives back is not known yet. The drift row
follows the walk's deepest path, and where the walk cuts its call cycle can
move with the frames (issue 2550). Acceptance is the re-walked row, with the
per-function frames above, not a predicted number.
