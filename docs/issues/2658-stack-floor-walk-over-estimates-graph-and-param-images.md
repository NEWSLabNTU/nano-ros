---
id: 2658
title: "The stack-floor walk refuses an image that queries the graph or declares a parameter, asking 3-10x the stack it uses"
status: open
type: limitation
area: build, stack-floor, zephyr
severity: medium
found: 2026-11-16
related: [issue-2360, issue-2202, issue-2402, phase-557]
---

**Home: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md).** The
walk binds an indirect call to every function its type allows, so a path it
cannot follow becomes a chain of every candidate. This
issue is measured evidence for the stack-floor redesign being weighed, from two
images that do things the templates do not.

## What was measured

Two Nucleo-F767ZI images, made with `nros new` from the `talker` template the
way `just zephyr stack-floor-twin` makes them, then changed:

- **C talker:** calls `rclc_executor_get_topic_names_and_types` and
  `rclc_executor_get_publisher_names_and_types_by_node` on every tick, with
  graph discovery switched on in the image
  (`[image.zephyr.knobs.zenoh] graph_discovery = true`,
  `graph_cache_size = 8192`). Both calls return `RCL_RET_OK`.
- **Rust talker:** declares an integer and a string parameter in `register()`
  (`param-store`, `[image.zephyr.knobs.params] max_parameters = 2`).

Each image was built with the stack-floor lane's `twin.conf` (the thread
analyzer, `CONFIG_INIT_STACKS`) and run in the Renode twin for 75 s: 12 to 14
analyzer rounds and 61 to 66 publishes, with no errors. Renode is
deterministic, so every round agrees.

| image | the walk's floor | measured main peak | floor / peak |
|---|---:|---:|---:|
| C talker, graph queries | 122,120 B | 12,844 B | 9.5x |
| Rust talker, two parameters | 76,816 B | 27,388 B | 2.8x |

For comparison, the unchanged templates' walks on this board are 16,112 B (C)
and 22,212 B (Rust), per phase-557 W6's first run.

At the lane's 64 KiB main stack the build REFUSES both images
(`the OPEN stack is 65536 B, below the measured floor ...`), so a user who adds
one graph query to the C template must give main 122 KiB of the board's
384 KiB to link it, against the 13 KiB the image uses.

## Why the floor is that large

The C image's worst path, from the walk's `--json` output, is 460 frames long.
It runs from `main` through the executor's open, logger dispatch, an action
server's goal iteration, `zephyr_thread_wrapper`, the UART driver's
`uart_stm32_configure` and the graph-change callback. Most links are indirect
calls bound by type. No frame on it is larger than 4,240 B, so the floor is a
long sum of small frames on a path no thread runs, not one large frame.

The Rust image's worst path is 273 frames, the same shape. Its largest frames
are the component install (8,040 B), an action server's registration
(6,056 B) and the parameter declaration (3,720 B), and the image declares no
action server.

## What would close it

A walk whose floor for these two images is within the lane's margin of the
measured peak, or a stated reason why the gap is the price of soundness. The
two images are the test: they are reproducible from the steps above, and the
twin measures them in under two minutes each.
