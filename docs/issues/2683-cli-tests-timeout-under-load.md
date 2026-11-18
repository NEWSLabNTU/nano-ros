---
id: 2683
title: "`check::build` gates time out at 2400 s together: `cli-tests`, `workspace-probes`, `workspace-all` and others, on dev hosts and on the daily CI shard"
status: open
type: bug
severity: high
area: [testing, gates, ci]
related: [issue-2380]
found: 2026-11-17
---

## Measured

Reported by session nros 139 on 2026-11-17. Both runs are `just ci gate`, and
the gate harness's per-test timeout is 2400 s. Both failures are in step 8,
`check::build`.

| run | tree | step 8 | `cli-tests` |
| --- | --- | --- | --- |
| B3, green | `p554-w3-slice-b3` | 57 min 04 s | **passed in 2319.7 s**, about 80 s under the timeout |
| B4, red | base `2cfe9f0094`, 13:33Z–15:23Z | 1 h 49 min 31 s | **timed out at 2400.03 s**, 228 of 230 tests done |

In B4, `build::check::workspace-probes` also timed out, at 2400.06 s.

Tests still running past 60 s when B4's `cli-tests` was killed:

- `rmw_resolver::tests::every_announced_backend_resolves_through_the_scan`
- `rmw_resolver::tests::the_baked_table_and_the_scan_agree_on_every_in_tree_backend`
- `serdes_resolver::tests::a_provider_in_the_user_workspace_reaches_the_default_search_path`
- `serdes_resolver::tests::the_nano_ros_root_is_scanned_despite_its_own_nros_ignore`

Also slow: the `configure_*` tests, `deleted_generated_dir_reconfigures`, and
`run_dry_run_why_after_an_edit_*`.

Load during B4, from the gate's token-pool report:

- host runnable threads: p50 6, p90 15, max 73, with other sessions' builds
  on the same host;
- free jobserver tokens: 0 in 51 % of the samples;
- `cli-tests` peaked at 17 runnable against its weight of 4, and
  `workspace-probes` at 14 against a weight of 1.

So the same suite on a busy host needs more than the timeout. Its green
margin, 80 s of 2400, is 3 %, and a gate whose verdict depends on how busy
the host is says nothing about the change under test. Issue 2380 is the same
shape for the `linux-check` gate.

### After the probe runtime sharing landed (2026-11-18)

Two more measurements, by session nros 139:

| run | tree | `cli-tests` |
| --- | --- | --- |
| B4 run 3, full gate, red | base `2cfe9f0094` | **timed out at 2400.05 s**; step 8 took 54 min 42 s, so the host was not heavily loaded. The unit tests passed in 3 s; the kill came inside the `--ignored` colcon flag parity phase |
| alone, `just check cli-tests` | slice B on main `5370be0bb9` (includes #4165) | **passed in 859 s**, compile included |

#4165 makes metadata probes with the same configuration share one compiled
runtime. It is the main change between the two rows, and the likely cause
of the drop, since the `configure_*` tests and the colcon flag parity phase
each run real `nros configure` builds. That cause is inferred, not measured
per test. Load was about 10 when the solo run started and about 115
when it ended, from another session's encoder run, so 859 s is not an idle-host
number.

That read it as HEADROOM, not an outage (859 s is 36 % of the timeout), and
lowered the severity. The next day's sightings below reversed that: the
class reached the daily CI shard, which blocks a release. What is
not yet measured is `cli-tests` inside a full gate on the new base, next to
the gates that run beside it in step 8; the "Done when" line below still
needs that number.

## Widened (2026-11-18): the step-8 class, not one suite

Four hosts saw the same shape: several `check::build` gates killed at 2400 s
in one run, each passing alone with time to spare.

| where | timed out | load | token pool |
| --- | --- | --- | --- |
| host 139, slice B gates (3 runs) | `cli-tests`, `workspace-probes` | host runnable p50 5-6 of 32 CPUs | free p50 13, empty 43 % |
| host 133, NXP #4171 (pre-#4165) | `required-features-tests`, `workspace-probes`, `workspace-all` | p50 5 | empty **95 %**, make lost 12 of 32 tokens |
| host 133, W2b (with #4165) | `workspace-probes` | p50 4 | empty 91 %, lost 13 of 32 |
| host 175, #4127 | `ament-census-sizing`, `cpp`, `census-hooks-complete`, `c`, `cli-tests` | p50 10, ~33 load | empty 35 % |
| **daily CI**, host-tests 44228702426 shard [2/3] | `workspace-all`, `template-copy-out#scaffold:runner-package` | GitHub-hosted `ubuntu-22.04`, 4 vCPU, p50 3 | 4-token pool, empty 91 %, lost 3 of 4 |

Alone, the same gates take: `workspace-probes` 375 s (139) and 540 s (133);
`cli-tests` 859 s (139).

### Measured on host 139 with a per-thread sampler

A scratch sampler read every lane thread's `wchan` each 5 s (Yama
`ptrace_scope=1` hides `/proc/<pid>/syscall` and fd links, so `wchan` is the
only view), classified as running, waiting on a pipe (a jobserver token, or a
parent reading a child's output), waiting on a child, or waiting on a file
lock, per gate. Two GREEN step-8 runs:

| run | step 8 | `workspace-probes` | its running / lock-wait (thread-s) | free tokens |
| --- | --- | --- | --- | --- |
| slice B, warm | 34 min | 493 s | 395 / 1630 | p50 24, empty 35 % |
| slice C, cold (nros-node changed) | 44 min | 926 s | 450 / 3130 | p50 22, empty 40 % |

- The dominant non-CPU wait is cargo's file LOCK. #4165 makes equal probe
  configurations share one cargo unit, so concurrent configures queue on its
  build-dir lock. That waiting replaces duplicate compiles; it is not a fault.
- Cold doubles it and still passes, so "cold plus lock" does NOT explain the
  reds. Neither run showed token starvation.
- The cold run also shows disk waits (`balance_dirty_pages` 984,
  `folio_wait_bit_common` 2371 thread-samples). The reds on host 139 coincided
  with heavy disk users (a post-verdict cache trim stuck in state D, an encoder
  run, the hook-repo self-test's cache sweep that issue 2682 fixed). That is a
  candidate, not a finding: no RED run has been sampled.
- The existing lane sampler (`scripts/build/run-load-sampler.py`) attributes a
  process by `NROS_GATE_LANE`, which is `dev` in every worktree, so another
  worktree's pre-push gates count as this lane's (host 133 saw `fast::check::*`
  inside step 8).

### First cause found: the daily shard (fixed here)

Shard [2/3] went green on 11-17 (91 min) and red on 11-18 (128 min). The
difference is `workspace-probes`, new in that shard (issue 2608). It runs 6
configures side by side (`NROS_WORKSPACE_PROBE_JOBS`, default 6) but declared
the default weight of 1, and peaked at 11 runnable on 4 CPUs. Its neighbours
tripled: `source-gates` 427 -> 1708 s, `scaffold-builds#c_project_server`
434 -> 1248 s, `workspace-all` 1324 s -> timeout.

Fix: the recipe declares `cpu:6`, the number it runs, and the script reads it
back with `check-gate-lists.py --weight workspace-probes`, so it is written
once. `cli-tests` reads its `cpu:4` the same way instead of restating it.
nextest clamps a weight above `--test-threads` (`linux-check`'s `cpu:8` runs on
the same 4-slot runner), so on that runner `workspace-probes` runs alone; on a
32-CPU host it takes 6 slots. Cost: the shard serializes it.

### A full gate with four first-wave timeouts (2026-11-18, phase-560 W9.d)

PR #4195 (its head after the rebase onto `09a5595bb4`), `just ci gate`, run gate3,
step 8 `check::build` red after 1 h 24 min 34 s: 79 tests run, 75 passed,
**4 timed out**, all in nextest's first wave and all killed at the cap
together:

| test | wall | where it was when killed |
| --- | --- | --- |
| `build::check::cpp` | 2400.03 s | after `failed_create_aborts: OK` |
| `build::check::cli-tests` | 2400.46 s | unit suites passed (230 in 191.65 s, then 28, 11, ...); killed in a later phase while cargo rewrote a lockfile |
| `build::check::ament-census-sizing` | 2400.46 s | `configure + build .../ament-census-port` |
| `build::check::census-hooks-complete` | 2400.46 s | after `OK (30 entry points hooked ...)`, running its 12 mutation tests |

Load over step 8, from the gate's token-pool report (168 samples):

- host runnable: p50 9, p90 48, max 85;
- this lane's runnable: p50 1, p90 19, max 68, against a pool of 20;
- free tokens: p50 18, p10 0, min 0; empty in 29 % of the samples.

The lane was mostly IDLE (runnable p50 1, 18 of 20 tokens free at p50), so the
four were not starved of CPU for 40 minutes; they were waiting on something.
The other 75 tests, `linux-check` ones over 1320 s among them, finished.
Steps 1-7 (fast, pristine-checkout included) were green.

A re-gate of the same head with `scripts/build/sample-build-wchan.sh` beside
it was queued and then cancelled before it reached the host lock, so no
`wchan` sample exists for this run; the head is being gated on another host.
What a sample should answer next time: which leaf process each of the four
is blocked in (a lock, a pipe, a child's `do_wait`) while the lane is idle.

A per-step wait table for this host (host 175), from
`NROS_GATE_WAITS=1 just ci gate` (#4230), will follow from #4223's gate.

### What the lane waits on, and what the cache wrapper costs (2026-11-18, host 139)

**How to sample a gate.** `NROS_GATE_WAITS=1 just ci gate` (also
`just check build`) samples every thread of THIS run's processes each 5 s.
A process belongs to the run that started it, through `NROS_GATE_RUN`, so
another worktree's gates never count. Each thread is classified by `wchan` as:
- `run`: on a CPU;
- `pipe`: waiting for a jobserver token or a child's output;
- `child`: waiting for a child to exit;
- `lock`: waiting on a file lock;
- `disk`: page or write-back I/O.

Step 8's report then prints a per-gate table of thread-seconds and keeps the
raw samples in `build/gates/waits/<run>/`. The table counts classes only; it
does not yet name which process holds a lock or a token.

**The mechanism measured on hosts 139 and 133.** cargo takes a jobserver token
for each compile and runs it through the `sccache` client. The client keeps
that token while it waits on the host's ONE shared `sccache` server. On host
139 the server was running 0-4 compiles at a time and was disk-bound:
- its cache sits at its 30 GiB limit;
- an average cache write took 1.36 s, against 0.65 s for an average compile
  and 0.52 s for a cache hit;
- the Rust hit rate was 53 %.

So tokens were held by processes doing no CPU work, which is the "pool empty
while the lane is idle" shape in every table above. The client already starts
the server with the jobserver variables stripped, so the server does not hold
tokens itself.

**A/B on host 139**, same tree (main with the `workspace-probes` weight and the
sampler), `NROS_GATE_WAITS=1 just check build`. "Cold" means one unique edit
to `nros-node` per run, so every dependent misses in cargo and in the cache.

| run | wrapper | total | `workspace-probes` | `cli-tests` | `workspace-all` | pool empty | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 warm | on | 2539 s | 49 s | 1207 s | 1577 s | 35 % | green |
| 2 warm | off | 1321 s | 36 s | 909 s | 1237 s | 21 % | green |
| 3 cold | on | 2234 s | 786 s | 1021 s | 2043 s | 32 % | green |
| 4 cold | off | 2533 s | 1476 s | 1548 s | 2197 s | 16 % | green |
| 5 cold | read-only cache server | 2901 s | 1788 s | 1972 s | **timeout** | 49 % | red |
| 6 cold | on (repeat of 3) | 3196 s | 1933 s | 1986 s | **timeout** | 27 % | red |

- Cold, the wrapper HELPS: 13 % less total time, and half the probes' time,
  because unchanged C/C++ and dependency compiles hit the cache.
- Warm, run 1 went first and still had compiles to do (`node-std-tests` 414 s
  against 46 s in run 2), so the 2x is order, not wrapper cost.
- With the wrapper, the pool is empty about twice as often. Freeing those
  tokens did not make the build faster.
- A read-only server, which keeps the hits and skips the slow writes, was not
  measurably better than run 6.
- Runs 3 and 6 are the same configuration three hours apart: +43 %, and red.
  In run 6, `cli-tests` spent 39,165 thread-seconds waiting on disk, against
  3-5 thousand in the other runs. Host load, the disk especially, decides more
  than any wrapper setting.

Decision: the wrapper stays in gate lanes, and no harness-owned server.

**Host 133, same fixes in** (#4220's weights, #4230's sampler), W9.d re-gated
on main: GREEN, `check::build` 1 h 28 min against 2 h 10 min before, with no
timeout. The pool was still empty 72 % of the time at lane runnable p50 3.
Thread-seconds:

| gate | run | pipe | child | lock | disk |
| --- | --- | --- | --- | --- | --- |
| `workspace-probes` | 1250 | 20620 | 53560 | 4360 | 120 |
| harness (`-`) | 5185 | 5185 | 49950 | 0 | 0 |
| `cli-tests` | 1215 | 8440 | 6100 | 4210 | 25 |
| `workspace-all` | 495 | 7405 | 14745 | 265 | 50 |

`workspace-probes` waited on a lock with 1-5 threads at a time, never a pile-up:
serial waits on one lock. A sampler that records each lock-waiter's command
runs beside host 133's next gate, to name it.

**Gates run more processes than their weight, and their peak is not their
weight.** Per-gate peak runnable across 9 runs on host 139 (32 CPUs, a
32-token pool):
- `workspace-features` 2-54;
- `scaffold-builds` 12-31;
- `cli-tests` 2-32, although its tests are capped at 4 threads;
- `template-copy-out` 13-27;
- `workspace-all` 5-24;
- `node-std-tests` 3-16.

On a 48-core host with no `sccache` at all, the same gates peak at 39-42.

A peak counts the tokens a gate's cargo held at its busiest moment. It is
bounded by the pool, scales with the host, and varies up to 25x between runs
on one host, so it measures the host as much as the gate. Issue 2625 makes the
weight a declaration instead: every gate that runs a parallel build declares
`cpu:<n>`, set at its measured knee, as `cli-tests`' 4 was. A peak ABOVE the
pool (`workspace-features`' 54 against 32) is a different thing: children
outside the jobserver, a bug of its own.

### sccache 0.8.2 against the pinned 0.17.0 (2026-11-18, host 175)

Host 175's SDK store held sccache 0.8.2, while the index has pinned 0.17.0
since 2026-09-11. `nros doctor` did not compare the two; #4272 adds that check. The store was moved to 0.17.0 inside the host gate lock, between gates.

Every run is a full `NROS_GATE_WAITS=1 just ci gate` of the SAME head,
#4223's final head before it merged, on host 175 (20 cores). Each starts after
`git clean -fdX` and the worktree setup recipes, so cargo's target is cold and
only the sccache cache differs. Load is the 1-min average sampled every 10 s.

| run | sccache | cache state | total | step 8 | load min / median / max | Rust hit rate | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A | 0.8.2 | warm from earlier gates | 64 min 45 s | 48 min 34 s | 13.4 / 34.3 / 152.5 (*) | not recorded (**) | green |
| B1 | 0.17.0 | first run on 0.17.0 | 54 min 10 s | 43 min 21 s | 11.6 / 32.9 / 171.9 | 35.0 % (all 34.5 %) | green |
| B2 | 0.17.0 | warm, the run after B1 (rerun, see below) | 59 min 16 s | 43 min 47 s | 12.1 / 39.4 / 205.5 | 63.1 % (all 63.6 %) | green |

(*) The load sampler started 35 min into run A, so its first 35 min are not in
the figures; sysstat does not collect on this host.
(**) The server idle-exits about 10 min after its last compile and takes its
counters with it, so a `--show-stats` after the gate can read zero. The next
runs poll `--show-stats` every 60 s instead.

B1 had 43413 compile requests: 9270 hits, 17580 misses and 15693
non-cacheable calls, with one cache error and a 10 GiB cache. C/C++ hit 3.2 %.

B2 had 43393 compile requests: 17070 hits, 9766 misses and 15687
non-cacheable calls, with one cache error. C/C++ hit 100 %. Its counters come
from the 60 s `--show-stats` poll, so the server's idle exit could not erase
them.

The first B2 attempt went red on an unrelated hang in `node-std-tests` (a test
helper spinning on an unbounded wait, fixed by #4291), so the row above is its
rerun on the same head.

What the three runs do and do not show:
- Step 8 on 0.17.0 was about 5 min shorter than A's in both runs, B2's at the
  highest median load of the three. That is inside the +43 % spread host 139
  measured for one configuration three hours apart (above), so it shows 0.17.0
  is not slower; it does not show it is faster.
- Warm, 0.17.0 hit 63.1 % of Rust compiles and every C/C++ compile; its first
  run hit 35.0 % and 3.2 %. A's rate is not recorded, so the hit rates compare
  0.17.0 cold with 0.17.0 warm, not 0.17.0 with 0.8.2.
- B1 is the one-time cost of switching, B2 the steady state against A.

### Load from other sessions, and the weights (2026-11-19)

- **Host 175, one gate under two loads.** W13's gate (a tree with
  `workspace-probes`' weight) lost `workspace-all` to the 2400 s kill at host
  load 39-50 from other sessions. A later run, with the gate lock keeping
  other gates off the host: `workspace-all` 466 s, `workspace-probes` 471 s,
  `check::build` 38 min, green. That is at least 5x, with two caveats. The
  trees differ (the second was rebased onto a newer main, plus test fixes,
  though not in `workspace-all`'s step). The second run's background load,
  from other sessions' builds, was not recorded.
- **Host 139, other gates on the old weights** (WAITS on): `check::build`
  3639 s and 3739 s green, and 4060 s red on a real failure. No timeouts. The
  pool was empty 53-63 % of the time at lane runnable p50 3.
- **Host 139, a red gate on the old weights, and it is the disk.** #4160
  (531dbbc39a), 2026-11-19 20:17-22:06Z: `check::build` red after
  1 h 29 min 44 s, against 61 and 63 min for two other gates on the same host
  and weights earlier that day. `cli-tests` (2400.09 s; every test that printed
  passed) and `workspace-probes` (2400.47 s) timed out. Thread-seconds:

  | gate | run | pipe | child | lock | disk |
  | --- | --- | --- | --- | --- | --- |
  | `cli-tests` | 945 | 5150 | 14235 | 1855 | **49350** |
  | `workspace-probes` | 1675 | 23480 | 90850 | 5880 | 16835 |
  | `workspace-features` | 510 | 1390 | 6590 | 60 | 17665 |
  | `required-features-tests` | 435 | 5675 | 8455 | 160 | 2825 |

  Lane runnable p50 2, p90 6, max 29, against a pool of 32. Free tokens p50 0,
  with the pool empty in 621 of 1050 samples (59 %). Host runnable p50 8, p90
  21, max 62; the busiest sample's other tenants were an FVP, a vLLM engine,
  Python and `sccache`. Over weight: `cli-tests` peaked at 26 against 4,
  `workspace-features` 22 against 1, `workspace-probes` 14 against 6.
  `cli-tests` waited on disk 52 times longer than it ran, so a weight alone
  does not cover it: the linker's threads (issue 2718) and a bound on
  concurrent disk-heavy gates are the next levers.
- **Host 133, the lock waits named.** With each lock-waiter's command recorded, every lock wait in `workspace-probes` is cargo's own
  `locks_lock_inode_wait`, 1-4 at a time: configures queuing on the shared
  probe build dirs. That is the intended trade, not a fault.
- **Declared weights (issue 2625, its PR follows this one).** Every
  build-tier gate declares a weight with a stated basis. At a builder default
  of 2, a cold `just check build` on host 139 took 3185 s with no timeout:
  `workspace-all` ran in 235 s instead of being killed. Its first above-pool
  catch, the host linker's threads outside the jobserver, is issue 2718.

## Not the fix

Raising the timeout. That moves the line and keeps the dependence on load
(the manager's direction, 2026-11-17).

## Fix direction

For the dev-host reds, once a red run is sampled: split the suite or make it faster, measured on a loaded host:

1. **Where the time goes.** Per-test wall time from both logs, and whether
   the scan tests (`rmw_resolver`, `serdes_resolver`) walk the whole checkout
   each, which a shared, once-per-process scan would answer.
2. **Split** `cli-tests` into gates of their own, so no single gate holds
   most of the work under one timeout.
3. **Weights.** Every gate that runs a parallel build declares `cpu:<n>`,
   from its measured knee (issue 2625), so the pool stops admitting more
   builders than the host can run. A peak above the pool, which means children
   outside the jobserver, is reported and fixed per gate.
4. **Not the wrapper.** The A/B above rules it out. The host-side cache state
   (full at its limit, slow writes, a busy disk) is host configuration and
   goes to the host's owner.

## Done when

- The daily build-tier shards are green, with their wall time stated against
  the 240 min job limit.
- A RED step 8 is captured with the per-thread sampler, so the dev-host reds
  have a measured cause, and its fix holds under the load that caused them.
- Every gate that runs a parallel build declares its weight, checked
  statically, so the next gate that under-declares is caught (the class
  `workspace-probes` was one of). A measured peak is not the check, because it
  depends on the host (above).

## Logs

On the reporting host: `/home/aeon/repos/nano-ros/.claude/worktrees/p555-rmw/tmp/`,
`gate-b3-full2.log` (green) and `gate-b4-run1.log` (red).

The A/B runs (host 139): the same directory, `ab-ab1-on-warm.log` through
`ab-ab6-on-cold-rep.log`, with the raw samples of each under that worktree's
`build/gates/waits/`.
