---
id: 726
title: "The fixture build statically partitions 32 cores into 4x8, so 45% of its wall clock runs at a 25% CPU ceiling"
status: resolved
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
| `just check fast` end to end | **90 s** |
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
serial `just check fast`      90 s
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

### Sweep: 87 sites conflate grep ERROR with grep NOT-FOUND

`git grep` over `scripts/`, `just/`, `justfile`:

| shape | sites | what an ERROR becomes |
| --- | --- | --- |
| `if ! … grep -q` / `grep -q … \|\| fail` | **50** | a FINDING is reported that is not real |
| `if … grep -q` / `grep -q … && …` | **37** | a check silently does NOT fire |

Both are wrong and they fail in opposite directions. The negated form is the one
that bit here: it turns a failed fork into a specific, confident, false claim
about the source tree, and it does so *only under load* — green to red when the
machine is busy, never the reverse, which is exactly the pattern that teaches
people to re-run a gate rather than believe it. The positive form is quieter and
arguably worse: a check that skips itself reports OK.

Fixed here: `check-rmw-force-link-anchor`, the one with a demonstrated failure.
It now captures the status and treats >=2 as a hard `exit 2` with a message
saying the tool failed rather than the tree being wrong. The positive control
(110 gates + 40 concurrent anchors) ran 6/6 clean afterwards against roughly
1-in-3 before — suggestive, though the guarantee is structural rather than
statistical: an error can no longer be reported as a finding.

The other 86 are NOT fixed and should not be swept blind. Each needs its own
reading, because for many of them `grep -q` is scanning a string that is
certainly present-or-absent and an error genuinely cannot occur, while for others
(anything scanning a file list, or piped from a subshell) it can. A mechanical
rewrite would churn 87 sites to fix an unknown fraction.

What would make this tractable is a shared helper plus a gate, the same shape as
the other recurring classes here:

```sh
nros_grep_q <pattern> [file...]   # 0 = match, 1 = no match, exits 2 on tool error
```

Then a gate rejects bare `grep -q` in a conditional in `scripts/`. That is the
structural fix; until it exists this class will keep being reintroduced, because
the wrong spelling is the natural one and is correct almost all of the time.

### Measured properly: the build is BLOCKED, not scheduler-capped

Earlier utilization figures in this issue came through a sampler that matched
itself (`pgrep -f "just build-test-fixtures"` appears in the sampler's own
command line, so the loop never exits and reports a build running long after it
ended — one figure was taken with no build alive at all). Replaced by
`scripts/build/sample-build-lineage.sh` (the `sample-build-cpu.sh` named here
was itself withdrawn and deleted — see below), which tracks the build by the PID captured
at launch and counts build tools by exact `comm` name rather than reading
`/proc/loadavg`'s global runnable count.

Pooled launcher, lane=tier2, 762 samples over 1543 s on 32 cores:

| | |
| --- | --- |
| runnable build tools | **mean 8.0/32, median 4, peak 51** |
| alive build tools | **mean 51, peak 156** |

| runnable compilers | share of run |
| --- | --- |
| **0 (fully idle)** | **24.8%** |
| 1-4 | 29.7% |
| 5-15 | 28.9% |
| 16-31 | 13.5% |
| 32+ | 3.1% |

**`alive 51` against `runnable 8` is the finding.** The build has ~51 tool
processes in existence and ~8 on CPU. Capacity is not being withheld from
runnable work; the work is not runnable. A quarter of the build has NO compiler
on CPU at all.

That reframes the whole campaign. Stage overlap was necessary — the static 4x8
split genuinely capped the tail at 8/32 — but it is nowhere near sufficient, and
no launcher can be, because the launcher's job is handing out permission to run
and permission is not the scarce resource.

It also bounds what the earlier work was worth: the gate fan-out (90 s -> 8 s)
is real and lands on the serial head, but the 1543 s body is 25% utilized for
reasons no scheduler change addresses.

Oversubscription is reduced but not gone: 3.1% of samples exceed 32 runnable,
peak 51, after NROS_INHERIT_JOBSERVER stopped stages handing ninja an explicit
`-j`.

Two stages failed this run — esp32 (rc=101) and native (rc=2) — and they passed
under the pooled launcher on the previous run. Unresolved whether that is
pooled-mode fallout or independent flake; not yet investigated.

### Next: why are 51 processes alive and 8 running

The question is now "what are they waiting ON", which is a different instrument.
Candidates, in the order they are worth checking:

* **cargo's package-cache lock (issue 0648)** — the original anchor of this
  campaign, and the obvious suspect once several stages run cargo at once. A
  contended `~/.cargo` lock serialises resolution across every concurrent leaf.
* **configure serialisation** — cmake configure, `west`, and codegen are
  single-threaded by nature. If a stage spends most of its span configuring,
  its own graph cannot fill any number of cores handed to it.
* **process-spawn overhead** — 156 alive at peak against 51 mean suggests
  churn; a build that spends its time forking short-lived tools shows exactly
  this shape.
* **I/O waits** — plausible on a tree this size, and distinguishable from the
  above by D-state vs S-state.

`wchan` sampling separates these directly: a futex wait is a lock, `do_wait` is
a parent blocked on children, `pipe_read` is a shell pipeline, D-state is I/O.
That is the measurement, and it should be taken before any more scheduler work.

### WITHDRAWN: the blocking-reason numbers measured another build

The "cmake dominates / 22.6% I/O waits / 1.8% futex" breakdown, and by extension
the `alive 51 / runnable 8` reading above, are **not trustworthy**. Both
samplers matched processes by `comm` GLOBALLY, and a `colcon2deb` Autoware build
was running in a container on the same host — 22 processes, including the cmake
whose command lines turned out to be `autoware_universe` and
`/output/workspace/src/…`, not nano-ros at all.

So those figures describe someone else's build sharing this disk. Withdrawn
rather than caveated: the direction of the error is unknown.

The conclusion they supported — "blocked, not scheduler-capped" — may still be
right, since a 51-to-8 gap is large, but it has not been re-measured and should
be treated as provisional until a lineage-scoped sampler runs on a quiet box.

Two measurement bugs, one root cause: **sampling by name instead of by
lineage.** The earlier `pgrep -f` self-match was the same mistake in a different
costume. Standing rule now recorded in
[phase-371](../roadmap/archived/phase-371-build-cpu-utilization.md): scope samplers by
process tree, and check what else is running before trusting an absolute number.

What survives, because it does not depend on live process sampling:

* the joblog concurrency profile (recorded stage spans)
* `check-fast` 90 s -> 8 s (wall-clock of a single command)
* the CMake configure trace — a single-process profile, immune to what else is
  on the box, which is why it is the finding this campaign now rests on:
  `nros_resolve_corrosion` is 20 s of a 33 s leaf configure.

## Priority 1 done (2026-08-26) — the gate phase was one gate, and it was walking build output

The "Corrected priority" above puts the gate phase first. It has since been
PARALLELISED (`check-fast (parallel) … at -P32`), which the sections above
predate — so the serial-dependency shape they describe is already gone. What
remained was worse and simpler: **one gate dominated the parallel phase.**

Its own banner said so, and nobody read it:

```
check-fast (parallel): 121 gate(s) OK at -P32; slowest check-deploy-board-resolves 1469201ms
check-fast (parallel): 116 gate(s) OK at -P32; slowest check-deploy-board-resolves 1392852ms
```

**23-24 minutes, one gate**, on the critical path of every `just check` and every
`build-test-fixtures`.

### Cause: a fourth spelling of the walk this repo already banned

`check-deploy-board-resolves.py` and `check-site-config.py` both did

```python
for pat in ("examples/**/system.toml", "packages/**/system.toml"):
    glob.glob(os.path.join(ROOT, pat), recursive=True)
```

`examples/` and `packages/` are the two trees holding build output — measured
**5769** `Cargo.toml` on disk against **237** git tracks. The walk must descend
every `target/` and `build-*/` tree to produce paths it then discards.

Issue 0721 already banned this and shipped `scripts/lib/tracked.py` for it. The
gate that enforces it, `check-no-tracked-file-find.sh`, missed both because its
pattern required a LITERAL `**` right after `.glob(`:

```python
PY_WALK = re.compile(r"\.rglob\(|\.glob\(\s*[\"']\*\*|\bos\.walk\(")
```

Here the pattern is a variable and the path is computed, so neither alternative
matched. The rule the file states is "EVERY recursive walk"; the regex was
narrower than the rule — issue 0196's shape, and the same
second-spelling failure `tracked.py`'s own docstring warns about.

### Fixed

Both scripts converted to `tracked()`, and `PY_WALK` widened with
`recursive\s*=\s*True`, which matches regardless of where the pattern or the
root came from.

Widening earned itself immediately: it flagged a SECOND recursive glob in
`check-deploy-board-resolves.py` that I had not noticed, in the function above
the one I had measured. Fixing only the measured site would have left the other
paying the same cold walk.

| | before | after |
| --- | --- | --- |
| `check-deploy-board-resolves` | 231.83 s warm (23-24 min inside `-P32`) | **0.09 s** |
| `check-site-config` | — | 0.06 s |
| whole `check-fast-parallel` | dominated by the above | **10.77 s, 124/124 OK** |

Verdicts unchanged: "20 distinct value(s), 36 descriptor alias(es)" and "30
deploy block(s) across 5 board(s) … 36 board name(s)".

Numbers are WARM, per this issue's own warning. The point of the fix is that it
removes the cold cliff rather than moving it: an index read is one file whatever
the page cache holds, which is why 0721 measured `git ls-files` at 0.002 s
against a walk that had not finished in 300 s.

### What is left

Priorities 2 and 3 are untouched: a launcher that overlaps stages AND shares
tokens, and the `threadx_riscv64` tail (1302 s) that bounds the build under any
scheduler. Both still need the measurement harness this issue describes.

## Priority 2 detour (2026-08-26): the configure finding was a DEAD `find_program`

The section above left "a launcher that overlaps stages and shares tokens" as
priority 2, and the CMake configure trace as the one live-process finding that
survived the sampler contamination — `nros_resolve_corrosion` at 20 s of a 33 s
leaf configure. Chasing that number found a defect, though not the one the
number described.

### The number did not reproduce; the shape did

A leaf configure here is **3.0 s warm**, not 33 s. The 33 s reading has the same
cold-vs-warm problem this issue has already recorded twice, so it is not quoted
further. What the trace DID show is real and does not depend on temperature:

```
execute_process   2.48 s of 2.95 s   (84%, 44 calls)
```

and, in the per-site attribution, paths that should not have existed at all:

```
1.17 s  FetchContent.cmake:1921   ${CMAKE_COMMAND} --build .
0.08 s  tmp/leaf-build2/_deps/corrosion-src/cmake/FindRust.cmake:352
```

`_deps/corrosion-src` means the leaf **cloned Corrosion from GitHub at configure
time** — while `/home/aeon/.nros/sdk/corrosion` held a provisioned 0.6.1.

### Root cause

`find_program` short-circuits when its result variable is already defined. That
is the caching contract, and **an empty string counts as defined.** So:

```cmake
set(_NROS_CLI "")            # normal variable, now DEFINED
find_program(_NROS_CLI nros) # <-- does nothing; _NROS_CLI stays ""
```

Demonstrated rather than reasoned about:

```
-- after find_program: []                                      # with the pre-set
-- no pre-set:         [/…/packages/cli/target/release/nros]   # same call, no pre-set
```

`_nros_corrosion_store_dir` therefore returned empty for every configure that
did not pre-set `_NANO_ROS_CODEGEN_TOOL`, skipped `find_package` entirely, and
fell through to the FetchContent branch. Issue 0500's whole store-ordering
apparatus was dead code on that path.

### Who was actually affected — NOT the CI lanes

Every `just` platform lane passes `-D_NANO_ROS_CODEGEN_TOOL` in its
`base_defs` (freertos, native, nuttx, threadx-linux, threadx-riscv64,
zephyr-dev, zephyr-setup), so they took the first branch and never reached the
dead search. The fixture build was not cloning Corrosion.

What was affected is the **documented user flow** — `book/src/getting-started/
first-project.md` and `workspace-cpp.md` both say

```
cmake -S . -B build -DNANO_ROS_ROOT=<path-to-your-nano-ros-checkout>
```

with no codegen-tool `-D`, as does
`examples/templates/multi-package-workspace/build-all.sh`. Those are the
copy-out projects RFC-0026 makes the primary consumer shape.

An earlier draft of this section claimed the 116 build dirs carrying
`_deps/corrosion-src` were evidence of scale. Checked properly: **118 of 119 of
them ALSO have `Corrosion_DIR` pointing into the SDK store**, so they are
residue from an earlier configure, not live FetchContent builds. Exactly one is
a true fetch-only dir. The claim is withdrawn; the impact below is the one that
holds.

### The consequence, measured

Same leaf, same tree, full PATH, three fresh configures each:

| | | |
| --- | --- | --- |
| before | 2.44 / 2.05 / 1.96 s | via FetchContent |
| after | 0.73 / 0.82 / 0.80 s | via SDK store |

And the sharp one — with the network blocked, on a host where
`nros setup --tool corrosion` HAS been run:

```
OFFLINE, before fix:  rc=1   Failed to clone repository: 'https://github.com/corrosion-rs/corrosion.git'
OFFLINE, after fix:   rc=0   via SDK store
```

The pre-fix run advises the remedy the user already applied:

```
-- nano-ros: Corrosion not provisioned — fetching v0.6.1 from git.
   Install it offline-safe with:  nros setup --tool corrosion
```

That is issue 0628's failure mode returning through a different mechanism: it
was fixed for the "installed where I did not look" case and reappeared as
"never looked at all".

### It is a CLASS — two more sites, both with quiet fallbacks

A sweep for `set(VAR …)` followed by `find_*(VAR …)` found three sites, and the
reason none had been noticed is that all three pair the dead search with a
legitimate-looking fallback:

| site | what the dead search cost |
| --- | --- |
| `cmake/NanoRosCorrosion.cmake` | FetchContent clone instead of the SDK store |
| `cmake/toolchain/riscv64-threadx.cmake` | never asked the store for libstdc++; silently took the "no SDK libstdc++" fallback |
| `zephyr/cmake/nros_rmw_cyclonedds.cmake` | skipped the PATH rung, fell to the host-idlc search |

Two of them predicted this in their own comments. The threadx one:

> Here the failure is quiet by design — the "no SDK libstdc++" fallback below is
> a real configuration — which is precisely why it could sit unnoticed.

and the cyclonedds one describes ending in "a FATAL_ERROR advising three
remedies that were all already satisfied". Both were right, and both were
describing a search that could not run.

The riscv64 candidate exists on this host
(`…/riscv-none-elf/lib/rv64imafdc_zicsr/lp64d/libstdc++.a`), and 10 C++ fixture
rows build on that platform — but that lane passes `-D_NANO_ROS_CODEGEN_TOOL`,
so it took the first branch and its behaviour is unchanged. The fix matters
there for the same non-lane configures as above.

### Fixed

* `NanoRosCorrosion.cmake` now calls the SHARED resolver
  `nros_resolve_cli(<out> OPTIONAL …)` (issues 0219 / 0325) — whose own comment
  records that it exists to stop bespoke copies, and this was the sixth. It
  already honours `_NANO_ROS_CODEGEN_TOOL` first, so 0754's precedence is kept.
* The other two use a distinct result variable instead. Deliberate asymmetry:
  a toolchain file is re-evaluated for every `try_compile`, so an include there
  is paid per probe; and the cyclonedds rung runs before the interface
  generator is included and must degrade rather than FATAL.
* New gate `check-cmake-find-program-shadowed` (12 self-tests, on the fast
  line). Negative control: it reports all three sites against `HEAD`, zero
  after. It is the durable half — the wrong spelling is the natural one.

Also corrected while here: `check-cmake-corrosion-prefix`, named as a live gate
by CLAUDE.md and twice by `NanoRosCorrosion.cmake`, **does not exist** — issue
0625 records phase-365 W3a retiring it deliberately when the prefix became
constructed. Three stale references now say so.

### Priority 2 and 3 still stand

This was a detour into the configure, not the launcher. Stage overlap plus a
shared token pool is untouched, and so is the `threadx_riscv64` 1302 s tail. The
lineage-scoped sampler on a quiet box is still the prerequisite for both.

## Priority 2 CLOSED (2026-08-26): the launcher was never the lever

Not by building a third launcher. The pooled one asked for here — stage overlap
AND one shared token pool — already exists as `NROS_BUILD_POOL=1` (phase-371 W3,
commit `5a403b4a8`), and its structural win is proven from the joblog: all 7
non-zephyr stages start at the same instant, mean 3.29 in flight against the
static launcher's hard cap of 4.

What closes the item is that phase-371's lineage-scoped measurement — the
harness this section said was a prerequisite — answered the question underneath
it:

| | |
| --- | --- |
| build's own processes alive | mean 36.3 |
| **runnable** | mean **0.1**, median 0 |
| share of run with ZERO runnable | **92.6%** |
| loadavg runnable (cross-check) | mean 1.9 |

A build that is 92.6% idle *by its own processes* cannot be helped by a
scheduler that hands out permission to run. Permission was never the scarce
resource. Both launchers were solving a problem the build does not have, and so
would a third.

**The default stays static 4x8, deliberately.** Flipping to pooled would be an
unmeasured change in the direction the evidence argues against: the build's
constraint is I/O, and pooled admits more concurrent stages, so it could
plausibly be slower rather than faster. The cold A/B that would settle it costs
several hours per run (native alone has run 655-4051 s) and this issue's own
data says the result cannot matter much either way. Pooled remains available,
opt-in, structurally better, and proven green 8/8 — the right shape for anyone
who later has a reason to want it.

### Correction: there were never "86 unconverted walk sites"

phase-371 wrote that its wchan result "raises the value of issue 0721's 86
unconverted walk sites". Two different sets got merged: 0721 is the WALK class
(resolved), and the 86 are 0726's own `grep -q` error-conflation sites, which
have nothing to do with traversal I/O.

Counted properly, the walk class has **9** live exemptions, and every one is
legitimately outside the git index — build output (`dep-closure`,
`prune-superseded-artifacts`), UNTRACKED leaf `target/` dirs the index cannot
see by definition, `--self-test` temp trees, the Zephyr submodule, an installed
ROS include tree. There is nothing left to convert.

### Re-measured: the gate phase's I/O signature is gone

phase-371's wchan table was taken BEFORE the `check-deploy-board-resolves` fix
earlier in this issue (231.83 s -> 0.09 s). Same instrument
(`sample-build-wchan.sh`), same target — that table explicitly characterises the
GATE PHASE, not the compile phase — re-run afterwards:

| | before | after |
| --- | --- | --- |
| leaves RUNNING | 1% | **34%** |
| leaves in D (uninterruptible disk) | **64%** | 7% |
| leaves in S | 35% | 58% |

Leaf blockers, before and after:

```
before:  468 python3  __wait_on_buffer          disk
         245 awk      pipe_read
         243 grep     folio_wait_bit_common     page cache / disk
         171 python3  d_alloc_parallel          dentry — PATH LOOKUP contention

after:    19 bash     pipe_read
          14 sleep    hrtimer_nanosleep
          11 sort     pipe_read
          11 awk      pipe_read
           9 cargo    jbd2_log_wait_commit
```

The directory-traversal signature — `__wait_on_buffer`,
`folio_wait_bit_common`, `d_alloc_parallel` — is **absent**, and the running
leaves are now `python3` (30) and `grep` (11) doing actual work. That confirms
the mechanism rather than merely the wall clock: the gates were not
"I/O-bound by nature", they were walking gigabytes of build output, and one
gate was most of it.

Limits, stated plainly: 593 process-samples over ~11 s against 5774 over 791 s,
so the after-sample is much smaller; and the page cache is warm. **I cannot test
cold — dropping caches needs root, which is not available here.** The
warm-to-warm 231.83 s -> 0.09 s comparison is what isolates the walk from cache
temperature, and 0721's own `git ls-files` at 0.002 s against a walk that had
not finished in 300 s is why the structural argument holds cold.

### What is actually left

Priority 3, unchanged: the `threadx_riscv64` tail at 1302 s, 1.6x the next
slowest stage, bounding the build under any scheduler. It is now the only
untouched item, and it is a question about one stage's own graph, not about
scheduling.

## Priority 3 answered (2026-08-26): measured, and it is not threadx's fault

"Worth asking separately why it is 1.6x the next slowest." Asked, with the
lineage sampler on a quiet box (loadavg 0.16, zero foreign build processes
before starting — checked, per this issue's own standing rule).

`just threadx_riscv64 build-fixtures`, 32 cores:

| | |
| --- | --- |
| cold | **1706 s** (the joblog's 1302 s, reproduced) |
| immediate warm re-run | **494 s** |
| occupancy cold | alive 27.4, **runnable 0.10** of 32 |
| occupancy warm | alive 14.0, **runnable 0.05** of 32 |
| running leaf processes, cold | 0 for **93%** of instants, 1 for 7%, never 2+ |

The warm number is the finding. That run did **zero** cmake configures and
**zero** compilations — its entire log is 70 `Finished` lines. It takes 494 s to
determine that nothing needs doing, because each of 70 cargo invocations
re-scans its own private fingerprint database at ~7 s each.

Cause: every C/C++ leaf is a standalone cmake project, so Corrosion puts the
cargo `--target-dir` inside that leaf's build dir, and each leaf rebuilds the
same staticlib. Five concurrent cargos were sampled with byte-identical
arguments — same package, features, target, crate-type — differing only in
`--target-dir`. One run wrote 21 fresh `libnros_c.a` and 14 `libnros_cpp.a`.
sccache cannot absorb it: `Non-cacheable reasons: crate-type 94` — it does not
cache `--crate-type=staticlib`.

**But the premise of the question is wrong.** This is not a `threadx_riscv64`
defect. The per-leaf cargo dir is the shape everywhere — 29 under
`qemu-riscv64-threadx`, 24 under `threadx-linux`, **59 under `native`** — and
native has more of them while finishing faster (720 s against 1302 s). So what
puts threadx on top is the per-invocation cost of the cross target, not a
structure unique to it. Isolating that needs a comparable cold measurement per
platform, which was not run, so no per-platform ranking should be quoted from
here.

Excluded by direct check rather than by argument: issue 0491 churn (build-script
output stamps did not move between the cold and warm runs) and issue 0648's
cargo package-cache lock (`~/.cargo/.package-cache` was absent from
`/proc/locks` during the build; the flocks held were each leaf's own
`.cargo-lock`, one holder per dir).

Filed as **issue 0805** with the fix and its constraint — the shared-target-dir
remedy sits on exactly the ground 0616 and 0500 mark dangerous, and the
distinction is that those forbid one target dir serving two workspace ROOTS
while these 21 invocations all name the same root. Also recorded there: a shared
directory fixes the COLD duplication and NOT the 494 s warm floor, which needs
fewer invocations rather than a shared destination.

### This issue is now fully answered

| priority | outcome |
| --- | --- |
| 1 — the gate phase | FIXED: one gate walking build output, 231.83 s -> 0.09 s; phase 124/124 in 10.8 s |
| 2 — a pooled launcher | CLOSED: it exists (`NROS_BUILD_POOL=1`) and the launcher was never the lever — 92.6% of the build has zero runnable processes of its own |
| 3 — the threadx tail | ANSWERED and handed to issue 0805; not platform-specific |

Nothing here remains actionable under this id. The live work is 0805.

## Resolved (2026-08-26): the premise was wrong, and measuring it was the value

This issue opened on "the fixture build statically partitions 32 cores into 4x8,
so 45% of its wall clock runs at a 25% CPU ceiling". That ceiling is real, and it
is not why the build is slow.

Lineage-scoped measurement — the build's OWN processes, 1327 samples — found
**mean 0.1 runnable, median 0, and 92.6% of samples with nothing on CPU at all**.
Permission to run was never the scarce resource, so no launcher change could have
helped. The pooled launcher built for this issue works and is kept opt-in
(`NROS_BUILD_POOL=1`); the gate fan-out built for it was REVERTED after it turned
out to defeat cargo's shared target directory.

Full accounting, including six retractions and the four durable lessons, is in
[phase-371](../roadmap/archived/phase-371-build-cpu-utilization.md)'s CLOSING SUMMARY.

What came out of it and stays: the `nros_grep_q` error/non-match class and its
ratchet gate, issue 0721's traversal fixes (>300 s -> 3 s on one gate), two
lineage-scoped samplers, and several real bugs surfaced by running gates
concurrently.

Closing rather than continuing because the last five measurements each retired a
proposed optimisation rather than enabling one. The remaining work — 0721's
unconverted walk sites, nightly's unexamined cost profile, a cold-tree occupancy
measurement — is ordinary and does not need a campaign.

## Reconciling two concurrent sessions (2026-08-26)

This file was written by two sessions at once, and they reached the same central
conclusion independently — the launcher was never the lever, because the build's
own processes are ~0.1 runnable and 92.6% of samples have nothing on CPU. Where
they appear to disagree, both are right and the difference is cache state:

**The gate fan-out.** The CLOSING SUMMARY reverts it (serial 84 s vs fan-out
>516 s) because the fan-out invokes `just <gate>` per gate, which defeats
cargo's shared target dir, and the 7 s figure was measured on a target dir a
preceding SERIAL run had warmed. That is correct and the revert stands. Measured
here afterwards on a tree whose cargo gates were warm from a session of repeated
runs: serial 75 s, fan-out 12 s. The serial numbers agree (75 s vs 84 s); the
fan-out number is exactly the confounder the summary names. So no fan-out figure
in the priority-1 section above should be read as a general result — the
`check-fast-parallel` wall-clock there is warm-cargo-dependent.

**What is NOT affected by that.** The `check-deploy-board-resolves` measurement
is of the gate itself, run directly, not through the phase: 231.83 s -> 0.09 s,
same verdict. It matters MORE under a serial `check-fast`, which pays it in
full, and the fix removes the cache dependence rather than benefiting from it —
an index read is one file whatever the page cache holds.

**One correction to the CLOSING SUMMARY's remaining-work list.** It names
"0721's unconverted walk sites". There are none: the walk class has 9 live
exemptions and every one is legitimately outside the git index — build output,
UNTRACKED leaf `target/` dirs the index cannot see by definition, `--self-test`
temp trees, the Zephyr submodule, an installed ROS include tree. The figure "86"
that circulated for these is this issue's `grep -q` error-conflation sites,
which have nothing to do with traversal I/O. Counted, not estimated.
