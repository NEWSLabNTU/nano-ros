---
id: 726
title: "The fixture build statically partitions 32 cores into 4x8, so 45% of its wall clock runs at a 25% CPU ceiling"
status: open
type: performance
area: build
related: [issue-0648, issue-0721, issue-0466, issue-0616, issue-0500]
---

# 0726 — fixture-build CPU utilization campaign

Goal: saturate the machine while building fixtures. This issue is the audit and
the measurement; fixes land against it.

## Measurement

From the `lane=tier2` build of 2026-08-20 (`build-test-fixtures.joblog`,
`tmp/build-test-fixtures-20260820-143826-886729/`), 32 cores, banner
`budget=32, make-jobs=5, pool=4x8 + zephyr=32 (solo)`:

| stage | duration |
| --- | --- |
| zephyr | 92 s (solo, order-only prerequisite) |
| threadx_linux | 35 s |
| qemu | 195 s |
| freertos | 198 s |
| native | 720 s |
| esp32 | 598 s |
| nuttx | 790 s |
| **threadx_riscv64** | **1302 s** |

Wall 1449 s. Concurrency profile, with each stage capped at `inner=8` jobs:

| stages running | wall time | core ceiling |
| --- | --- | --- |
| 1 | **655 s (45.2%)** | **8/32 = 25%** |
| 2 | 4 s (0.3%) | 16/32 |
| 3 | 70 s (4.8%) | 24/32 |
| 4 | 543 s (37.5%) | 32/32 |
| 5 | 177 s (12.2%) | 32/32 |

**Mean usable ceiling: 20.7 of 32 cores (65%).** And that is a *ceiling*, not
measured usage — actual utilization is at or below it.

The tail dominates: `threadx_riscv64` runs 1302 s, of which 655 s is alone on
the machine, permitted 8 cores out of 32.

## Mechanism

`justfile:build-test-fixtures-leaves` computes

```sh
outer=4
inner=$(( budget / outer ))     # 32/4 = 8
make_jobs=$((outer + 1))        # 5
```

then generates a make graph whose every stage recipe runs

```sh
env -u MAKEFLAGS -u CARGO_MAKEFLAGS ... <child gets -j $inner>
```

Two decisions combine into the ceiling:

1. **`outer=4` is hardcoded**, independent of `budget`. On a 32-core host that
   fixes the partition at 4x8 regardless of how many stages actually remain.
2. **The outer make's jobserver is explicitly stripped** (`env -u MAKEFLAGS`),
   so a child cannot draw tokens from a shared pool. Each child gets a static
   `-j8` allocation instead.

Static partitioning is fine while >=4 stages are running. It cannot reclaim
capacity as stages drain, which is exactly when the longest stage is still
going. 45% of the wall clock is spent in that state.

## The path that already exists and is not taken

`NROS_JOBSERVER=1` selects a different launcher: serial stage dispatch, with
children inheriting FIFO jobserver tokens rather than a static split. That is
the design that fixes this — one token pool, work-stealing, the tail stage
expanding to fill the machine.

It was not used by this run and appears not to be the default anywhere. Two
things to establish before making it one:

- **Is the pinned make still needed?** Several comments gate the jobserver path
  on "no pinned make 4.4" (`just/native.just:360,483`;
  `just/workspace.just:89` builds one from source). The system make here is
  already **GNU Make 4.4.1**, which has `--jobserver-style=fifo` natively. If
  the pin exists only to obtain 4.4, it may now be dead weight on this host
  class — verify before removing, since CI hosts may be older.
- **Does the serial launcher lose the overlap it currently gets?** Today 4
  stages genuinely run at once for 37.5% of the wall. A serial dispatcher that
  relies on intra-stage parallelism must actually saturate from one stage; if a
  stage's own graph is narrow (a long cargo chain), serial dispatch could be
  *worse*. Measure both, do not assume.

## Audit checklist

The campaign's other dimensions, with current state:

- [x] **Exhaustive I/O in gates** — issue 0721. Fixed: `check-no-std-stdio`
      >300 s -> 3 s, `check-example-leaf-target-dirs` >90 s -> 2 s, gate widened
      to read Python. Gates run before every fixture build, so this was pure
      serial dead time at the head.
- [ ] **Jobserver vs GNU parallel** — `just/native.just:323` still fans out with
      `parallel`; issue 0466 removed it as a tier-1 prerequisite but a
      `parallel -jN` inside a make graph is two schedulers each sizing to the
      whole machine. Audit every remaining site for whether it is under an outer
      jobserver.
- [ ] **Duplicate builds / identity** — 0616 (a `--target-dir` serves exactly
      one workspace root), 0500 (Corrosion sharing `cargo/build`), phase-340
      target-dir groups, `check-cargo-target-spelling`. Gates exist; confirm
      they still hold on a tier-2 build and that no leaf rebuilds a shared
      crate under a second identity. `cargo tree` cannot see this — compare
      fingerprint dirs.
- [ ] **Provisioning** — confirm cmake/ninja/sccache are the pinned ones on the
      build path and that sccache is actually hitting (phase-340 W3 measured
      0 hits / 62 misses when the `--target` spelling drifted).
- [ ] **The tail itself** — `threadx_riscv64` at 1302 s is 90% of the wall on
      its own. Even with perfect scheduling it bounds the build. Worth asking
      separately why it is 1.6x the next slowest.

## Do not measure this on a warm tree

Fixture builds are incremental and the page cache is hot after one run, so a
second run of the same lane measures almost nothing. Compare like for like: same
lane, same starting state, and record the banner line (`budget=`, `pool=`) with
every number, since it is the only evidence of which scheduler ran.

## Measured 2026-08-20 (second pass): the gate phase, and the jobserver launcher

Two results, and they redirect the campaign away from the scheduler.

### The 1449 s baseline undercounted the build by 481 s

The joblog begins at the FIRST STAGE, so everything before it was invisible to
the measurement above. Timed from process start to the first `== zephyr ==`:

```
start -> first stage:   481 s   at ~2 of 32 runnable (6%)
```

Eight minutes of `check-fast` — 115 gates, run serially as just dependencies —
at essentially one core. True wall for the lane is ~1930 s, of which **25% is
spent at 6% CPU before a single compiler runs**.

This is the largest single loss found so far and it is not a build at all. It is
also why issue 0721 mattered more than its own numbers suggested: those gates sit
on this serial path, so a gate that wastes 300 s wastes it with 31 cores idle.

### The jobserver launcher does not saturate either

`NROS_JOBSERVER=1` (serial stage dispatch, children inheriting FIFO tokens),
sampled from `/proc/loadavg` runnable count during the build stages:

| phase | mean runnable | peak |
| --- | --- | --- |
| zephyr + west fixtures | 4/32 | 12/32 |
| into native | 3/32 | 6/32 |

That is WORSE than the 4x8 static split, and it confirms the risk this issue
recorded rather than refuting it: serial dispatch only wins if one stage's graph
is wide enough to fill the machine. Zephyr's is not — west configure steps are
largely single-threaded, so one-stage-at-a-time leaves ~28 cores idle.

So neither existing option is right:

- **static 4x8** — good while >=4 stages run (37.5% of wall at a 32/32 ceiling),
  starves on the tail (45% of wall at 8/32).
- **serial + jobserver** — no tail problem by construction, but starves whenever
  the running stage is narrow, which is most of zephyr and the west fixtures.

The shape the evidence points at is BOTH: stage-level concurrency so narrow
stages overlap, AND a shared token pool so the tail stage can expand into the
capacity the others release. That is not either of the two launchers today.

### Corrected priority

1. **The gate phase (481 s at 6%)** — biggest, cheapest, and independent of the
   scheduler question. 115 gates that are almost all pure readers.
2. **A launcher that overlaps stages and shares tokens** — replaces the either/or
   above. Needs the measurement harness below to evaluate.
3. The `threadx_riscv64` tail (1302 s) — bounds the build under any scheduler.

### How to measure this honestly

`/proc/loadavg`'s runnable field sampled on an interval is enough to separate
"the scheduler permits N cores" from "N cores are busy", and the joblog cannot:
it records stage spans, so it can only ever produce a CEILING. Every number in
the first section of this issue is a ceiling; every number here is a sample.
Record which, always — the 65% ceiling above and the 3-4/32 measured here are
not in conflict, they answer different questions.

### Correction: the 481 s was cold and contended; check-fast is 90 s warm

The 481 s figure above is not reproducible and should not be quoted. Measured
again on a quiet machine:

| | |
| --- | --- |
| `just check-fast` end to end | **90 s** |
| sum of the 112 gates timed individually | 56 s |
| slowest single gate (`check-core-only-predicate`) | 8.3 s |
| other pre-stage deps (`generate-bindings`, `setup-launch-resolve`, …) | 5 s total |

The 481 s reading was taken immediately after killing an earlier build attempt,
so the cargo-invoking gates (the three fmt gates, `check-cargo-locked`,
`check-version-lockstep`) were resolving cold and contending. Cold-vs-warm on
this path is a 5x spread, which is exactly the trap the section below warns
about — and I walked into it while writing that warning.

What survives the correction is the SHAPE, not the number: `check-fast` runs 112
gates strictly serially as just dependencies, at 1-2 of 32 cores, and nothing
about that changes with cache temperature. It costs 90 s warm and several
hundred cold, always at ~5% CPU.

The distribution says parallelise rather than optimise: no outlier owns the
time (slowest 8.3 s, mean 501 ms), so there is no second 0721 hiding here. A
fan-out across 32 cores bounds the phase at roughly the slowest gate plus
overhead — order 10 s against 90 s serial, and far more when cold.

Independence must be established first, not assumed: `_check-skip-reset` is
ordered first by design, and some gates regenerate files (`check-cbindgen-
headers`, `check-abi-bindings`) rather than only reading.

### Gate fan-out: 90 s -> 8 s, but the gates are not all independent

`scripts/build/run-gates-parallel.sh` runs the same 111 gates concurrently,
derived from `check-fast`'s own dependency line so the list cannot drift:

```
serial `just check-fast`      90 s
parallel at -P32               8 s     (slowest single gate 7.7 s = the floor)
```

11x, and there is nothing left to win gate-by-gate — the distribution has no
outlier (mean 501 ms), so the floor is one gate's runtime.

**It is opt-in, not the default, because the gates are not independent.** On the
first full run `check-rmw-force-link-anchor` failed, reporting that a zephyr
example declares `rmw-xrce` without a `force_link_backend!` anchor. It passes
standalone, and two immediate re-runs of the whole fan-out were green. An
intermittent failure that cannot reproduce serially means some gate transiently
rewrites what another reads — a generated tree, or a leaf config.

That has to be found before `check-fast` switches over: a flaky gate is worse
than a slow one, because the response to a flaky gate is to stop believing it.
Note the fan-out is also what SURFACED this — the serial order has been hiding a
write/read pair between two gates that nothing else would have shown.

The shared skip log is not the culprit: `nros_check_skip` does a single short
`printf >>`, and one write under O_APPEND below PIPE_BUF is atomic.

### Hunting the racing gate pair: what is ruled OUT

The flake is real and reproducible in aggregate — roughly 1 full fan-out run in
5 — but it resisted every direct reproduction. Recorded so the next attempt does
not re-walk this:

| hypothesis | test | result |
| --- | --- | --- |
| another gate rewrites example sources | both fmt gates use `rustfmt --check`; non-comment scan of all 111 gate bodies for writers | **no gate writes** under `examples/` |
| `git ls-files` returns a partial list under index contention | 200 probes of the exact pathspec during a live fan-out | **never short** (steady 2/2) |
| `GIT_OPTIONAL_LOCKS=0` fixes it | 5 fan-out runs with it exported | **still failed on run 5** |
| latent bug in the gate, not a race | 40 standalone runs | **0 failures** |
| generic machine load | 40 runs of the script during a live fan-out | **0 failures** |
| something about `just` invocation | 25 runs via `just` during a live fan-out | **0 failures** |

It is also not one file: the failure named `action-server` once and
`service-client` another time, so whatever it is moves between the zephyr
examples.

What that leaves: a specific *other gate*, whose overlap window my external
loops never hit because they were not scheduled against it. The next step is to
bisect the gate set — run the fan-out with halves of the list plus the anchor
gate until the partner shows up — rather than more load testing, which has now
failed six different ways.

Until then `check-fast` stays serial and the fan-out is opt-in. The 11x is not
worth a gate nobody trusts.

### Bisection result: there is no single partner gate

Harness: run a candidate subset concurrently with N repeats of the anchor gate,
count anchor failures. Validated in both directions first — anchor alone at 25
and 60 concurrent copies never fails (so it does not race itself, and the
harness is not manufacturing the result), while all 110 candidates plus 25
anchors reproduces.

| subset | gates | attempts | anchor failures |
| --- | --- | --- | --- |
| none (anchor alone) | 0 | 3 (25, 60, 60 reps) | 0, 0, 0 |
| all candidates | 110 | 3 | 1, 0, 0 |
| first half | 57 | 3 | 0, 1, 0 |
| second half | 53 | 3 | 1, 0, 0 |

**Both disjoint halves reproduce it.** With the self-race excluded, that kills
the single-partner hypothesis: there is no one gate to find, and bisecting
further is pointless — every subset large enough to load the machine will test
positive.

What distinguishes a positive subset from the negative control is not WHICH
gates it holds but how much concurrent work it represents. 60 anchors alone are
32-way concurrent too, but they are 167 ms each and drain immediately; a subset
with real gates in it keeps the machine busy across the anchor's whole window.

So this is load-dependent behaviour inside the gate, not a cross-gate write/read
pair — which also means my earlier "generic load" tests were underpowered rather
than exculpatory: 40 sequential anchor runs beside a fan-out is a far weaker
probe than being scheduled inside one.

Next step is instrumentation, not more scheduling experiments: make the gate
dump the manifest text and the `git ls-files` result it actually saw when it
decides an anchor is missing, then run the positive control until it trips. Six
scheduling hypotheses and a bisection have now failed to identify it from the
outside; the gate has to report what it read.

### Instrumented: the read was COMPLETE, so the grep itself failed

Tripped on the third attempt of the positive control, with diagnostics:

```
cat rc=0, src_text bytes=4267
git ls-files returned:
  examples/zephyr/rust/talker/src/app_main.rs
  examples/zephyr/rust/talker/src/lib.rs
per-file re-read for force_link_backend!(nros_rmw_xrce_cffi):
  PRESENT on disk: examples/zephyr/rust/talker/src/app_main.rs  <-- read lost it
  absent: examples/zephyr/rust/talker/src/lib.rs (3387 bytes)
```

The arithmetic rules out a short read. app_main.rs is 896 bytes and lib.rs 3387,
so 4283 on disk against a measured 4267 — and `${#var}` counts CHARACTERS, with
7 non-ASCII lines across the two files making up the difference. Nothing was
lost. `cat` also returned 0.

So `src_text` DID contain `force_link_backend!(nros_rmw_xrce_cffi)` — the anchor
sits at byte 809 of 896, and a per-file re-read finds it — and

```sh
if ! printf '%s' "$src_text" | grep -q "force_link_backend!(${krate})"; then
```

still took the failure branch.

**`grep -q` returns 1 for "not found" and >=2 for an ERROR, and `if !` cannot
tell them apart.** Under a 32-way fan-out a forked `grep` can fail to start
(EAGAIN / resource limits) or be killed; either way the gate reads it as "the
anchor is missing" and reports a confident, specific, wrong finding.

That explains every observation the earlier hypotheses could not: load-dependent
but not tied to any gate (any subset heavy enough to strain fork does it),
moving between examples (whichever iteration loses the race), no file
corruption, `cat rc=0`, and clean standalone runs at any repetition.

This is a CLASS, not one site: `if ! ... | grep -q` conflates error with
absence everywhere it appears, and a gate that reports absence on error is a
gate that fails green->red under load and red->green never. The fix is to
capture the status and treat >=2 as a hard error:

```sh
printf '%s' "$src_text" | grep -q "$pat"; rc=$?
case $rc in 0) ;; 1) <real finding> ;; *) echo "grep failed (rc=$rc)"; exit 2 ;; esac
```

Worth a sweep of the gate scripts for the same shape before the fan-out becomes
the default — this is exactly the "fix the class" rule, and the fan-out is what
made a latent 1-in-N bug visible at all.
