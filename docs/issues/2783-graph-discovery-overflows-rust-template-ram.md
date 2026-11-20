---
id: 2783
title: "Turning on graph discovery in the stock Rust talker template on nucleo-f767zi fails to link: the template's 75,536 B main stack leaves 552 B of RAM, and discovery costs 8,232 B"
status: open
type: bug
severity: medium
area: [zephyr, stack-floor, zenoh, templates]
related: [issue-2400, issue-2658, issue-2360]
found: 2026-11-21
---

**Home:** [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md),
the stack-floor redesign that [issue 2400](2400-stack-floor-from-measured-frames.md)
tracks (nros 133). This issue is measured evidence for it.

## What happens

`nros new <name> --board nucleo-f767zi-zephyr --lang rust --rmw zenoh
--template talker` builds and runs. Adding only this to its `system.toml`, with
no code change:

```toml
[image.zephyr.knobs.zenoh]
graph_discovery = true
graph_cache_size = 8192
```

makes the image fail to link:

```
region `RAM' overflowed by 7680 bytes
zephyr/zephyr_pre0.elf section `noinit' will not fit in region `RAM'
```

A smaller `graph_cache_size` than the 65,536 B default does not help in
practice: the stock image has 552 B of RAM to spare.

## Measured (2026-11-21)

All on `nucleo-f767zi-zephyr` (sram0 384 KiB), Zephyr 3.7, `nros new` images.

**What discovery costs.** Diff of the RAM sections in the link maps of the stock
image and the same image with only the two knobs above: the net growth is
**+8,232 B**, all of it in `g_sessions` (`zpico.c`). The graph cache lives in
each zenoh session and is compiled in only when discovery is on, which is why
the default 64 KiB cache costs nothing while discovery is off. Every other
section is unchanged.

**Why there is no room.** The template writes `stack.conf` with
`CONFIG_MAIN_STACK_SIZE=75536`, the floor `config/stack-floors.toml` derives
for `talker/rust/zenoh@nucleo-f767zi-zephyr` from the static walk. The image
uses far less. Main-thread peak in the Renode twin (`CONFIG_INIT_STACKS` plus
the thread analyzer, 64 KiB main, 75 s, 14 analyzer rounds, every round equal):

| image | main peak |
| --- | ---: |
| stock talker template | 28,340 B |
| same, with a component that runs `get_topic_names_and_types`, `get_publisher_names_and_types_by_node` and `get_node_names` on every tick (phase-572 W4) | 28,340 B |

So 75,536 B of main is reserved for a 28,340 B peak: 2.7x, about 46 KiB of
sram0 the image never uses. That leaves the stock image 552 B of RAM, so any
static addition over 552 B fails to link, and graph discovery is simply the
first common one a user turns on. The graph queries themselves add nothing to
the peak: registration sets it.

To measure the graph image at all, its heap was lowered by 8 KiB
(`[image.zephyr.knobs.zephyr] heap_size = 188416`). A heap is not a stack
frame, so this does not move the number.

## Also found

- The table row for this template records `peak = 28888`. Today's measurement
  is 28,340 B, so the row over-states by 548 B (the safe direction).
- The stack-floor twin lane cannot currently re-measure this row:
  `python3 scripts/stack-floor-twin.py --only talker/rust/zenoh@nucleo-f767zi-zephyr`
  fails with `the build's report has no OPEN role (refused: functions reached
  with no frame in any input: __aeabi_dadd, __aeabi_dsub, __aeabi_l2d)`. The
  walk has no frame for compiler-rt's soft-float helpers, so the lane runs
  nothing. The number above came from running the image the lane built by hand.
  #4486 adds those frames; re-run the lane once it is on main.
- The lane's recipe lives in the zephyr module, not at the top level: it
  runs as `just zephyr stack-floor-twin`, or the script directly.

## Direction

For the redesign (issue 2400), not a fix this issue makes alone: a floor this
far above the measured peak costs RAM a user then cannot spend. Until then, a
user can set the main stack from the measured peak instead. That is
`CONFIG_MAIN_STACK_SIZE` in the image's own conf, overriding `stack.conf`,
with the floor check off (`NROS_STACK_FLOOR=OFF`), at their own risk.
