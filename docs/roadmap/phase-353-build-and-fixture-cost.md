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

* **#562** — the issue says `atomic_write_bytes` "never compared content", so
  every sync restamped mtimes and charged a cmake reconfigure. The code now
  documents itself as WRITE-IF-CHANGED (`packages/cli/nros-cli-core/src/atomic_file.rs`,
  header comment: "byte-identical content is not rewritten, so an unchanged file
  keeps its mtime and costs no downstream reconfigure"). **Determine whether the
  fix landed and the status was simply never flipped**, or whether residue
  remains. Note the issue's own "Separately" section hands the zephyr no-op
  finding to #509, so closing #562 does not close that.

  Observed independently on 2026-08-14 and 2026-08-15: a `nros sync` during
  tier 1 leaves `examples/threadx-linux/rust/talker/.cargo/config.toml` modified
  with a PURE WHITESPACE diff (`["../..` becoming `[ "../..`). That is a
  *content* difference, so write-if-changed cannot suppress it — sync is not a
  fixed point on its own output. Whether that belongs to #562 or is a separate
  defect is W1's to decide; it currently shows up as spurious unstaged churn
  that blocks `git pull --rebase` until discarded.

* **#446** — "the same crate is compiled ~21× across leaf target dirs". Filed
  before phase-340's coordinate-keyed group dirs and phase-343 W1's shared probe
  dir. Re-run the census (`nros-core` rlibs across leaf `target*/…/deps`) and
  restate the factor. The issue's real question — *what actually makes those
  builds incompatible* — is unanswered either way, and is the part worth
  keeping.

**Acceptance.** Each of #562 and #446 carries a dated re-measurement on this
tree, and is either resolved with evidence or restated with a current number.
No implementation lands under this phase before that.

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
