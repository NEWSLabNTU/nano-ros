---
id: 1899
title: "Tier-2 nightly fixture build: compilers were SIGKILLed across five families at once, so crates fail with no diagnostic"
status: wontfix
type: bug
area: [ci, build, testing]
severity: medium
found: 2026-10-14
related: [1353, 1365, 1898, 0115, phase-466]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

Nightly run **38527151921** (schedule 07:15 UTC, main `756676e16`), job
**115730089599** `tier 2 nightly (pairwise cover)`, step
`just build tier2-nightly`, on `nano-ros-runner-newslab-118-server`. The job
was created at 07:15:29Z and waited 44 minutes for a runner, starting at 07:59:53Z (issue 1365).
Of the run's other jobs, 12 passed and 3 were skipped.

The fixture build ran with:

```
build-test-fixtures: budget=48, make-jobs=5, pool=4×12 + zephyr=48 (solo)
```

It failed in five families at once (nuttx, freertos, linux, qemu,
threadx_riscv64), and none of the failures carries a compiler diagnostic. The
log has no `error[E…]`. Instead:

```
sccache: Compiler killed by signal 9          (x3)
could not compile `core` (lib)
could not compile `serde_core` (lib)
could not compile `nros-platform-config` (lib)
could not compile `nros-node` (lib)           (x2)
could not compile `zerocopy` (lib)            (x2)
could not compile `ros-launch-manifest-model` (lib)  (x2)
```

Every `nros-c` consumer then panics in the size probe, at
`nros-build-helpers/src/shared.rs:61`:
`nros-c: size probe could not locate the `nros` rlib: … nested cargo build
failed … error: could not compile `<one of the above>``. On threadx_riscv64
the panic surfaces as `FAILED: [code=101] … _cargo-build_rv_virt_threadx_listener`.

Unrelated crates (`core`, `zerocopy`, `serde_core`) failing with no message,
beside three SIGKILLs reported by sccache, is the signature of the kernel
killing compilers under memory pressure. Disk is not the cause: the job
reported `923G free on the checkout's volume`.

## What this is NOT

- **Not 1353.** Disk was ample, and the shape is a killed process, not
  `No space left`.
- **Not 1898.** That issue is the 05:11 nightly's harness failing to *fetch* a
  git dependency. Here everything was fetched, and the compiles were killed.
- **Not a code defect in the crates named.** The same crates build in every
  merge-group gate, and the five families that failed have nothing in common
  but the machine and the moment.
- **Not proven to be the OOM killer.** The job log cannot show `dmesg`.
  Signal 9 from sccache plus diagnostic-free compile failures is strong
  circumstantial evidence, not a kernel record.

## What would close this

1. Confirm on `newslab-118-server`: the kernel log (`journalctl -k` /
   `dmesg`) between 07:59Z and 09:04Z on 2026-10-14 (the job's run window) should show `oom-kill` entries
   naming `rustc`/`cc1`. That needs a human on the host.
2. If confirmed, size the budget to memory, not to cores. Either derive
   `budget`/`pool` from `MemAvailable` (a rustc compile of `nros-node` costs
   GBs, and 48 concurrent ones may not fit), or cap the tier-2 nightly's pool
   on that runner. Measure peak RSS for one family before choosing a number.
3. Make the failure say so. A fixture build whose compile was SIGKILLed
   should print "killed (likely memory)" instead of letting a size-probe panic
   about a missing rlib be the first readable line.

## Second sighting, same host: the runner itself died (2026-10-14 18:38Z)

Run-matrix **38615001083** (workflow_dispatch, main `36ec94ba0`), job
**116027718879** `tier 2 (1-wise matrix)` on
`nano-ros-runner-newslab-118-server`, ran from 18:25:21Z to 18:38:23Z. It has
no failing step. The log is `BlobNotFound`, and the only annotation reads:

```
The self-hosted runner lost communication with the server. Verify the machine
is running and has a healthy network connection. Anything in your workflow
that terminates the runner process, starves it …
```

That is the runner-death shape (the one 1353 also describes). It is recorded
here rather than under 1353 because it is the same host and the same lane as
the 07:15 SIGKILLs above, and an OOM killer that picks the runner agent instead
of a `rustc` produces exactly this. It is **not** evidence of OOM on its own:
the kernel log covering 18:25–18:39Z on `newslab-118-server` is what would
separate memory from a network drop or a container restart. Item 1 of "What
would close this" now has two windows to check.

## Third sighting: tonight's nightly, same host, runner gone mid-build (2026-10-15 05:27Z)

Nightly **38685461000** (schedule 05:11Z, main `b6909e6ed`), job
**116259166114** `tier 2 nightly (pairwise cover)` on
`nano-ros-runner-newslab-118-server`, ran from 05:11:43Z to 05:27:45Z. The other
25 jobs passed and 5 were skipped. Step 6, `just build tier2-nightly`, was still
`in_progress` when the job ended, so it died during the fixture build again. The
log is `BlobNotFound`, and the only annotation is:

```
The self-hosted runner lost communication with the server. … Anything in your
workflow that terminates the runner process, starves it for CPU/Memory, …
```

That makes three tier-2 deaths on this host in under 24 hours, all during the
parallel fixture build, which is when memory peaks: 07:59–09:04Z (compilers
SIGKILLed), 18:25–18:38Z, and now 05:12–05:28Z. The kernel log for those three
windows is still the missing evidence (item 1 under "What would close this"),
and it now has three places to look.

## No recurrence in eight days, and item 3 done (2026-10-23)

Every self-hosted job of `nightly` and `run-matrix` created from 2026-10-15
12:00Z to 2026-10-23 was scanned: 35 jobs, on `newslab-118-server` (24),
`newslab-server-241` (9) and `newslab-133` (2). None of their logs has a
`signal 9` line or a `lost communication` annotation. 19 of them ran the
fixture build at the same `budget=48, pool=4×12 + zephyr=48` as the 10-14
sighting. Two had `No space left on device` (10-17 on newslab-133, 10-21 on
newslab-118-server), which is issue 1353, not this.

So nothing in the repository changed and the kills stopped. The cause is
still unattributed: item 1, the host kernel log for the three windows above,
is the only thing that can say whether it was memory, and it needs someone
on `newslab-118-server`. Item 2 waits on item 1.

**Item 3 is done.** `scripts/build/log-first-errors.sh`, the excerpt printer
both fixture schedulers call, now counts the lines a killed compile leaves
(sccache's `Compiler killed by signal N`, cargo's `(signal: 9, SIGKILL`, gcc's
`Killed signal terminated program`) and names them before the error lines.
Replayed on the 10-14 job log (115730089599), it prints `3 compile(s) … were
KILLED by a signal and printed no diagnostic` above the `could not compile`
line for `ros-launch-manifest-model`. Self-test on the fast line:
`just check log-first-errors`.


## Resolution (2026-10-23) — wontfix: runner configuration, not a project defect

Closed by maintainer decision, on the same ground as issue 1353. Whether a
self-hosted runner has the memory for the lane's fixture build is the
operator's to provision, and nothing in the repository changed when the kills
stopped: 35 self-hosted jobs since 2026-10-15, 19 of them at the same
`budget=48, pool=4×12 + zephyr=48`, show no `signal 9` and no lost runner.

What the project owed is done. Item 3 makes a killed compile name itself:
`scripts/build/log-first-errors.sh` counts the kills and prints them ahead of
the error lines, with a self-test on the fast line (`just check
log-first-errors`). If it recurs, the log says so in its first lines, and
the kernel log on the runner host (item 1) is the evidence to collect.

Not done, deliberately: item 2 (size `budget`/`pool` from `MemAvailable`).
Its cause is unattributed and its cap unmeasured, so it would be a guess
defending against a symptom that has stopped. Reopen with a kernel-log
record of an `oom-kill` naming `rustc` or `cc1` if that changes.
