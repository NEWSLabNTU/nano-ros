# Phase 172 — Orchestration follow-ups (deferred from Phase 126)

**Goal.** Land the capabilities that Phase 126 (ROS 2 workflow
orchestration MVP) explicitly deferred. Phase 126 shipped the
end-to-end MVP — source metadata → launch plan → checked
`nros-plan.json` → generated per-board binary, verified across 9
boards — and listed nine deferrals as out of scope for the MVP.
This phase tracks them as discrete, independently-schedulable work
items.

**Status.** Not Started.

**Priority.** P2 — none block the MVP workflow; each is an
ergonomic or capability upgrade on top of a working pipeline.

**Depends on.** Phase 126 (archived) — the schema, planner,
checker, generator, and per-board templates this phase extends.

## Background

Phase 126's "Deliberate deferrals" section enumerated nine items
kept out of the MVP to keep the first end-to-end slice tractable.
The MVP is now complete and archived; these are the natural next
increments. They are listed here verbatim from the 126 doc, each
expanded into a work item with a sketch of scope.

## Work items

- [ ] **172.A — Lifecycle node orchestration.** Model
      managed-lifecycle nodes (configure / activate / deactivate /
      cleanup transitions) in the plan schema + generated runtime.
      Today every instance is a plain node brought up once at boot.
      Scope: lifecycle state in `nros-plan.json`, transition
      callbacks in the generated runtime, `nros check` validation
      of lifecycle graphs.

- [ ] **172.B — Automatic callback-chain inference.** Infer
      callback execution chains (which callback feeds which) from
      the topic graph instead of requiring explicit bindings.
      Scope: dataflow analysis in the planner; emit inferred chains
      into the plan with an override escape hatch.

- [ ] **172.C — Automatic callback-group inference.** Derive
      callback groups (mutually-exclusive vs reentrant) from the
      graph + scheduling annotations rather than hand-authored
      groups. Scope: planner heuristic + `nros-plan.json`
      representation + generated `SchedContext` binding.

- [ ] **172.D — Incremental / staleness-aware build.** Skip
      regeneration + recompilation when the plan + sources are
      unchanged. Today `nros build` regenerates the package every
      run. Scope: content-hash the plan + component metadata; gate
      `generate_package` + the cargo invocation on staleness.

- [ ] **172.E — Hardened metadata-mode sandboxing.** The
      `nros metadata` mode compiles + runs component code to
      extract source metadata. Harden that execution (resource
      limits, filesystem/network restrictions) so untrusted
      component crates can't escape during metadata extraction.

- [ ] **172.F — Polished `nros explain`.** A user-facing command
      that explains the generated plan: which launch node maps to
      which component, how params resolved, why a SchedContext was
      chosen, what each generated table contains. Scope: a
      readable, structured rendering of `nros-plan.json` + the
      generation trace.

- [ ] **172.G — Multi-tier scheduling.** Extend the single-tier
      `SchedContext` model to multiple scheduling tiers (e.g. a
      high-rate RT tier + a best-effort tier within one executor).
      Depends on the Phase 110 scheduling primitives. Scope: plan
      schema for tiers + generated multi-tier executor wiring.

- [ ] **172.H — Runtime parameter-override persistence.** Persist
      runtime parameter overrides (set after boot) across restarts.
      Today parameters come from the plan + launch manifest at
      generation time only. Scope: a persistence backend (flash /
      file) + load-on-boot in the generated runtime.

- [ ] **172.I — Generated shared state.** Support shared state
      between components in one generated binary (e.g. a shared
      blackboard / typed shared region) instead of every component
      owning isolated state. Scope: plan representation + generated
      `static` shared-region tables + access discipline.

## Acceptance criteria

Each work item is independently shippable. A work item is done when:

- [ ] Its capability is represented in `nros-plan.json` (where it
      affects the plan) with round-tripping fixtures.
- [ ] `nros check` validates the new construct.
- [ ] The generated runtime exercises it, verified by an
      `orchestration_e2e` fixture (or a unit test where no generated
      binary is involved).
- [ ] Docs show the workflow for the new capability.

## Notes

- These were all listed under Phase 126's "Deliberate deferrals".
  Splitting them into their own phase keeps 126 archivable as a
  clean MVP while preserving the backlog.
- No ordering is forced between work items; pick by user demand.
  172.D (incremental build) and 172.F (`nros explain`) are the
  lowest-risk ergonomic wins; 172.A (lifecycle) and 172.G
  (multi-tier scheduling) are the largest.
