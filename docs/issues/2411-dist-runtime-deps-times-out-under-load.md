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
