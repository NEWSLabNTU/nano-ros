---
id: 284
title: "NROS_CYCLONEDDS_MAX_TYPES is a hidden compile-time knob, derivable from source metadata"
status: open
type: enhancement
area: build
---

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
