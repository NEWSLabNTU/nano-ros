---
id: 1227
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

## Bisect: four verdicts, and why it could not be finished

Measured on `mr_canhubk3/s32k344`, wiped build directory each time. BAD is
always the SAME byte count, which is itself evidence it is one cause:

| commit | verdict |
| --- | --- |
| `1976727d8` | GOOD -- DTCM 93344 B, 71.22% |
| `e3786749a` | GOOD -- DTCM 93344 B, 71.22% |
| `d7e7bb477` | GOOD -- DTCM 93336 B, 71.21% |
| `fb94d1896` | BAD -- overflowed by 45040 bytes |

`d7e7bb477` (2026-09-06 21:14) is the newest commit known to build, and is the
floor to search from. The regression is in what `main` gained between it and
`fb94d1896`.

NOT environmental: `e3786749a` reproduces 71.22% a day after the same tree first
produced it, and the three GOOD commits agree to within 8 bytes.

### Two wrong readings, recorded so they are not repeated

* **"the first suspect is `dc2416f6e`"** -- from reading subject lines
  (`the placement arithmetic moves to a crate a build script can call`). It IS
  bad, but so is every commit below it in that range, including `3e5dc6e4f`,
  a CI cache fix that cannot move RAM. Reading a subject line is not a bisect.
* **`01cb87d8f` reported BAD -- RETRACTED.** It built only because it ran before
  the probe directory was touched, so `sync` skipped metadata regeneration and
  it consumed metadata emitted by ANOTHER commit's codegen. Its number is not
  attributable to that commit. Any narrowing that rested on it (an 11-commit
  range was claimed) is withdrawn.

### What blocks finishing it

Most commits in the range cannot build the CURRENT safety-island tree at all.
The CLI's `sync` step regenerates the message metadata whenever the CLI
changes, and at those commits regeneration fails:

```
build/nros-metadata/metadata-probe-cmake/build/nros-ws-nav_msgs/.../nav_msgs_msg_odometry.hpp:99:93:
error: static assertion failed: NROS_UNBOUNDED__nav_msgs_msg_odometry__field_header_frame_id:
nav_msgs/Odometry states no serialized-size bound
```

Reproduced at `0368d4040`, `5a3cbc008`, `3da478ffb`, `4d6e0da9c`, `09ed8e069`,
`c89431210`, `29ea7d181`, `e957bc634` -- with and without clearing
`build/nros-metadata`. ASI's caps are not being applied by those commits'
codegen, so the consumer's own messages come out unbounded.

Bisecting a library against a fixed consumer assumes the consumer builds at
every commit. Here it does not, and the commits that do not build are most of
the interval. Finishing this needs the ASI side moved in lockstep with the
library, or that codegen incompatibility understood first -- not more builds of
the same shape.

### Also in the way

Issue 1235: the `sync` step refuses a CORRECTLY paired resolver on these commits, and
`setup-cli` / `setup-launch-resolve` cannot repair what their own errors name.
Every bisect step needs this working around before it can even reach a compiler.

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
