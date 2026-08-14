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

  1. **The whitespace churn is NOT #562 and is still unexplained.**
     `examples/threadx-linux/rust/talker/.cargo/config.toml` came back modified
     after tier-1 runs on 2026-08-14 and 2026-08-15 with a pure-whitespace diff
     (`["../..` becoming `[ "../..`). That is a CONTENT difference, which
     write-if-changed cannot suppress by design. It does **not** reproduce from
     a direct `nros sync` of that leaf, so the writer is reached only by the
     full lane — most likely `model_ingest`, which #562 named as a leaf
     `.cargo/config.toml` writer. Until it is found, it shows up as unstaged
     churn that blocks `git pull --rebase` until discarded by hand, and CLAUDE.md
     is explicit that a blanket `git add -u` must never scoop it up.
  2. **A measurement lesson worth keeping.** The first restamp measurement used
     `comm` on `find -printf` output and reported 31 890 files. `comm` warned
     `file 1 is not in sorted order` and produced meaningless output; the real
     answer was 6. Use an explicit stat-map compare here, not `comm`.

* **#446** — "the same crate is compiled ~21× across leaf target dirs". Filed
  before phase-340's coordinate-keyed group dirs and phase-343 W1's shared probe
  dir. Re-run the census (`nros-core` rlibs across leaf `target*/…/deps`) and
  restate the factor. The issue's real question — *what actually makes those
  builds incompatible* — is unanswered either way, and is the part worth
  keeping.

**Acceptance.** Each of #562 and #446 carries a dated re-measurement on this
tree, and is either resolved with evidence or restated with a current number.
No implementation lands under this phase before that.

**Status: #562 done. #446 outstanding.** The whitespace-churn hunt above is the
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

---

## Deliberately not doing

* **No shared host target-dir.** Phase 343 established by measurement that cargo
  has no host-scoped target dir — not a flag, not a config key, not behind `-Z`
  — and that the host graph carries 0 of 91 crates with a single `-C metadata`
  identity. Any proposal here that assumes otherwise is already refuted.
* **No new caching layer.** Three of these four issues are about work that
  should not happen, not about caching work that does.
