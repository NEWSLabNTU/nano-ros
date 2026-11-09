---
id: 2388
title: "The census-sized Zephyr C++ talker leaf gets two transient-local retention slots where the C leaf and the C++ workspace image get one"
status: open
type: bug
severity: low
area: [memory, zephyr, rmw-zenoh, census]
related: [phase-524]
found: 2026-11-10
---

**Home: [phase-524](../roadmap/phase-524-emulator-twins.md)** W3, beside
issue 2202, which measured it. It is a sizing difference with no diagnosis yet,
so it is filed on its own rather than folded into 2202's defaults work.

## What

The zenoh backend's transient-local retention pool, `TL_SLOTS`
(`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/publisher.rs:196`), has
`MAX_TL_PUBLISHERS` entries. A sized image derives that count from its
declared or census-taken endpoints (`NROS_ZENOH_MAX_TL_PUBLISHERS`,
`nros-rmw-zenoh/build.rs:541`).

Measured with `just mem-report --json`, Zephyr 3.7, mps2-an385-zephyr, while
measuring issue 2202 (2026-11-10, PR #3390):

| image | road | `TL_SLOTS` |
| --- | --- | ---: |
| `examples/zephyr/c/talker` | census (`--from-leaf`) | 1,344 B (1 slot) |
| `examples/zephyr/cpp/talker` | census (`--from-leaf`) | 2,688 B (2 slots) |
| `pubsub_bringup:zephyr_talker`, C++ (`examples/workspaces/cpp`) | model | 1 slot (pools match the census-sized leaf) |
| any leaf, undeclared | defaults | 2,688 B (2 slots) |

The C++ leaf's census-sized number equals the UNDECLARED default. Either its
census states one more transient-local publisher than the C leaf's, or the
C++ leaf's census does not reach this knob and it falls back to the default.

## Not known yet

- Which of the two it is. The leaf's census lives under its `build/`.
- Whether the same holds on other boards. The Nucleo twin's C++ talker was not
  measured.

## Acceptance

- The C++ talker leaf's census-sized `TL_SLOTS` equals the number of
  transient-local publishers it really declares, the same as the C leaf and
  the workspace image.
- If the 2-slot number is the fallback, the knob's road from a C++ leaf
  census is fixed for the class (every pool that census feeds), not for this
  one knob.
