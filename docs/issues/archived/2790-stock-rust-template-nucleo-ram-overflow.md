---
id: 2790
title: "The stock Rust talker template for nucleo-f767zi no longer links: its stack floor rose to 78,152 B and the image overflows RAM by 2,112 B"
status: resolved
type: bug
severity: high
area: [stack-floor, zephyr, templates]
related: [issue-2783, issue-2400, issue-2658]
found: 2026-11-22
---

**Home:** [phase-557](../../roadmap/phase-557-stack-floor-measured-frames.md),
the stack-floor redesign that [issue 2400](../2400-stack-floor-from-measured-frames.md)
tracks (nros 133).

## What happens

On `main` at `4b3d19f1cb`, an untouched `nros new stock_talker --board
nucleo-f767zi-zephyr --lang rust --rmw zenoh --template talker` followed by
`nros build` fails to link:

```
region `RAM' overflowed by 2112 bytes
```

The template writes `CONFIG_MAIN_STACK_SIZE=78152` into its `stack.conf`.

## Why

`bf46a03f84` (phase-557 W9b) gave the walk frames for libgcc's soft-float
helpers, so it now finishes, and it re-walked the template's
`config/stack-floors.toml` row: `static` went from 66,568 to 69,648 B. The floor
derived from it, and with it the template's main stack, went from 75,536 to
78,152 B. The measured main peak of the same image on its Renode twin is
28,340 B (issue 2783).

Issue 2783 measured that this image had 552 B of RAM to spare at a 75,536 B
main stack. The extra 2,616 B of stack takes it over: 2,616 − 552 = 2,064 B,
close to the 2,112 B reported (the rest is alignment).

So it is issue 2783's mechanism, a floor far above the measured peak spending
RAM the image needs, now reached by the stock template with nothing added.

## Also

`python3 scripts/stack-floor-twin.py --only talker/rust/zenoh@nucleo-f767zi-zephyr`
is still red after `bf46a03f84`, for a new reason. Its `twin.conf` sets a 64 KiB
main stack, and the build's floor check refuses it:
`the OPEN stack is 65536 B, below the measured floor 75816 B`. The lane cannot
re-measure the row whose floor it exists to calibrate.

## Direction

For the floor redesign, not this issue alone: the floor of an image whose peak
is measured should not exceed what its board can hold. Until then, the
template's `stack.conf` could cap at a value that links, with the measured
peak as its evidence.

## Resolution (2026-11-22)

The floor stays as the walk derives it: a measured image whose stated stack is
below its walk's floor is refused at build (issue 2400 ruling (A)), so a
`stack.conf` capped below the floor would fail at build instead of at link.
The image was given RAM instead.

- **The board's transport task stacks moved to DTCM.** The link map of the
  failing image showed `nros_thread_stacks` (the transport's read and lease
  task stacks) at 32,896 B in sram0, and the part's 128 KiB DTCM at 0 B used.
  `nros-board-nucleo-f767zi-zephyr`'s `prj.conf` now sets
  `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM=y`, for every image on the board. The
  main stack is the kernel's and stays in sram0; no heap moved.
- **The sched-task pool stays in sram0**
  (`CONFIG_NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM=n`). In DTCM, Zephyr's
  `KEEP` output section keeps an unused pool whole (65,792 B), where sram0
  lets `--gc-sections` drop it: issue 2793.
- **The twin lane never lowers a template's stack.** `twin.conf`'s main
  stack is the larger of 64 KiB and the template's `stack.conf`, so the lane
  builds the image whose row it calibrates.

Measured on the same `nros new` image, Zephyr 3.7:

| | before | after |
| --- | --- | --- |
| link | RAM overflowed by 2,112 B | links |
| sram0 used | over 393,216 B | 362,432 B (92.17 %), 30,784 B free |
| DTCM used | 0 B | 32,896 B (25.10 %) |
| walk check | (no link) | OK, stack 78,152 B |
| Renode twin | refused (64 KiB below the floor) | boots and runs; main peak 28,340 B, inside the walk |

The 30,784 B now free also covers issue 2783's 8,232 B for graph discovery.
The floor's 2.7x over-statement itself stays with the redesign (issue 2400,
phase-557 W9c).
