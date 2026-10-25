---
id: 2041
title: "A level image's executor arena is provisioned at 2.5x its measured use, and larger than the arena the image compiled"
status: open
type: enhancement
area: [executor, codegen, memory]
severity: low
found: 2026-10-25
related: [phase-509, rfc-0108, 2019, 2040]
---

## Measured

simple-autoware-safety-island board image (Zephyr, S32K344, three levels on
one executor, nano-ros 4f545011ff). The boot record read over SWD after the
`b` act (run p2618-pb, 16/16 PASS):

```
compiled in:
  NROS_EXECUTOR_ARENA_SIZE      25724
measured on the board:
  arena capacity                29820
  arena used                    11976   (40.2%)
  arena allocations             12
NOTE: the executor was handed 29820 bytes but the image compiled 25724.
```

The arena lives inside `__nros_levels_backing`
(`NROS_CPP_LEVELS_BACKING_SIZE` = 40,640 B, every level's members summed), and
on this board that static sits in DTCM, the scarcer region (83.66 % used).

Two things look wrong:

1. The arena is sized at 2.5x what the image uses: 17,844 B of DTCM are never
   claimed. phase-403 W3/W5 claim the arena per type at registration, but the
   island's board conf already records that "the derivation still provisions
   the old way" (max_cbs x worst case).
2. The level backing hands the executor 29,820 B while the image compiled
   `NROS_EXECUTOR_ARENA_SIZE` 25,724. The boot record flags it as legitimate
   (issue 0900: placement is the caller's), but the two derivations disagree,
   and the larger one is the one paid for.

## Closes when

- The levels backing sizes its arena from the per-type claims the members
  register (the same bound the first-spin advisory prints), not from
  max_cbs x worst case, and agrees with `NROS_EXECUTOR_ARENA_SIZE`.
- A board or fixture boot record shows arena use within a stated margin of
  capacity on a level image.
