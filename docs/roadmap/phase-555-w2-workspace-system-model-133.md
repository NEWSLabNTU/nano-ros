# Phase 555 W2 — workspace-system-model brief, 133's half

**Status (2026-11-13). IN PROGRESS — the verdicts are in; the items below await the user's ruling, and 142 merges this half into the area brief.**

This is 133's half of the workspace-system-model decision brief for
[phase-555](phase-555-design-doc-review.md) W2. It covers RFC-0063, 0074,
0078, 0079, 0086, 0100, 0103, 0121, 0122 and 0125. It is to be merged with
142's half (RFC-0004, 0015, 0024, 0025, 0026, 0045, 0046, 0047, 0050, 0052,
0060) into one brief for the area.

Mechanical fixes were made in the RFCs and are listed in each RFC's
`## Review (2026-11-13, phase-555 W2)` section. Only design questions are
below.

## Verdicts

| RFC | title | verdict | reason |
| --- | --- | --- | --- |
| 0063 | The SystemModel is a build artifact | keep | Built and gated; only issue 1121's status was stale (fixed). |
| 0074 | Ingress budget: rate and burst | keep | Stable; renamed names match the code; open work is phase-519's deferred W1/W2. |
| 0078 | A WCET bound is declared per profile | keep | Built as written; status is Draft though all its work is archived (W1). |
| 0079 | Priority is allocated, not authored | keep | Every port declares a plan; status is Draft though its phase is archived (W1). |
| 0086 | Unified configuration, transport tenant | keep | D3-D6 built; two descriptors still in `config/` (W2); deprecated features past their release (W3). |
| 0100 | One sizing model | keep | Built and landed through Amendment 3; status is Draft (W1). |
| 0103 | One owner per fact | amend | D7's generated-path table names two files the code does not write (W4). |
| 0121 | Workspace-first project layout | keep | In flight under phase-531; frontmatter fixed. |
| 0122 | Behaviour bringups, witnesses, cadence CI | keep | In flight under phase-535; consistent with the tree. |
| 0125 | Topic visibility and locality | keep | Draft with no phase; W0 done (W5). |

## Items

### W1 — Promote three finished Drafts to Stable?

**Question.** RFC-0078, RFC-0079 and RFC-0100 are `Draft`, but the work each
names is done. Should they be `Stable`?

**Options.**
- (a) Promote all three.
- (b) Promote 0078 and 0100; keep 0079 Draft until the RTOS foreign-band
  enumerators exist.
- (c) Leave all three Draft.

**Measured facts.**
- RFC-0078: `implements-tracked-by` is issue-0404, issue-0259, issue-0403 and
  phase-463, all under `archived/`. Every type, error and test it names exists
  (`nros-cli-core/src/orchestration/cargo_metadata_schema.rs`,
  `nros-orchestration-ir/src/wcet.rs`). Its one open question (verify a
  profile's clock against the part) needs a platform clock-rate query that
  no phase plans.
- RFC-0079: phase-459, issues 0506, 0623, 0736 and 0765 are archived. Every
  port declares `[board.priority_plan]` (FreeRTOS x2, NuttX, ThreadX, Linux,
  Zephyr derived). Open: foreign-band enumerators on RTOS ports, headroom
  policy, per-port profile overrides.
- RFC-0100: phases 454, 457, 486 and 517 are archived; Amendment 3 says
  LANDED. The only open work touching it is RFC-0121's retirement of the
  leaf producer (phase-531), which is another RFC's change.
- RFC-0074 was promoted with an open phase-519 behind it ("Each would amend
  this RFC, not reopen it"), so open follow-ups have not blocked Stable
  before.

**Recommendation.** (a). The open questions are follow-ups that would amend
each RFC, the same footing RFC-0074 was promoted on.

### W2 — RFC-0086 D3: the two platform descriptors left in `config/`

**Question.** D3 moves every platform descriptor from `config/<name>/` into
its crate. Two remain. Should they move, or should D3 say which stay?

**Options.**
- (a) Move `bare-metal` into `packages/platform/nros-baremetal-common/` and
  `generic` into a crate of its own (or into `nros-platform`).
- (b) Amend D3: a platform with no crate of its own keeps its descriptor in
  `config/`, and the search path keeps `config/` as a root.
- (c) Leave both as they are, undocumented.

**Measured facts.**
- `git ls-files | grep nros-platform.toml$`: five under
  `packages/platform/nros-platform-<name>/`, plus `config/bare-metal/` and
  `config/generic/`.
- `packages/platform/` has `nros-baremetal-common` but no
  `nros-platform-bare-metal` or `nros-platform-generic` crate.
- `nros-cli-core/src/orchestration/board_descriptor.rs:237` still cites
  `config/bare-metal/` as the descriptor spelling.

**Recommendation.** (b) now, since the two have no crate to live in; (a)
only if a crate is created for another reason.

### W3 — RFC-0086 D5: remove `nros-log`'s deprecated features?

**Question.** D5 says the old `nros-log` pick-one features are "deprecated
for one release". A release has shipped. Should they be removed now?

**Options.**
- (a) Remove the five feature families now, keeping a build error that
  names the knob for anyone who still sets one.
- (b) Keep them one more release.

**Measured facts.**
- The deprecation landed 2026-10-06 (`git log -S'DEPRECATED (issue 1037'
  -- packages/core/nros-log/Cargo.toml`).
- `nros-v0.6.0` was tagged 2026-10-14; seven `nros-0.6.0-nros*` builds
  followed, the latest on 2026-11-07.
- `packages/core/nros-log/Cargo.toml` lines 59-88 still define the features
  (for example `dynamic-loggers-0 = []`).
- RFC-0103's changelog (2026-10-14) chose to keep retired-name error tables
  for good, because "a dropped row makes an old name silently ignored, not
  refused".

**Recommendation.** (a), keeping the refusal, so the same rule as RFC-0103's
retired names applies.

### W4 — RFC-0103 D7: the generated-path table does not match the code

**Question.** D7 lists where generated output lives. Two rows name files the
code does not write. Amend the table to the code, or change the code?

**Options.**
- (a) Amend D7 (and D4's mention of `locations.cmake`) to the code: the cargo
  config is `nros-cargo.toml` in the image's entry directory, and the cmake
  locations are GLOBAL properties set by `cmake/NanoRosLocate.cmake`, with no
  file.
- (b) Change the code to D7's paths (`<build>/nros/cargo.toml`,
  `<build>/nros/locations.cmake`).

**Measured facts.**
- `nros-cli-core/src/builder/cargo_config.rs`: `pub const FILE_NAME: &str =
  "nros-cargo.toml"`, "inside the image's entry directory".
- `git grep locations.cmake -- cmake zephyr packages` has no hit.
  `cmake/NanoRosLocate.cmake` runs `nros locate --all --format cmake` once
  per configure and "keeps the answers in GLOBAL properties". It keeps D4's
  other promises (normal variables, never `CACHE`).
- The other D7 rows hold: `resolved.toml` (`resolve.rs`), models under
  `<build>/nros/models/<bringup>/`, sizing under `<build>/nros/sizing/`.
- RFC-0103 is `Stable`, and its changelog says phase-484 W1-W7 implemented
  D1-D9.

**Recommendation.** (a). The behaviour D4 wants (one ask per configure,
normal variables) is built; only the file D7 imagined is not, and nothing
reads one.

### W5 — RFC-0125: open its phase now?

**Question.** RFC-0125 has decisions (2026-11-10) and W0 done, but no phase.
Open one now, or wait for the rlm grammar?

**Options.**
- (a) Open the phase now with Tier 1 (W2) first: per D2 and Decision 1 it
  needs only `[image.<id>] matched_only_wire`, not the rlm grammar.
- (b) Wait until rlm ships `visibility`/`closed`/`locality()`, then open the
  phase with W1.

**Measured facts.**
- `implements-tracked-by: []`; "Work items (phase to be opened)".
- W1 and W3 need rlm's grammar. W2 (Tier 1) is opt-in per image and uses
  zenoh-pico's existing write filter (`Z_FEATURE_INTEREST`, on for every
  target).
- `matched_only_wire` and a backend setting `from_intra_process` have no hit
  under `packages/`.
- The rlm design doc and the play_launch companion doc it builds on were not
  found in any local checkout (unverified whether they are published).
- `scripts/check-rfc-phase-tracking.py` asks every live RFC to name a phase
  or state `implementation: none`.

**Recommendation.** (a): open the phase with W2 first, and W1/W3 behind the
rlm release.
