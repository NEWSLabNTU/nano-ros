---
id: 1233
title: "Two derived facts take the resolver road only, and the tree says nowhere why — `NROS_DERIVED_EXECUTOR_MAX_NODES` and `NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE`"
status: resolved
type: tech-debt
area: cmake, core, memory
severity: medium
found: 2026-09-08
related: [1122, 1125, 1198, 1199, 0827, 0900, 0963, phase-448]
resolved: 2026-09-09
---

# Four road-gaps with no stated reason

phase-412 #7 gave every published `NROS_DERIVED_*` fact a stated disposition on
each of the three roads a derived number takes into a compile (resolver /
sidecar / declared — the table is in
[1199](1199-derived-counts-need-the-declared-road.md)). Twelve of the
fourteen facts came out either carried, or deliberately not carried with a
comment already in the tree that says so.

Two did not. **`NROS_DERIVED_EXECUTOR_MAX_NODES`** and
**`NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE`** are resolved by
`zephyr/cmake/nros_cargo_build.cmake` and reach no other road: not the leaf
sidecar (`DERIVED_ENV_KEYS` / `DERIVED_PAYLOAD_ENV_KEYS` in
`packages/cli/nros-cli-core/src/leaf_entity_env.rs`), not the declared road
(`NROS_DECLARED_*` out of `cmake/NanoRosEntityFacts.cmake`). That is four
road-gaps over two facts.

**No comment, issue or `NOT_DERIVED_*` constant anywhere in the tree explains
any of the four.** That is what this issue records. The gaps may well be
correct; nobody has said so, and inventing a justification in the registry would
make the registry the authority on a decision it did not take.

Contrast the two that ARE explained:

* `NROS_DERIVED_MAX_QUERYABLES` is off both cargo roads because
  `leaf_entity_env.rs` says so at length —
  "`ZPICO_MAX_QUERYABLES` is DELIBERATELY NOT DERIVED" — the count excludes the
  param and lifecycle service families, a leaf has no channel to complete it,
  and a short queryable table is a registration failure at boot rather than a
  smaller pool. The CMake road carries the two RAW inputs instead
  (`NROS_DECLARED_SERVICE_SERVERS` + `NROS_DECLARED_INFRA_QUERYABLES`).
* `NROS_DERIVED_LARGEST_TYPE` / `_LARGEST_RX` / `_LARGE_TYPES` are on no road
  because they are provenance, which
  `cmake/NanoRosMessageBounds.cmake`'s own header states.

## Why it matters

Issue 1122 was exactly this shape: a size knob computed on every lane and
consumed only under `zephyr/`, worth 131,072 B of `LARGE_PAYLOADS` on a
publish-only FreeRTOS image. 1199 swept seven more onto the declared road.
`NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE` is the **fourth size knob** and it was
not swept in with the other three — it sizes the runtime-owned take buffer
(`RX_BUF`, which `DEFAULT_TX_BUF` aliases), so a cargo leaf or a leafless CMake
image takes the crate default for a buffer its own configure measured.

The usual argument for a sidecar omission — "a leaf cannot see what it needs to
derive from" — is not obviously available for either of these. `EXECUTOR_MAX_NODES`
is a count of nodes; `SUBSCRIPTION_BUFFER_SIZE` is derived over the linked
CLOSURE, which is precisely the set a cargo leaf's own dependency graph has.

## What is already in place

`scripts/check-declared-fact-carriers.py` carries the four gaps as `OpenGap`
entries in `FACT_DISPOSITION` and PRINTS them on every run, on the success path.
They are recorded, not absorbed: a fifth gap cannot appear without either a
quoted reason or a new `OpenGap` row.

## To close

For each of the four, either:

* put the fact on the road (the mechanism exists on both — 1199 did seven at
  once), turning the `OpenGap` into a carried tuple; or
* write the reason where the decision lives — a comment beside the knob, in the
  shape of `NOT_DERIVED_NEEDS_INFRA_COUNT` — and turn the `OpenGap` into a
  `NotCarried` citing it. The gate re-reads the cited file, so the reason cannot
  later be deleted without the gate failing.

Acceptance: `just check declared-fact-carriers` prints zero OPEN roads.

## Resolved — all four gaps closed by DELIVERY, not by a reason

Both facts now travel all three roads, and
`just check declared-fact-carriers` prints **12 declared facts produced,
consumed and watched** with zero OPEN roads.

The gaps were investigated for a reason to write down and none held up:

* **`NROS_DERIVED_EXECUTOR_MAX_NODES`.** phase-412's ground for withholding it
  was that under-counting HALTS the board. It does — as
  `NodeError::NodeTableFull`, a NAMED failure at registration. That is a
  property of the failure, not of the road the number travelled, and the same
  derived count already reached the resolver road, where the identical halt
  was accepted. Delivered on both remaining roads, UNFLOORED: the floor belongs
  to the consumer that names the knob (issues 1015 / 1033), and this one sizes
  Rust tables where a short count is that named error and not a `#error`.
  Second consumer found and wired the same way:
  `packages/rmw/zenoh/nros-rmw-zenoh/build.rs`, which keeps its own `min 1`.
* **`NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE`.** Left behind by 1122 because its
  GUARD differs from the payload trio's: those need
  `PAYLOAD_STATUS derived` AND `BASIS subscribed`, while the take buffer is
  published under `NROS_MESSAGE_BOUNDS_STATUS derived` over the CLOSURE
  ("BASIS `closure`, always. Narrowing this one is the under-derivation." —
  `cmake/NanoRosMessageBounds.cmake`). So it needed its own carrier rather than
  a row in an existing one: `_nros_take_buffer_env` on the declared road, and
  a third key list `DERIVED_CLOSURE_ENV_KEYS` fed by a new
  `nros_cli_core::leaf_take_buffer` on the sidecar road. That module derives
  over every priced type in the leaf's `generated/` tree and REFUSES on any
  unbounded member, on the same rule the CMake lane refuses by — a type whose
  size is unknown makes the maximum unknown.

Measured ladders (`nros_node_config.rs` consts, host build):

| env | `DEFAULT_RX_BUF_SIZE` | `MAX_NODES` |
| --- | --- | --- |
| neither | 1024 | 4 |
| `NROS_DECLARED_*` = 880 / 2 | 880 | 2 |
| declared + named override 333 / 6 | 333 | 6 |

`env > Kconfig/board > derived > crate default` holds on both: a derived value
is a DEFAULT, never an override.

**Postscript (2026-09-11, phase-448 W6 / [issue 1198](1198-executor-node-and-sc-slots-are-undeclared-defaults.md)).**
The executor's OTHER fixed table, `MAX_SC`, was never one of the four gaps here
because it was never PUBLISHED as a fact at all — so no registry could record
it as missing a road. It travels all three roads now, on the same terms this
issue settled for the node table: the exhaustion path NAMES the knob, which is
a property of the failure and not of the road. Worth 672 B of a FreeRTOS
executor backing on top of the 3,672 B the node table gave back here.
