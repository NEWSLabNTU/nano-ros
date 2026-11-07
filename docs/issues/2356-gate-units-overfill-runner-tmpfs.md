---
id: 2356
title: "scaffold-builds and template-copy-out units all start at once under nextest and overfill the runner's /tmp tmpfs"
status: open
type: bug
area: [testing, ci, runner]
severity: high
found: 2026-11-07
related: [2239, 2318, phase-489, phase-543]
---

## Measured

Tier-2 run 42364894798, job 128383215067, on the self-hosted runner
`nano-ros-runner-newslab-118-server`, commit 9d0bd000a8, step `just ci matrix`,
in `check::default` from 11:39 to 12:23 UTC. Tier 1 was green in the same run.

At about 11:43 nextest started the build tier's unit tests. The log's `SLOW
[>360.000s]` lines show **44 of them running at once**: the 21 `scaffold-builds`
variants, about 21 `template-copy-out` units, and both gates' `@self-test`
units. Each one builds cold in its own scratch tree under `${TMPDIR:-/tmp}`
(`scripts/build/gate-scratch.sh`). From 11:52 they failed together:

- 8 gcc temp-file failures, e.g.
  `fatal error: error writing to /tmp/cc9TTIeF.s: No space left on device`;
- 10 `No space left on device (os error 28)`, and 2
  `LLVM ERROR: IO failure on output stream`;
- 62 `gate-units.sh: line 79: echo: write error: No space left on device`
  (the unit's own report file, which is on the same /tmp);
- scratch-root mentions: `/tmp/nros-template-copy-out.*` 53,
  `/tmp/nros-scaffold-builds.*` 11, `/tmp/nros-template-selftest.*` 4.

The failing gates were `scaffold-builds` (19 variants), `template-copy-out`
(11 units) and `census-hooks-complete`. Two more `template-copy-out` units
(`c-and-cpp-mixed-workspace`, `multi-node-workspace-cpp`) then ran to the
2400 s timeout.

The filesystem that filled was the container's /tmp, not `_work`. Inside the
container afterwards (it restarted at 13:14:43Z, so the tmpfs had been emptied):

    tmpfs           48G  108K   48G   1% /tmp      (HostConfig.Tmpfs "/tmp": "rw,exec,nosuid,size=48g")
    /dev/nvme0n1p2  3.6T  2.5T  940G  73% /home/runner/_work    (inodes 6% used)

### How big one unit is

Each of these units was run alone on newslab-118 from de94abdc04, with `TMPDIR`
set to a private dir, sampling `du -sk` every 5 s. All four passed.

| unit | peak scratch |
|---|---|
| `scaffold-builds#rust_project_talker` | 1.96 GiB |
| `template-copy-out#scaffold:linux` | 1.96 GiB |
| `scaffold-builds#cpp_project_talker` | 4.66 GiB |
| `template-copy-out#multi-node-workspace-cpp` | 4.69 GiB |

So 44 units at once need somewhere between about 90 and 200 GiB. A 48 GiB
tmpfs holds about ten of the C++ ones.

## Why

Two changes, each correct on its own, combined into this:

1. phase-489 (`59358821e8`, then `e071939525`) runs a gate's units in parallel
   *inside one gate process*, and `nros_gate_unit_jobs` caps how many by
   `$TMPDIR`'s free space divided by a per-unit size (default 3 GiB). That
   bound was added after six units filled the 8 GiB tmpfs in run 42258540290.
2. phase-543 W2 (`aebe984986`) makes each unit **its own nextest test**
   (`[group('units')]`, `NROS_GATE_UNIT=<unit>`). Each test process now runs
   exactly one unit, so `nros_gate_unit_jobs` has nothing to bound. Nothing
   on the nextest side replaced it: neither recipe has a `uses:` resource or a
   `cpu:` weight, and `.config/gate-lanes.toml` `[resources]` holds only
   `root-target = 2`. With 48 test threads, all the units start together.

Three things make it worse:

- the trees of FAILED units are kept as evidence (issue 2239's rule), so once
  units start failing for lack of space their trees hold on to that space;
- gcc's and sccache's temp files go to the same /tmp;
- the per-unit default of 3 GiB in `nros_gate_unit_jobs` is below the
  measured 4.7 GiB of a C++ unit.

The runner's /tmp cap has already moved 8 -> 24 -> 48 GiB
(`scripts/lib/runner-tmpfs.sh`, `NROS_RUNNER_TMP_SIZE`), each time after a
run like this one. A larger cap only moves the threshold while the number of
units grows; the scheduling is what has no bound.

## What would close it

- The bound moves to where the units are scheduled: the units of both gates
  are limited by nextest, as a shared `[resources]` group sized from the
  scratch filesystem's free space and the measured per-unit size, so the
  number running at once fits whatever the host's `$TMPDIR` holds. A fixed
  number is acceptable only if it is derived, not chosen.
- `nros_gate_unit_jobs`'s per-unit default reflects the measured size (or the
  per-unit size is stated per unit).
- A tier-2 run on newslab-118 passes `check::default` with the 48 GiB tmpfs.

Whether the runner should also point `TMPDIR` (and with it gcc's and
sccache's temp files) at `_work` is a runner change and the operator's call;
it is not needed for the scheduling fix above.

## Fix (landed; awaiting the tier-2 confirmation)

- `.config/gate-lanes.toml` `[resources]` gains `scratch-tree =
  { scratch-gib = 5 }`, and both recipes carry `[group('uses:scratch-tree')]`.
  `check-gate-lists.py` (`resource_capacity`) turns a `scratch-gib` resource
  into nextest's `max-threads` as `$TMPDIR`'s free space at config time over
  the per-unit size, never below 1, and 1 when free space cannot be measured.
  The harness generates that config per run, so the bound is the host's and
  the moment's: 9 on the 48 GiB runner tmpfs, 12 on a 63 GiB one, 182 on a
  931 GiB disk-backed /tmp.
- `nros_gate_unit_jobs`'s per-unit default is 5 GiB (it was 3, and
  `check-scaffold-builds.sh` passed 4), the measured 4.7 GiB rounded up.

The issue stays open until a tier-2 run on newslab-118 passes `check::default`
on its 48 GiB tmpfs.
