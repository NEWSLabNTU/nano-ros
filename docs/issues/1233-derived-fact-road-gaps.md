---
id: 1233
title: "Two derived facts take the resolver road only, and the tree says nowhere why — `NROS_DERIVED_EXECUTOR_MAX_NODES` and `NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE`"
status: open
type: tech-debt
area: cmake, core, memory
severity: medium
found: 2026-09-08
related: [1122, 1125, 1199, 0827, 0900, 0963]
---

# Four road-gaps with no stated reason

phase-412 #7 gave every published `NROS_DERIVED_*` fact a stated disposition on
each of the three roads a derived number takes into a compile (resolver /
sidecar / declared — the table is in
[1199](archived/1199-derived-counts-need-the-declared-road.md)). Twelve of the
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
