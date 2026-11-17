---
id: 2682
title: "`fixture-tree.sh --self-test` walks, and may evict from, the caller's shared build caches"
status: resolved
type: bug
area: [ci, testing]
severity: medium
found: 2026-11-17
resolved: 2026-11-17
resolved_in: "fix(ci): the fixture-tree self-test sweeps only its own sandbox"
related: [2669, phase-559]
---

**Home:** [phase-559](../../roadmap/phase-559-ci-workflow-organization.md),
"Issues homed here". A follow-up to
[issue 2669](2669-hook-repo-side-effects-gate-unbounded-runtime.md).

## What happened

After the 2669 fix bounded every probe of `check-hook-repo-side-effects`,
`scripts/ci/fixture-tree.sh --self-test` overran its 120 s budget in all three
probe shapes (worktree, explicit, tmpdir). This was during a pre-push on host
139, while another gate held the host at a load average of 20 to 33. On its
own a minute later it took 1.0 to 1.5 s per shape, so it was about 80 times
slower under contention. No lock was involved. The bound worked as designed
and named the probe. The probe itself was the defect.

## Cause, measured

The self-test sandboxes its git repositories, but not the sweep it causes.

- `fixture-tree.sh sync` sweeps the tree before it syncs it
  (`runner-sweep.sh --disk --provision`). The self-test's inherited-`GIT_DIR`
  case runs a real `sync`, with the caller's environment.
- `runner-sweep.sh` resolves its roots from the environment. Inherited from a
  developer shell, `NROS_ZEPHYR_BUILD_ROOT` and `SCCACHE_DIR` name the shared
  caches, and the sweep measured both with `du`. On newslab-server-241 those
  are `/tmp/nros-build-aeon/zephyr` (88 GB, about 200,000 files) and
  `/tmp/nros-build-aeon/sccache` (32 GB, about 49,000 files).
- Warm, the two walks take about 0.1 s and 0.7 s. Under another gate's I/O
  load the directory entries are no longer cached, and each walk becomes
  disk-bound. In an `strace` of one run at a load average of 72, the `du` of
  the zephyr root alone took about 9 s.
- The two `sweep_tree --check` cases set some sandbox variables but not
  `SCCACHE_DIR`, so they walked the shared sccache too.

The walk was the slowness. Two further side effects were worse:

- the `sync` case's sweep was **live**, not `--check`, so a sweep over budget
  could evict from the shared zephyr build root;
- it wrote its high-water mark into the caller's
  `~/.local/state/nros-runner-sweep`.

## Exposure

- **Since when.** `check-hook-repo-side-effects` has probed
  `fixture-tree.sh --self-test` since 2026-10-13 (`b522535dba`, which also
  added the inherited-`GIT_DIR` `sync` case). `sync` has swept before it
  syncs since 2026-11-01 (`cc0a125938`). So from 2026-11-01 every run of the
  hook gate ran this live, unsandboxed sweep, three times per run (one per
  shape). The hook gate is on the fast line, so that is every pre-push and
  every `check fast`, on developer hosts and in CI. It predates the 2669
  change, which only made the overrun visible.
- **Whether anything was evicted: unknown in general.** The self-test
  discards the sweep's output, so its `rm -rf <path>` lines were never
  logged. The high-water file (`~/.local/state/nros-runner-sweep/high-water.tsv`)
  holds the LAST measurement of each area, not a maximum, so it cannot prove
  that no earlier run evicted.
- **How to tell on a given host.** The sweep evicts from the fixtures area
  only when that area exceeds its budget: `NROS_SWEEP_BUDGET_FIXTURES_GIB`,
  280 GiB by default, capped at 40% of the volume. The area includes
  `NROS_ZEPHYR_BUILD_ROOT`. A host whose zephyr build root, together with the
  tree's own fixture dirs, never approached that budget cannot have been
  evicted from. Where it may have been, an evicted entry is a whole directory
  under that root that is now missing, oldest modification time first, and
  the next build recreates it. The symptom someone would have seen is an
  unexpectedly cold Zephyr build, not a failure.
- **newslab-server-241, checked 2026-11-17.** The fixtures area measured
  88,362,631,168 bytes (82 GiB) against a 280 GiB budget, so no run on this
  host could have evicted. The real high-water file was rewritten by these
  runs (its rows are stamped 2026-11-17T15:06:18Z).

## Fix

One `in_sandbox` wrapper in the self-test now sets every root the sweep reads,
all under the test's temporary directory: `NROS_SWEEP_STATE_DIR`,
`NROS_SWEEP_WORK`, `NROS_HOME`, `XDG_STATE_HOME`, `SCCACHE_DIR`, and an empty
`NROS_ZEPHYR_BUILD_ROOT`. Every sweep the test causes runs through it,
including the `sync` case. A new assertion checks that the sweep's sccache
line names the sandbox. With the wrapper removed, that assertion fails and
names the shared cache (mutation-checked).

Measured on newslab-server-241, with the host already loaded by other work:

| | load average | wall time | `du` of a shared cache |
|---|---:|---:|---:|
| before | 72 | 4.98 s | 2 (zephyr, sccache) |
| after | 84 | 1.36 s | 0 |

The fix removes the cause rather than raising the budget. A budget scaled to
load would have hidden a self-test that reaches outside its sandbox.
