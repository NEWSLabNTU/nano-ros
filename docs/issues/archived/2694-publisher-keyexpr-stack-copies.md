---
id: 2694
title: "Publisher creation holds each data key expression twice on the stack, at three call levels"
status: resolved
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

## Resolved (2026-11-23)

The key is held once, in the form the C call needs. `DataKeyexpr`
(`shim/publisher.rs`) is one `heapless::String<KEYEXPR_BUFFER_SIZE>` holding
the key and its NUL:
- `as_str()` is the key, for the debug log and the retention keyexpr;
- `as_nul_bytes()` is the form `declare_publisher` takes.

`ZenohPublisher::new_on` builds it once and passes it by reference to
`declare_data_publisher` and `declare_retention`. The one other declare site
(`ZenohSession::declare_node_topic_writer`) builds its own.

**A second defect on the same path.** `TopicInfo::to_key` ignores the write
error, so a topic whose key did not fit was built TRUNCATED. The
`TopicNameInvalid` check after it could never fire: a 256-character topic
became the key `0/`. `try_to_key` (`keyexpr.rs`) now reports the overflow,
and `DataKeyexpr::of` refuses such a key. `to_key` is unchanged for its other
callers.

**Measured** on `qemu_bsp_talker_bringup@mps2-an385-baremetal`, main
`856de6eb48` vs this change. Frames come from this build's llc objects only
(`llvm-readobj --stack-sizes`); the walk from `stack-floor-drift --only`.

| frame | before | after |
|---|---:|---:|
| `ZenohPublisher::new_on` | 2,360 B | 1,584 B |
| `declare_data_publisher` | 1,336 B | 24 B |
| `data_keyexpr` -> `DataKeyexpr::of` | 1,088 B | 560 B |
| the three, together | 4,784 B | 2,168 B (-2,616 B) |
| `ZenohSession::share_data_publisher` | 616 B | 616 B (out of scope: its key is the hashed lookup key) |
| the row's walk | 113,276 B | 111,124 B (-2,152 B) |

The row's recorded `static` (153,784 B) is not re-recorded here. The open
cargo-C typing PR (issue 2658's next narrowing) re-records this row, and
re-recording it twice would conflict. The walk on main is already below the
recorded value, and a fall never fails the drift check.

Tests: `the_key_and_its_nul_form_are_one_buffer` and
`a_key_over_the_bound_is_refused`. The latter failed before `try_to_key`,
because it caught the truncation.
