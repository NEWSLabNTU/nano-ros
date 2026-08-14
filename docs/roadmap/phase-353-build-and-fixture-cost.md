# Phase 353 — Build and fixture cost: what the lanes actually pay

**Status (2026-08-15). PLANNING — nothing implemented. Opened to give four
standing cost issues one owner instead of four.** Each was filed from a real
measurement; none has a phase. Two of them (#446, #562) are follow-ons to work
that already landed, so the first task in each case is to re-measure rather than
to build.

**Owns:** [issue 0446](../issues/0446-build-artifact-reuse-factors.md),
[issue 0509](../issues/0509-zephyr-lane-per-leaf-overhead.md),
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

Measured: 68 leaves in 40 minutes on a 32-core host, 1254 ninja edges replayed
per run, almost none of it compiling. #562's own text hands its "the zephyr
no-op lane is not a no-op" finding here: every one of seven consecutive runs
replayed a full Zephyr static-library link set plus a 129-crate `nros-c`
rebuild.

The shape to investigate is "skip per-leaf prep whose inputs are unchanged",
which is #509's line. Not started; no design committed.

**Acceptance.** A stated cause for the replayed edges (not a guess), and either
a fix with a before/after wall-clock on the same lane, or a written finding that
the cost is irreducible with the reason.

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

## W4 — Collapse the probe dir's over-keying (#446; opened by W1)

**Status (2026-08-15). SAFETY NET + DIAGNOSIS LANDED; the narrowing itself is
NOT done and now has evidence behind it.**

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

### What the narrowing should be, and what it must not be

**Not** "exclude paths". `NROS_BOARD_TOML` is a path whose CONTENT carries
sizing knobs, so a blanket path rule would reintroduce 0528 by a new route.

The shape that inverts the risk correctly is an explicit **denylist with a
stated reason per entry** — environment plumbing that provably cannot reach a
probed size (`NROS_REPO_DIR`, `NROS_*_INCLUDE`, `NROS_*_SRC`,
`NROS_ZEPHYR_CCACHE_*`, `NROS_ZEPHYR_BUILD_ROOT`, `NROS_CARGO_FLAGS`, the
`NROS_ESP_IDF_*` pair). Everything unlisted keeps today's behaviour, so an
unknown-but-set knob still keys the probe — which is the property
`knob_identity()`'s comment defends and which must survive.

**Acceptance.** A measured reduction in probe sub-key count and disk, with the
0528 reproduction passing, plus a `nros-probe-key-inputs.txt` census showing
which knobs still split. Each denylist entry carries its reason in the source.


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
