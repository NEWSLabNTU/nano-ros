# Phase 560 — Follow the contract revision on the RTOS side

**Status (2026-11-13). W1-W5 DONE** (rlm v0.3.15, play_launch v0.32.0,
#3745); W6 and W7 BLOCKED on releases not yet tagged; W8 DONE. nano-ros's
share of two play_launch phases: phase 103 (defects the contract grammar review found in shipped
behaviour) and phase 104 (the grammar revision itself). The design of
record is rlm `docs/design/grammar-revision/` (an overview and one file per
topic, T1-T10). Nothing here starts before the rlm release it follows.

## The division

| Repository | Role |
|---|---|
| rlm | The runtime-agnostic specification: the grammar, the SystemModel, and what every key and rule means. |
| play_launch | The Linux realizer: resolves launch trees, checks them, observes the running system. Its resolver's layer 2 (no ROS linked) is what nano-ros builds to resolve launch trees. |
| nano-ros | The RTOS realizer: reads the model into a board image and generates its monitors; the board's platform facts live in `system.toml`. |

The review found that three meanings were implemented on each side with no
rlm text to hold them together. Two of them are in nano-ros: what entering a
mode runs (`ftti_budget::reaction_path`) and the default of an omitted `on:`
(`ftti_budget::classes`). For each, rlm states the meaning first, and both
realizers then follow it. nano-ros does not choose its own reading.

## Work items

Each item names the upstream unit it follows. The upstream state on
2026-11-13: rlm v0.3.15 and play_launch v0.32.0 are the latest tags; play_launch
phase 103 (D1-D9) and phase 104 S1 and S2 shipped in them; phase 104 S3 and S4
have no release.

### W1 -- Entering a mode

- **Status:** DONE (2026-11-13, #3745)

Follows play_launch 103 D4, after rlm states the meaning.
`ftti_budget::reaction_path` used to take the entered mode's own `reaction` and,
with none, treat entering as the reaction, while the checker walked the mode's
`fallback` levels. rlm v0.3.15 states that the entered mode is the one taken
away and that its last fallback level, the minimal risk condition, is charged
against `ftti`. `reaction_path` resolves that level, so the budget and the
observer rows follow it. Tests: `a_mode_reaction_resolves_through_the_fallback`
(fails on the old reading) and `a_mode_without_fallback_has_no_reaction`.

### W2 -- The omitted `on:`

- **Status:** DONE (2026-11-13, #3745)

Follows play_launch 103 D5. `ftti_budget::classes` returned nano-ros's own copy
of the five pre-v0.3.13 classes, rlm's doc said every class, and play_launch's
message said omission, late and loss. rlm v0.3.15 defines `FaultKind::DEFAULT`
as those five. The local copy is deleted, and the budget reads
`HazardContract::classes()`, so no budget moves.

### W3 -- Constant predicates

- **Status:** DONE (2026-11-13, #3745: tag bump)

Follows play_launch 103 D1 (rlm model `Constant { constant, value }`). No
nano-ros code reads `when`, `commands` or a predicate's value. If the image ever
judges a value, it reads `value`, never the constant's name.

### W4 -- Criticality through the resolver

- **Status:** DONE (2026-11-13, #3745: submodule bump; no table changes)

Follows play_launch 103 D3: criticality is now derived through the same ladder
resolution as the reaction walk, so a hazard that enters a mode credits its
reacting nodes. The derivation reads hazards, and no nano-ros contract declares
one: none of the 34 tracked `*.contract.yaml` writes `hazards:`, `modes:`,
`enters:` or `fallback:` (measured on main after #3745). So no generated
criticality can change here, and there is nothing to review. The review is owed
when the first nano-ros contract declares a mode-form hazard.

### W5 -- Removals

- **Status:** DONE (2026-11-13, #3745: tag bump; `migrate` dry run clean)

Follows play_launch 104 S1. None of the removed keys (`external_topics:`, a
topic's `max_transport`, `paths.<p>.input`, `exit.on`, a mode in
`hazards.<h>.reaction`, `links.<l>.monitor`, `tolerance`) is written in
nano-ros's contracts or read by its code. Measured with play_launch v0.32.0's
`ros-launch-resolve migrate --check` (rlm v0.3.15): 34 of 34 tracked contracts
and 182 of 182 tracked `system.toml` files scanned, 0 to rewrite.

rlm v0.3.16 and play_launch v0.33.0 (#3765) finished S1: a path's `tolerance`
is refused, and a subscriber's inline `max_transport` is a
`deprecated-spelling` warning, refused at the next rlm release. nano-ros
writes neither key and reads neither model field (`git grep` over tracked
contracts and non-vendored Rust, 2026-11-14), so that refusal costs a tag
bump.

### W6 -- Renames and relocations

- **Status:** BLOCKED (rlm's rename release R, play_launch 104 S3; not tagged as of rlm v0.3.16)

Tag bump, then `migrate` over the contracts. Rates become `Hz` values (44
values); `entity_census.rs` messages and fix-its spell `trigger.timer.rate_hz`
(`:850-897`); inline test YAML. The model keeps its field names, so no model
reader changes. `supervision { tick, switch }` leaves the contract for the
platform file; nano-ros already states the tick per board in `system.toml`.
Size M.

### W7 -- Additions

- **Status:** BLOCKED (each play_launch 104 S4 addition; none released as of rlm v0.3.16)

Each lands when a contract needs it. A new fault class adds one arm to the
exhaustive match in `ftti_budget.rs`. End-to-end protection (a counter field,
T7) is the image's to check if it enforces one. Several hosts (T10): the image
reads one model per board, unchanged.

### W8 -- E7, the same answer as play_launch

- **Status:** DONE (2026-11-14, rule `hazard-level-runtime`)

The image reports each level its ladder becomes active on, by index and name.

Follows play_launch 102 M5 E7, which play_launch's report records as not run.
The image reports the fallback level it fell to, so the evaluation can compare
nano-ros's verdict with play_launch's on an omission fault. It needs W1's
meaning, which shipped in #3745. Size M.

## Acceptance

- W1, W2: one rlm sentence per meaning, cited in the code comment that
  implements it; a test that fails on today's reading.
- W5, W6: `migrate` over every nano-ros contract, and the gates report zero
  `deprecated-spelling` warnings.
- Every item lands in a pull request that bumps rlm and the play_launch
  submodule together. Its manifests name the tag the submodule pins.
