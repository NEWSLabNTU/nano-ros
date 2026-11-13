# Phase 560 — Follow the contract revision on the RTOS side

**Status (2026-11-13). W1-W5 DONE** (rlm v0.3.15, play_launch 0.32.0); W6-W8 planned. nano-ros's share of two play_launch
phases: phase 103 (defects the contract grammar review found in shipped
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

| Item | Follows | What changes here | Size |
|---|---|---|---|
| **W1** entering a mode | play_launch 103 D4, after rlm states the meaning | `ftti_budget::reaction_path` (`ftti_budget.rs:386`) reads a mode-form hazard the way rlm defines it. Today it takes the entered mode's own `reaction` and, with none, treats entering as the reaction. The checker instead walks the mode's `fallback` levels. **Done:** rlm v0.3.15 states that the entered mode is the one taken away and its last fallback level, the minimal risk condition, is charged against `ftti`; `reaction_path` resolves that level, so the budget and the observer rows follow it. | S |
| **W2** the omitted `on:` | play_launch 103 D5, after rlm states the meaning | `ftti_budget::classes` (`ftti_budget.rs:243`) returns rlm's default. Today it returns the five pre-v0.3.13 classes, rlm's doc says every class, and play_launch's message says omission, late and loss. **Done:** rlm v0.3.15 defines `FaultKind::DEFAULT` as those five; the local copy is deleted and the budget reads `HazardContract::classes()`. | S |
| **W3** constant predicates | play_launch 103 D1 (rlm model `Constant { constant, value }`) | Tag bump only. No nano-ros code reads `when`, `commands` or a predicate's value today. If the image ever judges a value, it reads `value`, never the constant's name. | S |
| **W4** criticality through the resolver | play_launch 103 D3 | Picks up through the play_launch submodule bump. Check every changed criticality in the generated tables against the hazard that now reaches the node. | S |
| **W5** removals | play_launch 104 S1 (one rlm release) | Tag bump. None of the removed keys (`external_topics:`, topic-level `max_transport`, `paths.input`, `exit.on`, a mode in `hazards.reaction`, `links.monitor`, `tolerance`) is written in nano-ros's contracts or read by its code (`links.monitor` is in the model; nano-ros reads no `contracts.links`). Run `migrate` over the corpus to confirm zero changes. | S |
| **W6** renames and relocations | play_launch 104 S3 (one R release) | Tag bump, then `migrate` over the 33 contracts. Rates become `Hz` values: 44 values; `entity_census.rs` messages and fix-its spell `trigger.timer.rate_hz` (`:850-897`); inline test YAML. The model keeps its field names, so no model reader changes. `supervision { tick, switch }` leaves the contract for the platform file; nano-ros already states the tick per board in `system.toml`. | M |
| **W7** additions | play_launch 104 S4, each when a contract needs it | A new fault class adds one arm to the exhaustive match at `ftti_budget.rs:266`. End-to-end protection (a counter field) is the image's to check if it enforces one. Several hosts: the image reads one model per board, unchanged. | per addition |
| **W8** E7, the same answer as play_launch | play_launch 102 M5 E7 | The image reports the level it fell to, so the evaluation can compare nano-ros's verdict with play_launch's on an omission fault. | M |

## Acceptance

- W1, W2: one rlm sentence per meaning, cited in the code comment that
  implements it; a test that fails on today's reading.
- W5, W6: `migrate` over every nano-ros contract, and the gates report zero
  `deprecated-spelling` warnings.
- Every item lands in a pull request that bumps rlm and the play_launch
  submodule together. Its manifests name the tag the submodule pins.
