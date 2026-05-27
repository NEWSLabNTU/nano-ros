# Phase 172 — Orchestration follow-ups (deferred from Phase 126)

**Goal.** Land the capabilities that Phase 126 (ROS 2 workflow
orchestration MVP) explicitly deferred, **plus the configuration
consolidation absorbed from Phase 116** (archived 2026-05-27). Phase
126 shipped the end-to-end MVP — source metadata → launch plan →
checked `nros-plan.json` → generated per-board binary, verified across
9 boards. This phase organizes the remaining work into **four parallel
work groups** (see below).

**Status.** Not Started.

**Priority.** P2 — none block the MVP workflow; each is an
ergonomic or capability upgrade on top of a working pipeline.

**Depends on.** Phase 126 (archived) — the schema, planner,
checker, generator, and per-board templates this phase extends.

**Subsumes.** Phase 116 (configuration redesign, archived). See *Why
configuration lives here* below.

## Background

Phase 126's "Deliberate deferrals" enumerated nine items (the original
A–I) kept out of the MVP to keep the first end-to-end slice tractable;
the configuration work folded in from Phase 116 adds five more (J–N).
The MVP is complete and archived; everything here is a natural next
increment on a working pipeline. Items keep their stable `172.<letter>`
IDs but are now clustered into work groups by area, not by origin.

## Why configuration lives here (subsumes Phase 116)

Phase 116 set out to "redesign configuration" as a standalone concern.
Investigation showed it is not standalone: **configuration is the input
contract of this orchestration pipeline.** The config files are exactly
what the planner consumes; redesigning them in isolation would compete
with — and duplicate — the Phase 126 model that already ships.

The pipeline and its config inputs:

```
  package.xml        identity + msg <depend> + <export>build_type (colcon dispatch)
  component nros.toml reusable: linkage, metadata, default ns/params/remaps
  system nros.toml    deployment: target{triple,board,rmw,network,transport},
                        components, overlays(per-instance), scheduling(RT), build
  launch files (opt)  node graph / topology
        │
        ├─ MODE 1 DIRECT   one node, hand-written main(), reads its nros.toml
        │                  subset via Config::from_toml (include_str! on embedded).
        │                  Replaces config.toml. Keeps copy-out-template examples.
        └─ MODE 2 PLANNED  nros plan → nros-plan.json → nros build → generated
                           main() → ONE binary, all nodes wired at compile time.
```

**One schema (the Phase 126 component/system `nros.toml`), two modes.**
A trivial single-node app reads a subset directly (no launch, no
planner, no generated `main`); multi-node systems go through the
planner. `package.xml` owns identity + msg deps in both modes;
`nros.toml` owns all nano-ros config.

What 116 wanted, mapped to this model:

| 116 concern | State in the 126 model |
|---|---|
| RMW selection | already `system.target.rmw`; wire it everywhere (172.M) |
| per-node options | already `system.overlays` + `component.overrides` — done |
| RT / scheduling | already `SchedContextConfig`; multi-tier is 172.G |
| peripheral/network | **schema gap** — add `target.network`/`transport` (172.J) |
| `config.toml` sprawl | retire into direct-mode `nros.toml` (172.K) |
| `nros.toml` name clash | bridge (Phase 124 `run_from_config`) vs orchestration — rename bridge (172.L) |

The single-`[node]` schema and the package.xml-vs-`nros.toml` (A/B)
framing explored in the archived 116 doc are **superseded** by this
component/system model.

## Work groups (parallelization)

Four groups, each owning a largely disjoint area of the tree, so they
can be staffed and shipped in parallel:

| Group | Area | Items | Intra-group order |
|-------|------|-------|-------------------|
| **1 — Configuration & build inputs** | config files, `SystemConfig` schema, examples, colcon/`nros build` RMW wiring, `.cargo/config.toml` | L, M, J, K, N | L, M (small unblockers) → J (schema) → K (migration) → N (audit/docs) |
| **2 — Planner & scheduling** | host planner dataflow, plan-schema sched representation, generated executor wiring | B, C, G | B → C → G |
| **3 — Generated-runtime capabilities** | `nros-orchestration` runtime, generated `main`, plan representation of runtime features | A, H, I | independent (A largest) |
| **4 — Host tooling & DX** | host CLI only; no runtime/plan-schema coupling | D, E, F | independent |

**Shared contract.** Groups 1–3 all touch the `nros-plan.json` schema
(Group 1 feeds `SystemConfig` → plan inputs; Group 2's planner writes
the plan; Group 3's runtime reads it). Schema changes must be
**additive + version-bumped** and coordinated across these three.
**Group 4 is fully independent** — it only reads existing artifacts.

> **Schema log (`PLAN_VERSION`).** v1 → **v2** (Group 2, 172.B/C):
> two additive top-level arrays on `NrosPlan`, both
> `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so a
> plan with neither serializes byte-identically to v1:
> - `callback_chains: Vec<PlanCallbackChain>` (172.B) — `{ id,
>   callbacks, links: [{from,to,topic}], inferred }`.
> - `callback_groups: Vec<PlanCallbackGroup>` (172.C) — `{ id,
>   kind: CallbackGroupKind (mutually_exclusive|reentrant), callbacks,
>   inferred }`.
>
> No `PLAN_VERSION` bump for 172.G — it adds no field; `sched_contexts`
> already existed. But the planner now **consumes**
> nros.toml `[[scheduling.contexts]]` (172.G), previously parsed-and-
> ignored. **Group 1 (config owner):** that key is now live — a
> declared tier id is matched against each callback's `group`.
>
> 172.A adds one additive top-level field on `NrosPlan` (still v2):
> `lifecycle: Option<PlanLifecycle>` (`{ autostart:
> none|configure|active }`), `#[serde(default,
> skip_serializing_if = "Option::is_none")]`, read from nros.toml
> `[lifecycle]`. **Group 1 (config owner):** `[lifecycle]` is now a
> live key.
>
> Group 1/3 agents: rebase onto these. Two **pre-existing** schema
> bugs found + fixed in the same pass (HEAD's `orchestration_schema`
> round-trip was already red): the `PlanEntity` `callback: Option`
> fields and `PlanBuildOptions.transports: Vec` both serialized
> `null`/`[]` while the golden fixtures omit them — added
> `skip_serializing_if` to all five.

## Work items

### Group 1 — Configuration & build inputs

**Parallel lane.** Touches the config inputs (`package.xml` stays
identity-only; `nros.toml` schema; examples; colcon task; `nros build`;
`.cargo/config.toml`) and the direct-mode `Config::from_toml` path. Do
L + M first (small, unblock the rest), then the schema (J), then the
example migration (K), then the audit/docs (N).

- [ ] **172.L — Resolve the `nros.toml` name collision.** Two
      incompatible schemas currently share the filename: the Phase 124
      **bridge** config (`nros_bridge::run_from_config`, runtime
      `[[node]]`/`[[bridge]]` multi-RMW forwarding) and the Phase 126
      **orchestration** config (build-time component/system). Different
      lifecycles — they cannot share a schema. Orchestration keeps
      `nros.toml`; rename the bridge config to `nros-bridge.toml`
      (update `run_from_config` default, docs `book/src/reference/nros-toml.md`,
      and any callers). No example ships the bridge file today, so the
      blast radius is small.

- [ ] **172.M — Wire RMW from `system.target.rmw`.** Make
      `system.target.rmw` the single source for RMW selection across
      every build path: the colcon task (`colcon_nano_ros/task/nros/build.py`)
      currently **hardcodes `-DNANO_ROS_RMW=zenoh`** and references the
      dead `find_package(NanoRos)` (removed in Phase 140) — both must be
      fixed; `nros build` threads the Cargo feature / CMake `-D` / Zephyr
      `prj-<rmw>.conf` from `target.rmw`; direct-mode `Config::from_toml`
      reads it. Manual `cargo`/`cmake` builds keep working by passing the
      selection by hand.

- [ ] **172.J — Peripheral/network config in `SystemConfig`.** Extend
      the Phase 126.A `SystemConfig` schema with `target.network`
      (ip/mac/gateway/prefix) and `target.transport`
      (ethernet/wifi/serial + their params). Today this lives only in
      the per-example `config.toml` and is invisible to the planner.
      Scope: schema fields + `nros check` validation + consumption in
      126.D generated `main` (bake into the generated binary) and in
      direct-mode `Config::from_toml`. This is the one genuine schema
      gap from 116.

- [ ] **172.K — Retire `config.toml` into `nros.toml` (direct mode).**
      Define the single-node **direct mode**: a hand-written one-node
      app reads a `system nros.toml` subset (`target.rmw`,
      `target.network`, node namespace/params, `[node.rt]`) via
      `Config::from_toml` — no launch file, no planner, no generated
      `main`. Migrate the 88 example `config.toml` files, 86
      `include_str!("config.toml")` call sites, and the 8 board
      `Config::from_toml` parsers + 5 board `build.rs` to `nros.toml`.
      Delete `config.toml`. Preserves the copy-out-template examples
      (`boilerplate IS lesson`) — they keep their hand-written `main()`.

- [ ] **172.N — Audit `.cargo/config.toml` to dep-injection only.**
      Confirm every example `.cargo/config.toml` holds **only**
      `[patch.crates-io]` (local crate + generated msg paths) — no
      nano-ros semantic config. Move any stray config into `nros.toml`.
      Document the one-lane-per-file model
      (`Cargo.toml`/`CMakeLists.txt` = build; `package.xml` = identity +
      msg deps; `.cargo/config.toml` = patch injection; `nros.toml` =
      nano-ros config) in `book/src/user-guide/configuration.md`.

### Group 2 — Planner & scheduling

**Parallel lane.** Host-side planner dataflow analysis + the
`nros-plan.json` scheduling representation + generated executor wiring.
B infers the chains, C groups callbacks from those chains, G consumes
the grouping into multi-tier scheduling — so run B → C → G.

- [x] **172.B — Automatic callback-chain inference.** Infer
      callback execution chains (which callback feeds which) from
      the topic graph instead of requiring explicit bindings.
      Scope: dataflow analysis in the planner; emit inferred chains
      into the plan with an override escape hatch.
      *Done:* `infer_callback_chains` (planner.rs) walks
      instance publisher→subscriber dataflow, union-finds
      weakly-connected components, Kahn-topo-orders each into a
      `PlanCallbackChain`; emitted into the plan (`callback_chains`).
      `inferred: true`; an explicit `[[chain]]` override sets it
      false. 3 unit tests.

- [x] **172.C — Automatic callback-group inference.** Derive
      callback groups (mutually-exclusive vs reentrant) from the
      graph + scheduling annotations rather than hand-authored
      groups. Scope: planner heuristic + `nros-plan.json`
      representation + generated `SchedContext` binding.
      *Done:* `infer_callback_groups` derives groups from the
      172.B chains — each chain → one `mutually_exclusive` group
      (dataflow-coupled stages serialize); each chain-less callback
      → its own `reentrant` singleton group (no coupling ⇒
      concurrent-safe). `PlanCallbackGroup` + `CallbackGroupKind`
      in the plan; 3 unit tests. The generated single-threaded
      executor already serializes all callbacks, so group **kinds**
      become observable only with the 172.G multi-tier executor —
      the runtime enforcement of `reentrant` concurrency lands there.

- [x] **172.G — Multi-tier scheduling.** Extend the single-tier
      `SchedContext` model to multiple scheduling tiers (e.g. a
      high-rate RT tier + a best-effort tier within one executor).
      Depends on the Phase 110 scheduling primitives. Scope: plan
      schema for tiers + generated multi-tier executor wiring.
      *Done (config-driven):* the runtime already dispatches across
      Phase 110.C's three `Priority` buckets (FIFO/EDF by class) and
      the generated `run_executor` already creates **N** sched-contexts
      in one executor + binds callbacks — multi-tier was wired at the
      runtime + codegen layers. The gap was the **planner**, which
      hardcoded a single `best_effort` `default_executor` and bound
      every callback to it. Now `collect_sched_contexts` reads the
      nros.toml `[[scheduling.contexts]]` tiers (author-declared, not
      inferred — launch files carry no scheduling, source metadata only
      a `group`) into the plan's `sched_contexts`; each callback binds
      to the tier whose id equals its `group` (**group name = tier id**),
      falling back to `default_executor` (still emitted only when used,
      so single-tier plans stay byte-identical). The binding onto a
      declared tier carries its priority + `source: "nros.toml"`. 4
      tests (3 unit + 1 end-to-end `plan`→`check` in
      `orchestration_cli.rs`). **Tier→callback binding is by `group`
      name only**; an explicit `[[scheduling.bindings]]` table
      (decoupling group names from tier ids) is a deferred follow-up if
      that proves too rigid.

> **172.G binding source.** nros.toml `[scheduling]`
> (`config::SchedulingConfig`) was a fully-designed but **unwired**
> schema — parsed as raw `Value`, only the `[build]` block consumed.
> 172.G wires `[[scheduling.contexts]]` through `schema_plan_json`.
> `config::SchedContextConfig` already mirrors `PlanSchedContext`
> field-for-field, so a TOML context maps straight onto a plan tier
> (absent optional keys normalised to null/defaults).

### Group 3 — Generated-runtime capabilities

**Parallel lane.** Extends the `nros-orchestration` runtime + the
generated `main` + the plan's representation of runtime features. The
three are independent of each other; A (lifecycle) is the largest.

- [x] **172.A — Lifecycle node orchestration.** Model
      managed-lifecycle nodes (configure / activate / deactivate /
      cleanup transitions) in the plan schema + generated runtime.
      Today every instance is a plain node brought up once at boot.
      Scope: lifecycle state in `nros-plan.json`, transition
      callbacks in the generated runtime, `nros check` validation
      of lifecycle graphs.
      *Done (system-level, config-driven):* the REP-2002 state
      machine (`nros-core`/`nros-node` `lifecycle*.rs`) + the
      executor services (`Executor::register_lifecycle_services`,
      `lifecycle-services` feature) already exist. The plan now
      carries an optional `lifecycle: { autostart: none|configure|
      active }` block (`PlanLifecycle` / `LifecycleAutostart`),
      read from nros.toml `[lifecycle]` (`collect_lifecycle`).
      Codegen emits `apply_lifecycle(&mut executor)` — a no-op for
      unmanaged plans (no feature, byte-equivalent), else
      `register_lifecycle_services()` + the boot autostart
      transitions; `run_executor` calls it after binding callbacks,
      and a managed plan enables `nros/lifecycle-services`. `nros
      check` validates via the `NrosPlan` parse (autostart enum). 4
      tests (planner unit + plan→check e2e + managed/unmanaged
      codegen); the no-op path is compile-checked by the real-build
      e2e suite. **Scope note:** the runtime models **one** lifecycle
      SM per executor, so this is *system-level* (the generated
      binary's node is managed). **Deferred (needs new runtime
      core):** per-instance lifecycle (multiple managed nodes in one
      binary, requiring a per-node SM registry), component-provided
      transition callbacks (today's transitions take the
      default-success path), and gating callback dispatch on the
      `Active` state.

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

### Group 4 — Host tooling & DX

**Parallel lane.** Host CLI only — reads existing artifacts, touches no
runtime code or plan schema, so it is fully independent of Groups 1–3.
The three items are independent of each other.

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

## Acceptance criteria

Each work item is independently shippable. A work item is done when:

- [ ] Its capability is represented in `nros-plan.json` (where it
      affects the plan) with round-tripping fixtures.
- [ ] `nros check` validates the new construct.
- [ ] The generated runtime exercises it, verified by an
      `orchestration_e2e` fixture (or a unit test where no generated
      binary is involved).
- [ ] Docs show the workflow for the new capability.

Group 1 (configuration) additionally:

- [ ] A project carries at most `Cargo.toml` **or** `CMakeLists.txt`
      (build), `package.xml` (identity + msg deps), `.cargo/config.toml`
      (patch injection only), and one `nros.toml` (all nano-ros config).
      No `config.toml`; no `nros.toml`/bridge name clash.
- [ ] A single-node example builds + boots in **direct mode** from
      `nros.toml` (network + RMW + RT) with no launch file or generated
      `main`, on both a hosted and an embedded (`include_str!`) target.

## Notes

- Items keep their stable `172.<letter>` IDs from when A–I were Phase
  126's "Deliberate deferrals" and J–N were absorbed from Phase 116
  (archived). The groups re-cluster them by area, not by origin — e.g.
  scheduling item 172.G (originally a 126 deferral) now sits with the
  planner items 172.B/C.
- **Cross-group parallelism is the point**; pick groups by available
  hands. Group 4 (host tooling) is the most independent. Within groups,
  the lowest-risk single wins are 172.L + 172.M (Group 1) and 172.D +
  172.F (Group 4); the heaviest are 172.K (88 examples + 86
  `include_str!` + 8 board parsers, Group 1) and 172.A / 172.G (Groups
  3/2).
- Groups 1–3 share the `nros-plan.json` schema — coordinate additive,
  version-bumped changes; don't let two groups mutate the schema in the
  same window without rebasing.
