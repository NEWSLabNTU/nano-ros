---
id: 1204
title: "The safety-island Zephyr image no longer links on main -- DTCM
  overflowed by 45040 bytes"
status: open
type: bug
area: zephyr
severity: high
related: [issue-1197, issue-1132]
---

## Symptom

Autoware Safety Island's `board-build` recipe (that repo's justfile, not
this one) for `mr_canhubk3/s32k344` fails at the link:

```
ld.bfd: region `DTCM' overflowed by 45040 bytes
collect2: error: ld returned 1 exit status
FAILED: zephyr/zephyr_pre0.elf zephyr/zephyr_pre0.map
```

Reproduced on a wiped build directory, twice, with the SAME byte count both
times -- so this is not the derived-knob convergence that needs a second pass.
The derived knobs are identical to a build that links:

```
NROS_EXECUTOR_MAX_CBS=19
NROS_SUBSCRIPTION_BUFFER_SIZE=1496
NROS_SUBSCRIBER_BUFFER_SIZE=880
entity inventory: 4 components -- 33 entities, 19 executor callback slots
```

## Scale

DTCM is 128 KB. A build of this image that links reports:

```
DTCM:       93344 B       128 KB     71.22%
```

Overflowing by 45040 puts it near 176 KB, so roughly 83 KB has appeared. That is
not drift; something is placed in DTCM that was not there before, or is placed
twice.

## Bisect boundary, established

Not caused by the change that found it. A control build of clean `origin/main`
with that change stashed produces the IDENTICAL overflow, to the byte.

The last commit known to produce a linking image is `fb94d1896`
(`fix(zephyr): the C-for-C++ nros-c string must read the C++ one`), measured on
its own branch at 71.22% DTCM before it merged. The commits merged after it:

```
055b84387 fix(check): doc-commit-citations is red on main, in both directions
56fc7b03c build(#1197): six more leaf locks the gate surfaced once the first 13 were fixed
a0992e5d8 build(#1197): the 13 leaf locks that carry nros-node gain the new path dep
dc2416f6e feat(#1197): the placement arithmetic moves to a crate a build script can call
3e5dc6e4f fix(ci): the CLI cache stored the whole job, not the CLI build
```

`dc2416f6e` is the one to look at first -- it moves the PLACEMENT arithmetic,
which is what decides what lands in DTCM. The other four are a lockfile sweep, a
CI cache fix and a docs gate, none of which should reach a linker script. This
is a reading of the subject lines, NOT a bisect: the bisect was not run.

## Why this matters more than the byte count

Nothing merge-gating builds this image. The regression reached main and the only
reason it is known is that someone tried to verify an unrelated cmake fix
against a real board image. That is the same coverage hole issue 1177 is about,
one platform over.

## Next step

Bisect the five commits above with the island's board-build recipe on a wiped
directory, and
if it is `dc2416f6e`, compare the generated linker script and the DTCM section
list against `fb94d1896`.
