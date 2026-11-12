---
id: 2460
title: "The stack-floor walk is refused on mps2-an385-zephyr images: z_malloc, z_free and z_sleep_us have no frame, because the zpico-sys frame rows are stated for thumbv7em only"
status: open
type: limitation
severity: low
area: [memory, zephyr, build]
found: 2026-11-12
related: [issue-2400, issue-2402, phase-557]
---

## What

Issue 2400 W4 runs the stack-floor CHECK on every Zephyr image off `ARCH_POSIX`. On the four
`mps2-an385-zephyr` template leaves (`ament-workspace-port`, `cpp-port-minimal-publisher`,
`rclcpp-compat-smoke`, `topic-state-monitor-port`, all C++ on thumbv7m) the walk is refused:

```
nros-stack-floor: REFUSED
root `…create_publisher_trampoline…`:
  - functions reached with no frame in any input (a missing frame would count as 0): z_free, z_malloc, z_sleep_us
```

Those images are UNMEASURED, so ruling (A) holds and the build only warns (`the walk was refused
for an UNMEASURED image; not enforced`). The cost is that they get **no floor at all**, not even
the static walk + margin an unmeasured image normally reports.

## Why

`zpico-sys`'s `platform_aliases.c` is built by its cc build script, which gets no
`-fstack-usage` under Zephyr. So its five functions carry hand-read frames in
`config/stack-floors.toml` (`[[policy]] kind = "frame"`: `z_malloc`, `z_free`, `z_random_fill`,
`z_sleep_us`, `z_sleep_ms`). Every one of those rows says `target = "thumbv7em-none-eabi"`:
they were read off the nucleo-f767zi (Cortex-M7) prologues. mps2-an385 is
`thumbv7m-none-eabi`, so no row applies there.

## Fix directions

- Preferred: give the `zpico-sys` cc build `-fstack-usage` (and `-fcallgraph-info=su`) when the
  stack floor is on, so the frames are measured per target and the hand rows retire.
- Or: read the same five prologues on thumbv7m and add rows for that target.

Acceptance: `just check template-copy-out` on the four leaves prints an UNMEASURED floor
(walk + margin) instead of a refusal.
