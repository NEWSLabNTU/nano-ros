---
id: 284
title: "NROS_CYCLONEDDS_MAX_TYPES is a hidden compile-time knob, derivable from source metadata"
status: resolved
type: enhancement
area: build
---

## RESOLUTION (2026-07-28) — full derive + auto-emit

`codegen-system` now DERIVES `NROS_CYCLONEDDS_MAX_TYPES` from the SystemModel and
auto-sizes it, so a CycloneDDS image can no longer `RegistryFull` at runtime for
lack of a hand-set knob.

- **Count** (`nros-orchestration-ir::cyclonedds_type_sizing::count_dds_types`):
  the DISTINCT DDS type names the entry registers. The model is COMPLETE for
  types (only pub/sub/service/action register DDS types — timers/guard
  conditions register none), so no source-metadata union is needed (unlike the
  0257 callback count). The registry holds EXPANDED types, so the count mirrors
  `nros-node`'s `register_type` sites: **msg = 1, service = 2** (Request+Response),
  **action = 8** envelopes **+ 3** shared `action_msgs` types once per entry with
  any action. `derive_max_types` rounds the count up to the next power of two
  (heapless' `FnvIndexMap` constraint), never below the default 32.
- **Emit** (`model_ingest::{resolve,manage}_cyclonedds_max_types`): when the
  count exceeds the default, write `NROS_CYCLONEDDS_MAX_TYPES = { value = "<pow2>",
  force = true }` into `<workspace>/.cargo/config.toml [env]` (tagged
  `# nros-managed`, format-preserving). Cargo's `[env]` reaches the dep crate's
  `option_env!` — verified end-to-end. A model that shrinks back under the default
  removes the managed line; a user's own (un-tagged) env line is never clobbered.
- **Gate**: if the user PINNED `NROS_CYCLONEDDS_MAX_TYPES` smaller than the count,
  the bake fails loud naming the count + the value to set (0257's
  `check_executor_capacity` precedent) instead of auto-overriding their intent.

Fragility mitigation (the expansion factors couple to `nros-node`'s register
sites): a doc-comment cross-references those sites + the unit test
`expansion_matches_documented_factors` pins the arithmetic, so a drift breaks the
build. Tests: `cyclonedds_type_sizing` (6, in orchestration-ir) +
`cyclonedds_type_capacity_tests` (6, in the CLI — resolve policy + emit/remove +
user-line preservation). `option_env!` reachability of a cargo `[env] force`
value confirmed with a scratch crate.

## Finding

Split out of issue 0257 when phase-307 closed that issue's executor-sizing
scope. `NROS_CYCLONEDDS_MAX_TYPES` is the same shape of defect as the
`NROS_EXECUTOR_MAX_CBS` knob that 0257 was opened for: a compile-time
capacity the user discovers only when a bringup exceeds it, with no
build-time hint.

## Why it is now cheap to fix

Phase-307 built the whole input path and 0257's fix demonstrates the
pattern end to end:

* `nros sync` produces a `source-metadata.json` per Rust node package and
  keeps it fresh (content-addressed provenance stamp).
* `nros-orchestration-ir::count_callbacks_with_recorded` merges the
  recorded entity set with the SystemModel's wiring as a per-node
  `max(...)`, and both bakes — the CLI's `codegen-system` and the
  `nros::main!` macro — call that one implementation.

Distinct msg/srv TYPES per entry are derivable from the same sidecars: a
recorded publisher / subscriber / service / action each names its
`interface` (`{package, name, kind}`), so the distinct-type count is a
set-cardinality over the same data the slot count already walks.

## Shape of the fix

1. Count distinct `interface` refs across the entry's nodes, unioned with
   the model's `msg_type` / `srv_type` wiring (same max-of-two-incomplete-
   sources reasoning as 0257 — neither input is complete alone).
2. Emit the derived value where the entry is sized, and fail the bake
   loudly when a declared or default capacity is known-too-small, naming
   the count. 0257's `executor_sizing_bake_gate.rs` is the precedent for
   what that error must say.
3. C/C++ node packages have no sidecar until
   `docs/roadmap/phase-308-cpp-metadata-producer.md` lands; they keep the
   current behaviour, so nothing regresses.

## Cross-refs

* `docs/issues/archived/0257-executor-max-cbs-not-derived-from-model.md`
* `docs/roadmap/phase-307-metadata-mode-completion.md`
* `docs/roadmap/phase-308-cpp-metadata-producer.md`
