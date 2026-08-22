# Phase 375 — The board tier is a promise with an owner, and onboarding is the cost

**Status (2026-08-22). PROPOSED — W0 landed, W1–W5 not started.** Opened from the
question "more and more boards appear; it bloats — balance the platforms per
tier". The measurement says the tiers are NOT what bloats, so the waves below
target owners and onboarding instead.

**Implements:** [RFC-0064](../design/0064-board-support-organization.md)
revision 4 (2026-08-22).
**Related:** [phase-320](archived/phase-320-board-support-tiers.md) (W3.b opened
the maintainer field and left it unenforced — this closes it),
[phase-346](archived/phase-346-out-of-tree-board-seam.md) (the seam W5 needs,
COMPLETE), [phase-370](phase-370-freertos-posix-board-cyclone.md) and
[phase-372](phase-372-s32z270-freertos-board-bundle.md) (the two boards whose
onboarding cost is the evidence).

## The measurement

| | |
| --- | --- |
| `matrix::CELLS` | 191 — 181 Runtime, 5 BuildOnly, 5 CarveOut |
| `fixtures.toml` rows | 422, of which `linux` is 195 (46 %) |
| lane coordinates | tier 1 **10**, tier 2 **14**, nightly **37**, tier 3 **51** |
| board registry | 5 tier-1, 6 tier-2, 2 tier-3, 9 infra — **0 with a maintainer** |
| tier-2 fixture BUILD | ~33 min (one observation, this host) |
| tier-2 RUN | 128 s for 1673 tests |

A new platform costs **+1 coordinate in tier 1, tier 2 and nightly** (1-wise and
pairwise both absorb a new axis value cheaply) and +2 in tier 3. The run is
nearly free; the BUILD dominates, and it scales with fixture ROWS, where the
mass is `linux` at 46 % — not the new boards, which carry 2–3 rows each.

What actually cost: **`s32z270` landed red on FIVE gates** (weak symbols, board
tiers, leaf lock, provider announcements, matrix orphan) and `freertos-posix` on
two plus a lane-table cascade. Each gate was correct. The cost was discovering
them serially, on main, by whoever noticed.

## W0 — Model the S32Z270 row — **LANDED 2026-08-22**

- [x] `Tier::BuildOnly` cell for `(FreertosMps2, Cpp, Cyclonedds, EntryPubsub,
      Workspace)`, string naming what unlocks it (a hardware or simulator
      witness). Clears `fixture_rows_all_modeled_by_matrix`, the last real tier-1
      failure.
- [x] `check-board-tiers` taught the BORROWED-token case: a tier-3 row whose
      platform is claimed at tier 1/2 by a DIFFERENT crate is not proven by cells
      that are the other board's witness. Inferred from existing rows, not a new
      key; the exemption prints. Negative control: with the owner also at tier 3,
      the rule fires again.

**A correction this wave produced.** An earlier reading of the matrix reported
"181 cells, all Runtime" and concluded the table had no vocabulary for a
build-only board — so the first proposal here was to ADD a `Build` kind. Wrong:
the regex matched only bare-identifier tiers and silently dropped
`BuildOnly("reason")`. The vocabulary has existed since revision 3. Re-measure
before proposing a mechanism, and prefer a count the code produces over one a
regex infers.

## W1 — Maintainers become the tier gate

- [ ] Adopt Rust's counts in `check-board-tiers`: tier 1 >=3, tier 2 >=2,
      tier 3 >=1 named maintainer. Currently ALL 22 rows carry `maintainers = []`
      and the gate prints "not enforced yet (phase-320 W3.b)".
- [ ] Land it as a RATCHET, not a cliff: existing rows are grandfathered by an
      explicit baseline that can only shrink, so the rule binds new boards
      immediately and existing ones as owners are found. A gate that fails 22
      rows on the day it lands gets bypassed, which is worse than one that binds.
- [ ] Demotion is automatic and printed, never silent.

**Acceptance:** a new board row without a maintainer cannot be tier 1 or 2, and
the baseline count only decreases.

## W2 — `just board-new` — onboarding is a scaffold, not a scavenger hunt

- [ ] Emit, in one command: the `nros-board.toml` descriptor, the `package.xml`
      `<nano_ros_provides>` export mirroring its `names`, the
      `board-support.toml` row, a weak-symbol allowlist stub, and the leaf lock.
- [ ] Those are exactly the five gates `s32z270` tripped. The scaffold's
      acceptance is that a board created by it passes `just check-fast` on the
      first run.

**Acceptance:** a scaffolded board is green on `check-fast` before its first
commit; the five gates stay unchanged (this does not weaken them).

## W3 — A board below tier 2 must not redden a shared lane

- [ ] State the rule in RFC-0064 terms and decide the mechanism: either
      onboarding-complete-at-merge (W2 makes this cheap) or excluding tier-3
      boards from the gates that block `check-fast`.
- [ ] W2 first — the second option trades coverage for isolation and should only
      be reached for if the scaffold proves insufficient.

**Acceptance:** adding a build-only board cannot make main red for people who do
not use it.

## W4 — Smoke floor, witness-gated ceiling

- [ ] Every supported platform earns exactly ONE Runtime cell (boots, delivers a
      message) to sit in tier 2; full cells in nightly require a witness.
- [ ] Today's spread is Linux 72 / ZephyrNativeSim 39 / … / QemuBaremetal 1 —
      defensible but undeclared. This makes it intentional and gives a new board
      a known, bounded entry cost.
- [ ] Do NOT replace the computed 1-wise/pairwise cover with declared per-test
      platform lists. Zephyr's equivalent (`integration_platforms`) is reported
      by their own issue #57595 as trial-and-error and untested; this tree's
      cover is gated by `documented_lane_table_is_live` and cannot drift.

**Acceptance:** the per-platform cell count follows a stated rule, and a new
platform's tier-2 entry cost is one cell.

## W5 — Decide S32Z270's home

- [ ] It exists for `autoware-safety-island`. Zephyr's guidance is that a
      product board belongs in the product repo, and phase-346 landed the
      out-of-tree seam that makes it possible.
- [ ] The trade is explicit: out-of-tree costs in-tree evidence that the board
      still links; in-tree costs a maintainer under W1 and the onboarding under
      W2. Either is fine; defaulting without deciding is not.
- [ ] If it stays, it needs a named maintainer and it stays BuildOnly until a
      witness exists.

**Acceptance:** a recorded decision, not a default.

## Risks

**The ratchet is the whole of W1.** A maintainer rule that fails every existing
row on landing day gets bypassed, and a bypassed gate is worse than none — the
same argument CLAUDE.md makes about a tier nobody can afford being followed
selectively.

**W4 changes what a tier PROMISES**, so it is the one wave that needs agreement
before implementation rather than after. The smoke floor is cheap; the nightly
ceiling removes coverage that exists today.
