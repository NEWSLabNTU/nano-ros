# Phase 353 — Build and fixture cost: what the lanes actually pay

**Status (2026-08-15). W1, W2 and W4 COMPLETE; W3 BLOCKED.** Opened to give four standing cost issues one owner instead of four.

* **W1** — #562 verified and RESOLVED (its fix had landed, status never
  flipped); #446 re-measured and restated.
* **W4** — probe key narrowed: **25 -> 8** sub-keys, **7.2 G -> 2.2 G** on the
  same lane, and a second run now adds ZERO new keys.
* **W2** — all three directions answered.
  * **(1) skip unchanged prep — DONE.** The zephyr no-op lane now does nothing:
    **1668 log lines -> 73, 1244 ninja edges -> 0, 129 `Compiling` -> 0.**
  * **(2) storage — REFUTED** on this host: iowait ~0 on both the HDD and the
    NVMe build root, so there is no stall to recover.
  * **(3) fewer COLD leaves — DONE.** The dep-info staleness arm compares
    CONTENT, not mtime, so a `git pull --rebase` / `stash` / branch switch no
    longer turns every prebuilt fixture cold. The mechanism already existed and
    served the zephyr arm alone (issue 0442's shape); it now lives in
    `fixtures::staleness` and both arms share it.

  #509 is now CLOSED (upstream `07e4dce92`, 2026-08-15): every direction was
  either fixed or refuted, and the one survivor — fewer COLD leaves — continues
  as **#604**, filed as a MEASUREMENT (how many leaves a pull genuinely
  invalidates versus merely re-stamps) rather than as a defect. #604 already
  credits this phase's W2 for the content-aware staleness half.
* **W3** — #200 is blocked on a big-disk CI runner and is not actionable
  locally.

**Remaining:** only W3 — #200's timing campaign, which needs a big-disk CI
runner. Everything actionable on a dev host is done.

Carried OUT of this phase rather than left implied: **#604** continues #509's
one surviving direction (fewer cold leaves) as a measurement — #509 itself
closed upstream on 2026-08-15; **#446** stays open with a current number and
W4's census as its next lead; and **#601** (a ROS `idlc` that is found but
cannot load) blocks a cold cyclonedds build wherever ROS is installed but not
sourced into the build.

**Owns:** [issue 0446](../issues/0446-build-artifact-reuse-factors.md),
[issue 0509](../issues/archived/0509-zephyr-lane-per-leaf-overhead.md) (now closed; continues as [issue 0604](../issues/0604-cold-leaves-mtime-treadmill.md)),
[issue 0562](../issues/0562-sync-rewrites-unchanged-files.md),
[issue 0200](../issues/0200-fixture-build-timing-campaign-needs-ci-runner.md).

**Related:** [phase-340](archived/phase-340-build-artifact-reuse.md) (fixture
target-dir grouping), [phase-343](phase-343-host-build-graph-duplication.md)
(host build-dep graph; W1 recovered 63.1 GiB),
[phase-342](phase-342-test-runtime-reduction.md) (test runtime, COMPLETE).

## Why these four are one phase

They are all "the build pays for something it does not use", and they interact:
a fix in one changes the measurement in the next. Phase 340 grouped fixture
target dirs by coordinate; phase 343 shared the sizes-probe dir. Both moved the
numbers these issues were filed against, which is exactly why the first work
item is a re-measurement and not a change.

They are NOT a single mechanism, and this phase does not claim they will share
one fix.

---

## W1 — Re-measure before building anything

Two of the four may already be closed or substantially moved by work that landed
after they were filed. Establish that first; a phase that starts by implementing
against a stale number repeats what phase-343 W2 had to undo.

* **#562** — **DONE (2026-08-15). RESOLVED and archived.** The class fix was in
  the tree and the status had simply never been flipped, which is what this work
  item existed to determine. Both headline measurements re-verified on no-op
  syncs: `examples/native/rust/talker` 2 → **0** restamped files,
  `examples/workspaces/features` 27 → **6** — and all six are cmake's own
  outputs under `build/nros-metadata/metadata-probe-cmake/build/`, so
  **sync-owned restamps are 0**. See
  [archived/0562](../issues/archived/0562-sync-rewrites-unchanged-files.md).

  Two things carried forward from it:

  1. **The whitespace churn is NOT #562, and no longer reproduces.**
     `examples/threadx-linux/rust/talker/.cargo/config.toml` came back modified
     after tier-1 runs on 2026-08-14 and 2026-08-15 with a pure-whitespace diff
     (`["../..` becoming `[ "../..`). That is a CONTENT difference, which
     write-if-changed cannot suppress by design, so it was never #562's.

     Re-checked on 2026-08-15 after #562's verification: **clean** after a
     direct `nros sync` of that leaf, and **clean** after a full green tier 1
     (`git status --porcelain -- examples/` empty both times). So something
     between those runs fixed it, or it needed a state this tree no longer has.

     Left recorded rather than declared fixed, because two observations and two
     clean checks is not a diagnosis and nobody has identified the writer.
     #562 named `model_ingest` as a leaf `.cargo/config.toml` writer, which is
     where to look first if it returns. Symptom to watch for: unstaged churn
     that blocks `git pull --rebase` until discarded — and CLAUDE.md is explicit
     that a blanket `git add -u` must never scoop it up.
  2. **A measurement lesson worth keeping.** The first restamp measurement used
     `comm` on `find -printf` output and reported 31 890 files. `comm` warned
     `file 1 is not in sorted order` and produced meaningless output; the real
     answer was 6. Use an explicit stat-map compare here, not `comm`.

* **#446** — **DONE (2026-08-15). Re-measured; the issue stays OPEN with a
  current number and a sharper target.** Original scope re-run
  (`libnros_core-*.rlib` under `*/nros-relwithdebinfo/deps/`):

  | | 2026-08-06 | 2026-08-15 |
  | --- | --- | --- |
  | rlibs | 106 | **707** |
  | target dirs | 60 | **385** |
  | `-C metadata` identities | 5 | **49** |
  | factor | 21.2x | **14.4x** |

  The ratio improved and the absolute waste grew, because the tree grew. The
  finding is that **the duplication MOVED**: the worst population is now
  `build/sizes-probe` at **25.8x** (155 rlibs, 6 identities, **37 GB**) — the
  directory phase-343 W1 created to remove duplication. Under its single
  `rustc-1.97.1` key sit 110 sub-key directories holding 18 identities, the top
  two being one compilation done 70 and 69 times.

  Cause: `probe_key` hashes the REQUEST (target, features, every set `NROS_*`
  knob plus Zephyr `$DOTCONFIG` knobs) while `-C metadata` hashes what
  determines the ARTIFACT.

  **Corrected 2026-08-15 by W4's evidence.** This entry first said the split
  came from sizing knobs moving `ExecutorInlineStorage`. That was reasoning, not
  measurement, and it is wrong about the dominant population — see W4.

  **Direction 3 of the issue is already DONE** — phase-340 W3 normalised the
  `--target` split (gate `check-cargo-target-spelling`). Direction 1 now has a
  better target than when it was written; see W4.

**Acceptance.** Each of #562 and #446 carries a dated re-measurement on this
tree, and is either resolved with evidence or restated with a current number.
No implementation lands under this phase before that.

**Status: W1 COMPLETE.** The whitespace-churn hunt above is the
one piece of #562 that survived it, and it is a correctness annoyance rather
than a cost item — fix it when the writer is identified, not by widening
write-if-changed, which cannot see it.

## W2 — The Zephyr lane's per-leaf overhead (#509)

**Status (2026-08-16). COMPLETE — all three directions answered.**

* **(1) skip unchanged prep — DONE.** `west-fixtures.sh`'s unconditional wipe
  WAS the 1244-edge replay. A no-op lane goes 1668 log lines to 73, 1244 ninja
  edges to 0, 129 `Compiling` to 0.
* **(2) storage — REFUTED.** iowait ~0 on both the HDD and the NVMe build root,
  so there is no stall to recover.
* **(3) fewer COLD leaves — DONE.** The dep-info staleness arm compares CONTENT,
  not mtime.

#509 itself CLOSED upstream (`07e4dce92`, 2026-08-15); its one surviving
direction continues as **#604**, filed as a measurement rather than a defect.
The sections below are the working notes, in the order they were written — the
"remain" language in them predates (2) and (3).

### Landed: `west-fixtures.sh` had no warm state, by construction

It ran `rm -rf "$bld"` unconditionally on every invocation, so every run was a
cold `west build`. The freshness answer was already being computed in the same
loop and discarded — `write_compile_check_inputsig` hashes the manifest record,
the row's source tree and the nros CLI's codegen fingerprint, and writes it
after a successful build, but nothing read it back.

Reading it is the whole change, and using that signature satisfies issue 0196:
the build-side probe now watches exactly what the test-side probe recomputes.
Reuse needs an identical signature AND the declared `output` AND the
`.compile-ok` stamp; anything else falls through to the old wipe, so the failure
mode is the old cost, never a stale fixture.

| run | result | elapsed |
| --- | --- | --- |
| cold | `1/4 ok (0 reused, 1 built)` | 17.8 s |
| warm | `1/4 ok (1 reused, 0 built)` | 9.7 s |

### It IS the 1244 edges — and my first reading of that was wrong

This section first said the 1244-edge replay was NOT west-fixtures, on the
strength of a standalone run of the script that emitted 0 `Compiling` lines.
That run lacked the lane's environment, so only 1 of its 4 rows built. The
generalisation was the error.

Measured in the lane, narrowed to `zephyr,rust,zenoh` (7 leaves) so it completes
on this host, two runs with nothing changed between them:

| run | log lines | `Compiling` | ninja edges | west fixtures |
| --- | --- | --- | --- | --- |
| 1 | 1668 | **129** | **1244** | `4/4 ok (0 reused, 4 built)` |
| 2 | 73 | **0** | **0** | `4/4 ok (4 reused, 0 built)` |

Run 1 reproduces issue 0509's numbers exactly, and all of them fall between the
first `== west-fixture:` marker (line 64) and the step's summary (line 1667).
The reuse removes them: **a no-op lane now does nothing, in 73 lines instead of
1668.** Direction (1) of the issue's revised list — "skip per-leaf prep whose
inputs are unchanged" — is delivered.

### Direction (2), storage — REFUTED on this host

The issue promoted storage to a first-class direction from a 76 % idle / 18 %
iowait sample on "a rotational 5.5 TB `/dev/sda`" with 50 GB of page cache
against 61 GB RAM. Both halves of that premise have moved:

| | |
| --- | --- |
| RAM | **125 GB** (117 available), not 61 |
| `/tmp` — the Zephyr build root | `nvme0n1`, **SSD** |
| `/home` — the repo, so `build/` | `sdb`, rotational |

A/B with `NROS_BUILD_ROOT` as the ONLY difference, both sides cold, identical
work (the 1244-edge west-fixtures build through the lane), iowait taken from
`/proc/stat` deltas:

| build root | elapsed | iowait | busy | edges |
| --- | --- | --- | --- | --- |
| HDD `/home` | 46.9 s | **0.25 %** | 21.4 % | 1244 |
| NVMe `/tmp` | 43.1 s | **0.03 %** | 14.8 % | 1244 |

**iowait is ~0 on both sides**, so there is no stall to recover and relocating
the build root buys nothing measurable; the 8 % elapsed gap sits far inside the
14x spread the issue documented for identical work. The doubled RAM is the
likely reason — 117 GB of page cache now covers what 50 GB did not.

Scope, stated precisely: this measures the 1244-edge build, not the 58-leaf
lane, and the original sample was taken mid-`lane=all` with eight families
competing at half the memory. A full-lane number is unavailable here because the
cyclonedds zephyr leaves abort at configure on `ModuleNotFoundError: No module
named 'rosidl_adapter'`.

### What is still open

Direction (3), **fewer COLD leaves**, is now the dominant remaining item and is
not a hardware problem: a cold leaf costs ~28 s (512 s for 18, measured
2026-08-13), and the mtime treadmill (#0466) is what makes leaves cold after
every pull, rebase or `git stash`. That is caching correctness, and it is
actionable on this host.

Blocking a full-lane measurement on this host: the cyclonedds zephyr leaves fail
at configure with `ModuleNotFoundError: No module named 'rosidl_adapter'`, so
`just zephyr build-fixtures` aborts before finishing the 58-leaf set.

**Acceptance (unchanged).** A stated cause for the replayed edges, and either a
fix with a before/after EDGE COUNT — not wall-clock, which issue 0509 itself
showed is unusable on this host at a 14x spread — or a written finding that the
cost is irreducible.

## W3 — The timing campaign (#200), and why it stays parked

Phase 226's three validation measurements need a clean timed build that consumed
~52 GiB and 25 minutes on the maintainer host without completing. This is
blocked on a big-disk CI runner and is **not** actionable locally.

It is listed here so the blocker is visible in one place rather than
rediscovered. **Do not** attempt it on a dev host; the disk cost is the reason
it was parked.

Note W1's re-measurements may reduce what this campaign needs to cover — phase
340 and 343 both cut per-leaf disk substantially after #200 was written.

**Acceptance.** Either a runner exists and the campaign runs, or #200 is
restated against post-340/343 disk figures so a future runner is sized correctly.

**Status (2026-08-16): the RESTATEMENT half is DONE.** Phase-340 completed
2026-08-12, which cleared this issue's own "run after item 5" precondition, so
the arithmetic was re-run. Recorded in #200:

* the checkout measures **994 GB**, and that number is useless for sizing — it
  is an accumulated tree, the trap issue 0499 and the artifact-identity gate
  both name;
* **120 GB of it is dead**: 65 plain per-leaf `examples/**/target/` dirs,
  mtimes 2026-08-01…06, replaced by phase-340 P2's coordinate-keyed group dirs
  and rebuilt by nothing;
* the duplication factor the ≥200 GiB requirement rested on moved **21.2x →
  14.4x** (#446), and `build/sizes-probe` went from growing-per-run (37 G) to
  bounded (**8 sub-keys / 2.2 G** on a clean native lane, phase-353 W4).

**The runner is still required**, and for a sharper reason than disk: issue 0509
established that wall-clock on this host is not a usable instrument at all — a
14x spread on provably identical work. The campaign needs a machine where
timings mean something.

So W3's local half is complete; the measuring half stays blocked, as designed.

## W4 — Collapse the probe dir's over-keying (#446; opened by W1)

**Status (2026-08-15). COMPLETE — narrowing landed and measured: 25 -> 8 probe
sub-keys, 7.2 G -> 2.2 G on the same lane, and a second run now adds ZERO new
keys. The first diagnosis was wrong and the A/B caught it; both are recorded.**

W1's measurement made this the phase's largest contained prize: **110 probe
sub-directories holding 18 distinct `nros-core` identities, 37 GB**, where cargo
itself says most of those artifacts are interchangeable.

### The constraint, first

**Issue 0528's invariant must hold.** A knob that CAN change a probed size must
still split the key, or the failure returns as order-dependent corruption — a
4-CBS leaf poisoning a 16-CBS one into `EXECUTOR_OPAQUE_U64S too small`, which
survives a clean rebuild of the failing leaf because the poisoned directory is
the shared one.

**Landed: that reproduction is now a test**, not a memory
(`nros-sizes-build`): `zephyr_dotconfig_sizing_knob_splits_the_probe_key`
(two `$DOTCONFIG`s differing only in `CONFIG_NROS_EXECUTOR_MAX_CBS`),
`env_sizing_knob_splits_the_probe_key` (the env route, issue 0460), and
`identical_inputs_share_a_probe_key` — the control, without which a key that
split on everything would pass the first two and be useless.

Verified by sabotage: deleting the knob half of `probe_key` fails both
reproductions with
`a 4-CBS and a 16-CBS Zephyr leaf landed on the SAME probe key — issue 0528 is
back`, while the control still passes. Restored, 5/5 green.

### The diagnosis, which needed the key to be attributable

The key is an opaque FNV hash and the directories recorded nothing, so the 110
dirs could not be attributed to target, features or knobs without re-deriving
every consumer's build. **A key that cannot be attributed cannot be narrowed**,
so each probe dir now writes `nros-probe-key-inputs.txt` when it is created —
write-once, diagnostic only, never an input to the key it describes.

The first such record answered the question immediately. 100 of the 110 sub-keys
share ONE target triple, and the knob list looks like this:

```
target   x86_64-unknown-linux-gnu
features alloc,default,rmw-cffi,std
knob     NROS_CARGO_FLAGS=--locked
knob     NROS_C_INCLUDE=/home/aeon/repos/nano-ros/packages/api/nros-c/include
knob     NROS_REPO_DIR=/home/aeon/repos/nano-ros
knob     NROS_ZEPHYR_BUILD_ROOT=/tmp/nros-build-aeon/zephyr
knob     NROS_ZEPHYR_CCACHE_DIR=/tmp/nros-build-aeon/ccache
   … 11 more, nearly all absolute paths
```

**The dominant splitter is absolute PATHS**, set by `activate.sh` — not sizing
knobs. `knob_identity()` sweeps every `NROS_*` in the environment, and this
environment is mostly path plumbing. A path cannot change a compiled size, so
these split the directory while cargo says the artifact is identical.

This is the class CLAUDE.md already names as issue 0491: *never fingerprint on a
PATH variable — cargo compares an env value as TEXT, and one directory has three
spellings here.* The same defect, one layer over, in a key rather than a
`rerun-if-env-changed`.

### The first diagnosis was wrong, and the A/B is what caught it

The provenance record showed the knob list was dominated by absolute paths
(`NROS_REPO_DIR`, `NROS_C_INCLUDE`, `NROS_ZEPHYR_BUILD_ROOT`, …), and this phase
concluded those were the splitter. A denylist of exactly those names was written
and A/B'd on the same lane with the probe dir wiped both sides:

```
before: 25 sub-keys, 7.2 G
after:  25 sub-keys, 7.2 G      <- no change at all
```

**Those variables are CONSTANT within a run, and a constant input cannot split
anything.** The diagnosis had been inferred from what the knob list *contained*
rather than from what actually *varied* — the same error twice in one phase.

### What actually splits it, from the census the provenance made possible

Of the 25 keys, all shared ONE target triple and **19 shared the same feature
set**. 35 knobs varied inside that group of 19, and not one was a sizing knob:

```text
NROS_BUILD_LOG_DIR    .../logs/20260815-111859-1157807-9133   <- timestamp + pid
NROS_WS_RECORDS_FILE  .../ws-linux-20260815-112230-1214903-group-10.records
NROS_FIXTURE_ID       11 values
NROS_KIND_*           ~20 per-kind marker strings
NROS_BUILD_JOBS       24 vs 6
```

The timestamped ones are the mechanism behind the 37 GB: they differ on **every
run**, so every fixture build minted probe keys that could never be reused. That
is why one lane creates 25 directories while the tree had accumulated 110.

### Landed

`KNOBS_THAT_CANNOT_CHANGE_A_SIZE` (exact names) plus
`KNOB_PREFIXES_THAT_CANNOT_CHANGE_A_SIZE` (`NROS_KIND_`, `NROS_BUILD_` — one
producer, uniform by construction). Anything unlisted keys the probe exactly as
before, so forgetting a name costs a wasted directory, never corruption.

Measured on the same lane, probe dir wiped both sides:

| | before | after |
| --- | --- | --- |
| probe sub-keys | 25 | **8** |
| disk | 7.2 G | **2.2 G** |

And the growth is stopped, which is the defect rather than the symptom: a
SECOND run of the same lane creates **zero** new keys (8 → 8, 2.2 G).

Tests, all in `nros-sizes-build`:

* `zephyr_dotconfig_sizing_knob_splits_the_probe_key` / `env_sizing_knob_…` —
  issue 0528's reproduction, still passing WITH the narrowing in place
* `identical_inputs_share_a_probe_key` — the control
* `an_unlisted_knob_still_splits_the_probe_key` — 0528's default survives for
  unknown knobs, including a real sizing knob
* `run_scoped_orchestration_does_not_split_the_probe_key` — the census's actual
  offenders, by name and value
* `every_excluded_knob_carries_an_argument` — every entry has a reason, and
  `NROS_BOARD_TOML` / `NROS_PLATFORMS_DIR` / `NROS_MODEL_DIR` / `NROS_HOME` can
  never be excluded, directly or by prefix, because each names a file whose
  CONTENT carries sizing knobs

**Not claimed:** a wall-clock improvement. Issue 0562 established that this
host's lane timing is set by page-cache state (a 14x spread on provably
identical work), so no timing claim is supportable from it. The key count and
the disk are deterministic and are what changed.

---

## Deliberately not doing

* **No shared host target-dir.** Phase 343 established by measurement that cargo
  has no host-scoped target dir — not a flag, not a config key, not behind `-Z`
  — and that the host graph carries 0 of 91 crates with a single `-C metadata`
  identity. Any proposal here that assumes otherwise is already refuted.
* **No new caching layer.** Three of these four issues are about work that
  should not happen, not about caching work that does.
* **No loosening of the probe key without a per-knob argument.** W4 exists
  because the key is over-broad, but that breadth IS issue 0528's fix; a blanket
  narrowing trades a disk cost for a corruption class.
