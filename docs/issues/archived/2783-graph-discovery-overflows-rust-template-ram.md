---
id: 2783
title: "Turning on graph discovery in the stock Rust talker template on nucleo-f767zi fails to link: the template's 75,536 B main stack leaves 552 B of RAM, and discovery costs 8,232 B"
status: resolved
type: bug
severity: medium
area: [zephyr, stack-floor, zenoh, templates]
related: [issue-2400, issue-2658, issue-2360]
found: 2026-11-21
resolved: 2026-11-22
resolved_in: "fix(issue 2783): the stock Rust talker builds and runs with graph discovery on"
---

**Home:** [phase-557](../../roadmap/phase-557-stack-floor-measured-frames.md),
the stack-floor redesign that [issue 2400](../2400-stack-floor-from-measured-frames.md)
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

## Not in the stack-floor bug batch (2026-11-22)

The bug batch (issues 2818, 2784 and 2693) does not take this issue's fix,
for three reasons:

- **The floor cannot go below the walk.** The 75,536 B floor is the bind walk
  plus the margin, and ruling (B) of issue 2400 makes the static walk the floor,
  never lower. A floor nearer the 28,340 B peak needs a TIGHTER walk, not an
  allowance.
- **The tighter walk is parked work.** The walk is loose because zenoh-pico's
  indirect call sites carry no type, so each reaches every function C can hold
  (issue 2516). Narrowing those sites is the site-classifier work of
  issue 2658, which stays parked as ruled.
- **The batch moves the other way.** Issue 2818 binds Cyclone's vtable calls,
  which raises Cyclone floors. It never lowers a zenoh one.

So this stays measured evidence for issue 2658 and the redesign. The user-side
workaround above still holds.

Of the two loose ends: the row's `peak = 28888` over-states by 548 B, which is
the safe direction, so it waits for the twin lane to re-measure the row once
#4486's soft-float frames are on main.

## The floor half: sized from the peak (2026-11-22)

User ruling, amending ruling A: an `[[observed]]` row whose static walk is only
an upper bound (its deepest path crosses an indirect call bound by type, not by
a vtable slot) is sized from its measured `peak` plus `margin_pct`. The build's
check, `--raise` and the `nros new` stamp all use that one rule. The row states
the edge in `upper_bound` (`<caller> -> <callee> (<tier>)`, crate hashes
stripped), taken from a fresh walk's `upper_bound_through`.

The drift check walked every row on 2026-11-22. Five rows cross a type-bound
edge and move; their template stamps (`recorded_floor`) go:

| row | was | now |
| --- | --- | --- |
| `talker/rust/zenoh@nucleo-f767zi-zephyr` | 78,152 | 31,784 |
| `talker/rust/zenoh@mps2-an385-zephyr` | 83,280 | 31,920 |
| `action-server/rust/zenoh@mps2-an385-zephyr` | 132,208 | 33,368 |
| `qemu_bsp_talker_bringup@mps2-an385-baremetal` | 176,208 | 39,424 |
| `talker/c/zenoh@nucleo-f767zi-zephyr` | 125,896 | 13,064 |

`action-server/rust/zenoh@nucleo-f767zi-zephyr` kept its walk floor (77,728 B)
at first: its image did not link (`region 'RAM' overflowed by 77440 bytes`),
so it had no fresh walk. The cause was its heap default (192 KiB for a Rust
image with parameter services), fixed under issue 2772 (112 KiB). Once it
linked, the walk rose to 111,136 B through the type-bound
`NodeImpl::___declare_entity -> zephyr_thread_wrapper` edge, the same edge as
the mps2 action server's, so it is binding noise, not a new frame. Re-measured
on the Renode twin (2026-11-22): peak 30,328 B. The row now carries
`upper_bound`, and its stamp is 77,728 to 33,368 B.

The drift check failed that rise. It now treats a row with `upper_bound` as
sized from its peak: a rising walk is a note, and a walk below the peak still
fails (it would mean the walk missed a path the image ran).

The template outcome (graph discovery on the stock image) is the template
side's to close.

## Resolved, 2026-11-22

Measured on `main` 6a2a272dc9 (after the peak-sizing change, #4617):

- `nros new tlk --board nucleo-f767zi-zephyr --lang rust --rmw zenoh
  --template talker` writes no `stack.conf`: the row's floor is now 31,784 B,
  within the stack the board gives by default, so the template states none.
- With only `[image.zephyr.knobs.zenoh] graph_discovery = true` and
  `graph_cache_bytes = 8192` added, `nros build` links. RAM is 327,168 B of
  384 KiB (83.2%), about 64 KiB free, where the same two lines overflowed by
  7,680 B before. The build states `ZPICO_GRAPH_DISCOVERY=1` and
  `ZPICO_GRAPH_CACHE_SIZE=8192`.
- `nros run tlk_node_bringup:zephyr` boots it in the Renode twin, and it
  publishes `Hello World: 1` through `92`, one a second, until the run is
  stopped at 106 s. Nothing faults.

The user-facing side:
- `zenoh.graph_discovery`'s help states the cache's RAM cost.
- The Nucleo-F767ZI platform page shows the two knob lines and the RAM they
  leave.
- Issue 2836 tracks explaining a region overflow when one still happens.
