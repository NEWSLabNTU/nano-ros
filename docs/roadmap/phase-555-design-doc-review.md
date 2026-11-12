# Phase 555 -- Design doc review: organize the RFCs and resolve what conflicts

**Status (2026-11-13). OPEN: W0 done (the inventory and its candidate pairs);
W1 started (`area:` drafted on every RFC); W2 to W4 not started.**

User request, 2026-11-12, verbatim: "Have a design doc review campaign. I
would make them organized. Some design docs may be old and could conflict
with new designs."

This phase is the campaign. Per the standing rule that implementation work is
tracked by a phase doc, every step is a work item here with its status.
Nothing in it changes a design without a ruling: a reviewer fixes what is
mechanically wrong (a dead path, a retired name, a broken link), and gathers
anything that would change a design into one decision brief per area, with a
recommendation, for the user to rule.

## Where the series stands (measured 2026-11-12)

From `scripts/docs/rfc-inventory.py` on main, after the `rfc-phase-tracking`
backfill (#3647):

- 126 RFCs in `docs/design/`: 71 Draft, 49 Stable, 2 Accepted, 2 Superseded,
  plus one `Draft (LIVE -- under active exploration)`. The one RFC with no
  frontmatter is 0059, which is Superseded.
- Only 2 are Superseded. Over this many redesigns that is implausible: most
  replacements were recorded as prose ("amends", "replaces") or not at all.
- `Accepted` and the `LIVE` qualifier are outside the
  Draft -> Stable -> Superseded vocabulary that `docs/design/README.md` defines.
- 38 have a `last-reviewed` older than 2026-07, and one has none.
- `docs/design/archived/` holds 15 files named by slug, not by number, so
  nothing above counts them.
- The README index is hand-written. It groups RFCs by series, not by subject,
  and its one-liners have grown into paragraphs.

## Work items

### W0 -- The inventory, generated

`scripts/docs/rfc-inventory.py` writes `build/rfc-inventory/inventory.md` and
`pairs.md`, generated and never committed. `inventory.md` holds one row per
RFC: id, title, status, since, last-reviewed, tracking phases, the RFCs it
cites and is cited by, and its declared `supersedes`, `superseded-by` and
`amends` links. `pairs.md` lists the CANDIDATE conflict pairs. A candidate
is an older RFC and a newer one with no supersede or amend link either way,
and either:

- **subject overlap**: shared vocabulary in the titles and in each RFC's own
  first paragraph, at a Jaccard similarity of 0.2 or more (0.08 or more when
  they also share a tracking phase). Paragraphs that several RFCs share,
  such as a rename note stamped across the series, and words in more than a
  tenth of the RFCs are left out, because they made unrelated RFCs look
  alike. The first cut, without that filter, ranked RFC-0015 and RFC-0027 at
  0.8 on a shared rename note; or
- **a citation without a link**: the newer RFC cites the older one, and
  neither says it amends or supersedes the other.

A candidate is a question for a reviewer, not a verdict: two RFCs on one
subject can be complementary.

The pairs are RANKED, because most citations are plain references, not
conflicts:

1. **Tier 1, subject overlap.**
2. **Tier 2, a citation that claims a change.** The newer RFC cites the older
   in a sentence that says it replaces, supersedes, retires, amends,
   contradicts or deprecates it. The sentence is printed as evidence.
3. **Tier 3, a plain citation.** Listed for completeness and not dispatched.

Measured: 126 RFCs give **352 candidate pairs: 10 in tier 1, 93 in tier 2
and 249 in tier 3**. Only tiers 1 and 2 (103 pairs) go to reviewers.

- Tier 1 includes 0024/0025 (two "Phase 212 workspace layout" RFCs),
  0007/0041 (service and action QoS), 0014/0099 (provisioning), 0095/0099
  and 0097/0099 (the store and release composition).
- Tier 2 includes declared amendments that never reached either RFC's
  frontmatter. For example, RFC-0109 says it "**Amends** RFC-0004", and
  neither RFC links the other.

- **Acceptance:** the generator is committed, and its two outputs and the
  counts above reproduce from a clean tree.
- **Status:** DONE (2026-11-12, the PR that adds this doc)

### W1 -- Organize: an area for every RFC, a generated index, one status vocabulary

- An `area:` frontmatter field with seven values:
  - `runtime-api`
  - `rmw-wire`
  - `platform-board-rtos`
  - `build-toolchain-codegen`
  - `workspace-system-model`, which covers launch and sizing
  - `test-ci`
  - `docs-ux-diagnostics`

  Every live RFC gets exactly one, proposed from the inventory and confirmed
  by its W2 reviewer.
- Status is normalized to Draft, Stable or Superseded. `Accepted` and the
  `LIVE` qualifier map onto it, each mapping stated in the commit that makes
  it.
- `docs/design/README.md`'s index is GENERATED from frontmatter: grouped by
  area, with status and title. The prose above the index stays authored.
- Archived RFCs gain `rfc:` numbers in their frontmatter, so the inventory
  sees them.
- Gates: one for the `area:` vocabulary, and one that the README index
  equals the generator's output.
- **Acceptance:** every live RFC has an `area:` and a status in the
  vocabulary; `just check fast` fails on a hand edit of the index or an
  unknown area.
- **Status:** OPEN (the `area:` drafts are in, for 125 RFCs, to be confirmed
  per slice in W2. The index, the status mapping, the archived numbering and
  the gates are not started.)

### W2 -- Review, one slice per area

Each area's RFCs are reviewed by the worker who knows them, when they are
free. A slice gets a short prioritized list: its RFCs' tier 1 and tier 2
pairs from W0, cut by W1's `area:`, never the raw pair list. The manager session dispatches the slices:

| area | reviewer |
| --- | --- |
| rmw-wire | 139 |
| build-toolchain-codegen | 241 |
| platform-board-rtos | 118, with 133 |
| workspace-system-model | 142, with 133 |
| test-ci | 175 |
| runtime-api | wtf (it overlaps the ROS-compatibility audit) |
| docs-ux-diagnostics | AUO book, with 243 |

Each RFC gets one verdict, with evidence:

- `keep`
- `amend`
- `supersede-by <RFC>`
- `merge-into <RFC>`
- `archive`

The evidence is judged against the newer RFCs (starting from its rows in
`pairs.md`), the code as it is, and the user's rulings. Mechanical fixes are
made directly in the review PR. Anything that would change a design goes
into ONE decision brief per area: each item states the question, the
options, the measured facts and a recommendation, the way the NuttX decision
brief of RFC-0126 did.

- **Acceptance:** every live RFC has a verdict and an updated
  `last-reviewed`; each area's brief is ruled.
- **Where a slice records its result:** one file per area,
  `docs/roadmap/data/phase-555-w2-<area>.md`, holding the verdict table and
  the area's decision brief. Mechanical fixes are made in the RFCs, in the same
  commit.
- **Status:** OPEN. test-ci reviewed 2026-11-13, brief awaiting a ruling
  ([data/phase-555-w2-test-ci.md](data/phase-555-w2-test-ci.md)). workspace-system-model reviewed
  2026-11-13 (both halves, 21 RFCs), brief awaiting a ruling
  ([data/phase-555-w2-workspace-system-model.md](data/phase-555-w2-workspace-system-model.md)). docs-ux-diagnostics reviewed 2026-11-13 (both halves, 8 RFCs), brief
  awaiting a ruling ([issue 2486](../issues/2486-docs-ux-diagnostics-review-brief.md)).

### W3 -- Apply the rulings

- `supersedes` / `superseded-by` (or `amends`) links are written both ways.
- A superseded RFC moves to `archived/` with its forward pointer.
- `ARCHITECTURE.md` is aligned with every RFC that changes status (the
  AGENTS.md drift rule).
- Each `implements-tracked-by` is updated where a ruling moves work.
- **Acceptance:** `rfc-inventory.py` reports no candidate pair that a ruling
  resolved, and `check rfc-phase-tracking` stays green.
- **Status:** OPEN

### W4 -- Keep it organized

- `last-reviewed` is required, and must be a date.
- A scheduled report lists RFCs not reviewed in N months; the cadence and N
  are to be chosen when this item starts.
- A new RFC must declare its `area:` and name, in frontmatter, the RFCs it
  amends or supersedes, or say `none`.
- **Acceptance:** each rule is a gate on the fast line or a scheduled lane,
  and a new RFC that omits any of them is refused.
- **Status:** OPEN

## Order

W0, then W1 (the area field and the generated index make the W2 slices). W2's
slices run in parallel. W3 follows each area's ruling. W4 can start once W1
lands.

## What this phase does not do

- Change a design without a ruling.
- Review phase docs or issues. Those have their own gates
  (`check phase-item-status`, `check rfc-phase-tracking`).
