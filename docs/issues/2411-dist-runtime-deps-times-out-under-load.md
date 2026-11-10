---
id: 2411
title: "`check dist-runtime-deps` hits nextest's 2400 s timeout in `just ci gate` under host load: ~13 min solo where its recipe comment says ~1.5 s warm"
status: open
type: bug
severity: medium
area: [ci, testing, sdk]
found: 2026-11-10
related: [issue-1744, issue-1452, issue-1546]
---

Found while gating PR #3421 (phase-535 W6 slice 3). The slice touches nothing
this gate reads.

## What

`build::check::dist-runtime-deps` (`scripts/check-dist-runtime-deps.py`,
recipe `just/check/workflows.just` `dist-runtime-deps`) timed out in 2 of 3
consecutive `just ci gate` runs on one host:

```
SLOW [>2280.000s] (─────) nros-gates::gates build::check::dist-runtime-deps
TERMINATING [>2400.000s] (─────) nros-gates::gates build::check::dist-runtime-deps
TIMEOUT [2400.035s] (39/63) nros-gates::gates build::check::dist-runtime-deps
error: recipe `dist-runtime-deps` was terminated on line 172 by signal 15
```

Last failing run: nextest run ID `278b78b3-3d05-43e5-855f-efbc4e14afc5`.

- Host load average was 21–47 during the failing runs (a shared 26-user
  machine). No other gate ran at the same time.
- The middle gate passed.
- `just check dist-runtime-deps` run SOLO on the same HEAD passed (rc 0) in
  about 13 min wall.
- The recipe's comment says "~1.5 s warm over a full store". The measured solo
  time is about 500× that, so either the store or the scan has grown since that
  number was taken, or the scan is not "warm" on this host.
- The store here is `~/.nros/sdk`: 20 GB, 20 tools, several with 3–4 pins.
  It sits on `/home`, a spinning disk.

The SDK-store location is a measured fact. Whether that disk is why it is
slow is NOT measured; the next step is to profile it, not to guess.

## Why it matters

`ci gate` is the lane everyone runs before every push. A gate whose solo
runtime is a third of the slow-timeout reads red whenever the host is busy, and
a red that is "only load" teaches people to wave off the lane. (It is not in
the merge-gating CI lanes: `.config/ungated-gates.txt` keeps it on the
self-hosted nightly, issue 1744.)

## Candidate fixes (not yet chosen)

1. Profile the solo run: time spent per dist, ELF reads vs `readelf`/parse,
   and how many objects are scanned. Issue 1452 narrowed the subject to the
   `DT_NEEDED` closure of each dist's PROGRAMS; check that the narrowing still
   holds and the scan does not walk all of each dist.
2. Narrow the scan to the pins the index names (`<store>/<tool>/<pin>`, issue
   1546). Stale sibling pins are not this checkout's subject.
3. Put the gate in a nextest `test-group` with low `max-threads` so it is not
   competing with the rest of `check::build`.

Do NOT raise the timeout or add retries (start races and slow paths are fixed,
not retried).

## Measured (2026-11-10, newslab-118, NVMe store of 14 GB)

Profiled per pinned dist (`closure()` instrumented: files walked, ELF opens,
`ldd` calls, time). Solo it took 7.0 s cold and 2.0 s warm here, against 11-13
min on 175's spinning btrfs. The time is not in `ldd` (139 calls, under 1 s in
all) and not in sibling pins: `audit` already reads only the index's pin of
each tool (issue 1546). It is the walk. `closure()` stat'ed every file twice
and opened every one to read 64 bytes:

| pinned dist | files walked | dynamic ELF |
| --- | --- | --- |
| zephyr-sdk 0.16.8 | 58,863 | 1,339 |
| arm-none-eabi-gcc 13.2-nros5 | 7,501 | 52 |
| riscv-none-elf-gcc 14.2-nros1 | 5,939 | 150 |
| renode 1.17.0 | 1,899 | 34 |
| 10 others | 313 | 31 |

That is about 75,000 opens for about 1,600 objects. On a cold spinning disk
each open of a small file is a random read; at roughly 10 ms each, 75,000 of
them is about 12.5 min, which matches the solo time measured on 175.

## Fix

`closure()` now `lstat`s each file once and opens it only when the host could
load it: it has an exec bit, or `.so` in its name. Over every pin of a full
store (all 20 tools, not only the index pins), that skips exactly 41 ELF files
the old scan read: guest firmware images (`openbios-ppc`, `hppa-firmware.img`,
ESP ROM `.elf`s). None has a `DT_NEEDED` or a `PT_INTERP`, so none could reach a
verdict. The gate's output on this store is unchanged, including the "92
plug-in object(s) no program reaches" count. Files opened: about 75,000 down to
2,560; stats: two per file down to one. Warm time here: 2.0 s down to 1.3 s for
`audit`.

Not measured: the cold time on a spinning disk after the fix, because dropping
the page cache needs root. The next `ci gate` on 175 is the measurement.

