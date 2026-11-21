---
id: 2790
title: "The stock Rust talker template for nucleo-f767zi no longer links: its stack floor rose to 78,152 B and the image overflows RAM by 2,112 B"
status: open
type: bug
severity: high
area: [stack-floor, zephyr, templates]
related: [issue-2783, issue-2400, issue-2658]
found: 2026-11-22
---

**Home:** [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md),
the stack-floor redesign that [issue 2400](2400-stack-floor-from-measured-frames.md)
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
