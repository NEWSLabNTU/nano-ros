---
id: 2388
title: "The census-sized Zephyr C++ talker leaf gets two transient-local retention slots where the C leaf and the C++ workspace image get one"
status: resolved
type: bug
severity: low
area: [memory, zephyr, rmw-zenoh, census]
related: [phase-524]
found: 2026-11-10
resolved: 2026-11-16
resolved_in: "phase-524 (no longer reproduces on main; the fixing commit was not bisected)"
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md)** W3, beside
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

## Resolution (2026-11-16): no longer reproduces

Re-measured on main (Zephyr 3.7, `mps2-an385-zephyr`, `nros build`), reading
`TL_SLOTS` with `arm-zephyr-eabi-nm -S` and the generated
`MAX_TL_PUBLISHERS` in each lane's `nros-rmw-zenoh` `buffer_config.rs`:

| image | road | `TL_SLOTS` | `MAX_TL_PUBLISHERS` (both Rust lanes) |
| --- | --- | ---: | ---: |
| `examples/zephyr/c/talker`, `mps2_zenoh` | workspace bringup | 1,344 B (1 slot) | 1 |
| `examples/zephyr/cpp/talker`, `mps2_zenoh` | workspace bringup | 1,344 B (1 slot) | 1 |
| `examples/templates/cpp-port-minimal-publisher/zephyr` | standalone C++ leaf, census-sized | 1,344 B (1 slot) | 1 |

Each image's `resolved.cmake` states `NROS_DERIVED_TL_PUBLISHERS 1`.

Two things explain the 2-slot reading this issue was filed from.

- **The talker changed roads.** phase-531 W6 (3eefb44f93) made
  `examples/zephyr/cpp/talker` a workspace bringup. It reached main on
  2026-11-10 at 01:09 UTC, the day this was measured, so the measurement read
  the old standalone leaf.
- **The standalone C++ leaf road is also sized now.** The
  `cpp-port-minimal-publisher` leaf is census-sized and links one slot. That
  meets the acceptance's class half: no pool on that road falls back to the
  default. Which commit closed the leaf road's gap between the 2026-11-10
  reading and now was not bisected.

One thing that looks like the bug and is not: the leaf's host census build
(`build/census-host/cargo/nros-{c,cpp}-image_*`) generates
`MAX_TL_PUBLISHERS = 2`. That build runs the app in counting mode to take its
endpoints, before any declaration exists, so it compiles at the undeclared
default by design. It is never linked into the image.

