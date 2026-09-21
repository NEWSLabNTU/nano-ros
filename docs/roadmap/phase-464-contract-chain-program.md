# phase-464 - the contract-chain program: milestones, claims, and the parallel order

**Status (2026-09-21). PROPOSED; an umbrella, nothing lands under this number.**
This phase owns no code. It sequences six nano-ros phases, one play_launch
phase, one ros-launch-manifest design issue and the safety island's own
units into milestones ordered by what each guarantees, and it fixes the
protocol several agent sessions use to work on them at once. The phases it
indexes are [457](phase-457-consume-the-shared-derivation.md),
[459](phase-459-cmake-image-reaches-tier-derivation.md),
[460](phase-460-formal-checks-on-the-contract-chain.md),
[461](phase-461-service-inbox-per-family.md),
[462](phase-462-safety-vocabulary-on-target.md) and
[463](phase-463-host-census-reconciles-contract-with-code.md); outside this
repository, play_launch `docs/roadmap/phase-78-one-derivation-two-consumers.md`,
ros-launch-manifest `docs/design-issues.md` #52, and the island's
`docs/roadmap/phase-5-contract-chain.md`.

## Why an umbrella

The gap review of 2026-09-18 (briefs A to D, re-verified against `783cdfa14`
on 2026-09-21) produced seven themes and thirty-odd findings. Filed as six
phases they read as six independent plans, which they are not: 459 cannot
rank from the contract's own facts until 457 consumes the shared derivation,
457 cannot start until play_launch 0.12.0 ships against rlm v0.1.37, 462 W3
needs both 459 and 457, and 461 is the only item that needs hardware. A
session picking a wave by reading one phase doc picks blind. The owner's
order is: correctness of what the chain says first, then the real-time and
system guarantees reaching the target, then the board, then the safety
vocabulary. This document is that order, with one claimable id per unit.

## The protocol

- **One unit, one claim, one branch, one PR.** A unit is a wave with a claim
  id of the form `phase-NNN-Wk` (nano-ros), `phase-78-Wk` (play_launch),
  `phase-52-Rk` (rlm) or `island-Wk` (the island). In nano-ros, `just claim
  <id>` before touching a file; in the other three repositories the claim is
  the branch named after the id plus a draft PR opened before the first
  commit of substance. An open PR supersedes the claim.
- **Disjoint ownership.** Every unit's `Owns:` footer lists the files it
  edits. Two units whose `Owns:` intersect are serialised in the phase docs'
  "Parallel plan" sections; a session may not take a unit whose dependencies
  are not merged. A unit that finds it needs a file another unit owns stops
  and says so in its PR rather than editing it.
- **Status lives in the footer.** `Status: not started | claimed <date>
  <session> | PR #N | landed <commit>` on the wave's footer line; nothing
  else is updated by hand. `just check fast` carries the roadmap gates.
- **Pins move forward only, in one commit each.** The cross-repo chain is
  rlm tag -> play_launch pin and release -> nano-ros pins (rlm tag and
  play_launch gitlink in ONE commit) -> island submodule. A pin bump is its
  own unit and its own PR.
- **Evidence over projection.** Every wave's gate is a test, a script or a
  measured artifact. The island's numbers are the fixture: four MRM nodes,
  33 entities, timers at 10 and 30 Hz, the S32K344's 320 KiB.

## Milestones

Read each table row as: unit, what it guarantees once landed, gate, depends
on. "now" means no unmerged dependency. Units within a milestone run in
parallel unless the last column says otherwise.

### M0 - the toolchain stops lying (this week; every unit is one day or less)

| unit | guarantee | gate | depends on |
| --- | --- | --- | --- |
| phase-460-W1 | a refused resolve leaves no model any consumer can read (issue 1420) | `check-model-freshness` and a negative control | now |
| phase-460-W2 | a partial `params:` declaration refuses instead of falling to crate defaults (1421) | build refusal names the node | now |
| phase-460-W4 | `system.toml` domain and Kconfig domain agree, or configure says which wins (1423) | configure line + gate | now |
| play_launch 0033 | strict enforcement signals the children and the supervisor exits | runtime_enforcement test | now |
| play_launch 0031 | `--enforce-rules warn` without interception says so | unit test on the config | now |
| play_launch 0032 | strict trips on errors, warnings are reported | runtime_enforcement test | now |
| rlm 0037 | `dangling-entity` and `service-wiring` honour `external:` | check crate tests | now |

### M1 - the contract means one thing (two weeks, two parallel tracks)

Track B, the shared derivation, is the critical path of the program.

| unit | guarantee | gate | depends on |
| --- | --- | --- | --- |
| rlm phase-52-R1 | the model carries every fact the checker resolves per entity | model golden + format-reference | now |
| rlm phase-52-R2 | one `mapper_input_from_model` for both consumers | derive crate tests | now (rebases onto R1) |
| rlm phase-52-R3 | rank parity: snapshot of `chain_aware_rank(from_model(...))` | split-parity test extended | R1, R2 |
| rlm phase-52-R4 | tag v0.1.37 | CHANGELOG, tag | R3 |
| play_launch phase-78-W1 | pin v0.1.37, lower the new fields into the model | resolve tests | R4 |
| play_launch phase-78-W2 | consume the shared derivation | sched tests | W1 |
| play_launch phase-78-W3 | `from_dump == from_model` on every Autoware fixture | transition gate | W2 |
| play_launch phase-78-W4 | delete the private copy, ship 0.12.0 | release | W3 |
| phase-457-W1 | pins bumped, fixtures re-resolved | `just check fast` | play_launch W4 |
| phase-457-W2 | `mapper_input.rs` is a call into rlm | orchestration-ir tests | 457-W1 |
| phase-457-W3 | the island ranks from timers with every `min_rate_hz` removed | parity fixture | 457-W2 |
| phase-457-W4 | `deadline_us` folds through the shared input | test | 457-W2 |

Track A, the census, is independent of track B and of everything in M2.

| unit | guarantee | gate | depends on |
| --- | --- | --- | --- |
| phase-463-W0 | the reference consumer is measured before it changes | recorded numbers in the phase | now |
| phase-463-W1 | the recorder tells the whole truth (QoS, parameters) | metadata-mode test | 463-W0 |
| phase-463-W2 | the native entry is the census producer | census file with provenance | 463-W1 |
| phase-463-W3 | every delta between contract and code is a named verdict | gate on the island fixture | 463-W2 |
| phase-463-W4 | the census runs at RTOS configure and goes stale by content | configure refusal + freshness | 463-W3 |

### M2 - the guarantees reach the target (two weeks; after M1 track B)

| unit | guarantee | gate | depends on |
| --- | --- | --- | --- |
| phase-459-W0 | a cmake fixture with groups and no declared tiers exists | fixture builds | now |
| phase-459-W1 | `codegen-system` reads cmake callback groups (1426) | test on the fixture | 459-W0 |
| phase-459-W2 | the entry derives, or reads what the bake derived | `run_tiers` in the generated entry | 459-W1 |
| phase-459-W3 | `[tiers.X] derived = true` is the request | parser + test | 459-W2, rlm R1 |
| phase-459-W4 | priorities allocated from the board's plan, below the transport band (1427) | realizer test, island projection 30 Hz -> 5, 10 Hz -> 6 | 459-W2 |
| island-W1 | the island declares its groups; two 30 Hz nodes above two 10 Hz on their own threads, measured on native_sim | demo VERDICT + thread listing | 459-W2, 459-W4 |
| phase-462-W1 | C++ entries bake the monitor table; `min_rate_hz` and `max_latency` watched on target | emitter parity test | now |
| phase-462-W2 | `on_violation` lowers to the executor's deadline action and a silence monitor | executor test | 462-W1, 457-W2 |
| phase-459-W5, W6 | the hand-run form is refused; code and keyword agree | gates | 459-W2 |
| phase-459-W7 | the trigger rate is read from the shared input | test | 457-W2 |
| phase-463-W5, W6 | compatibility invariants as gates; retire the max, flip the island | gates; island-W2 | 463-W4 |
| island-W2 | the island's census is a configure gate | the island's board-build recipe refuses a stale census | 463-W4 |

### M3 - the board runs (one to two weeks; overlaps M1 and M2; the only milestone needing hardware)

| unit | guarantee | gate | depends on |
| --- | --- | --- | --- |
| phase-461-W1 | the inbox ring is a header over caller-visible storage | rmw-zenoh tests | now |
| phase-461-W2 | the parameter and lifecycle families bring their own inbox at depth 1, sized from the declared shape, const-asserted | nros-node build assert | 461-W1 |
| phase-461-W3 | service and action request types are priced | bound inventory | now |
| phase-461-W4 | an inbox drop is counted and said once | test | 461-W1 |
| phase-461-W5 | the island links: projected 298,552 of 327,680 B | map report | 461-W2, 461-W3 |
| island-W3 | the board image links and the map says why | region report in docs/nxp-deployment.md | 461-W5 |
| phase-461-W6 | a store without a server, as the fallback | image with 0 param queryables | now |
| island-W4 | first execution on silicon (MCU-Link probe), heap high-water read | boot log, `nros_zephyr_heap_peak` | island-W3, hardware |
| phase-460-W5 | the heap knob is gated against the measurement (1424) | gate | island-W4 |
| phase-460-W7 | a fault reaches a hook a console-less board can read (1425) | test | now |
| phase-460-W3, W6 | ceilings compared to derived bounds (1368, 1422); slot release gate | gates | now |

### M4 - the safety vocabulary on target (after M2)

| unit | guarantee | gate | depends on |
| --- | --- | --- | --- |
| phase-462-W3 | `safe_state`, hazards, the FTTI budget against the tier plan | build-time check | 462-W2, 459-W4, 457-W2 |
| phase-462-W4 | modes as tier-table variants | codegen test | 462-W2, 457-W2 |
| phase-462-W5 | deadline and liveliness QoS from the contract | rmw tests | rlm R1, 462-W2 |
| phase-462-W6 | the ingress budget knob | measured stall count | now (schema half after rlm 0760) |

### M5 - rolling

play_launch 0034 to 0036, rlm 0038 and 0039, the docs each phase lists under
"docs to update", and phase-463-W7 (profiling) only after a separate
decision.

## Critical path

M0 -> rlm R1..R4 -> play_launch 78 W1..W4 -> 457 W1..W2 -> 459 W1..W4 ->
island-W1, about five weeks of serial work; everything in track A, M3 and
462 W1 runs beside it. The program is done when the island runs on silicon
with its four nodes on derived priorities, its counts reconciled against
its code, its deadlines enforced by the kernel where it can and by the
executor where it cannot, and every number in `docs/nxp-deployment.md`
traceable to a gate.

## Not in this program

The Linux side's own reservations (`SCHED_DEADLINE` from budgets) beyond
what phase 78 ships; a second Autoware subsystem on Linux; the deck. Each is
a decision, not a wave.
