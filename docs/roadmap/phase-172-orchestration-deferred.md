# Phase 172 — Orchestration follow-ups (deferred from Phase 126)

**Goal.** Land the capabilities that Phase 126 (ROS 2 workflow
orchestration MVP) explicitly deferred, **plus the configuration
consolidation absorbed from Phase 116** (archived 2026-05-27). Phase
126 shipped the end-to-end MVP — source metadata → launch plan →
checked `nros-plan.json` → generated per-board binary, verified across
9 boards — and listed nine deferrals. This phase tracks those (A–I) and
the config work folded in from 116 (J–N) as discrete,
independently-schedulable items.

**Status.** Not Started.

**Priority.** P2 — none block the MVP workflow; each is an
ergonomic or capability upgrade on top of a working pipeline.

**Depends on.** Phase 126 (archived) — the schema, planner,
checker, generator, and per-board templates this phase extends.

**Subsumes.** Phase 116 (configuration redesign, archived). See *Why
configuration lives here* below.

## Background

Phase 126's "Deliberate deferrals" section enumerated nine items
kept out of the MVP to keep the first end-to-end slice tractable.
The MVP is now complete and archived; these are the natural next
increments (A–I), listed here verbatim from the 126 doc, each
expanded into a work item with a sketch of scope.

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

## Work items

### A–I — Phase 126 deferrals

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

### J–N — Configuration consolidation (subsumed from Phase 116)

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

- [ ] **172.N — Audit `.cargo/config.toml` to dep-injection only.**
      Confirm every example `.cargo/config.toml` holds **only**
      `[patch.crates-io]` (local crate + generated msg paths) — no
      nano-ros semantic config. Move any stray config into `nros.toml`.
      Document the one-lane-per-file model
      (`Cargo.toml`/`CMakeLists.txt` = build; `package.xml` = identity +
      msg deps; `.cargo/config.toml` = patch injection; `nros.toml` =
      nano-ros config) in `book/src/user-guide/configuration.md`.

## Acceptance criteria

Each work item is independently shippable. A work item is done when:

- [ ] Its capability is represented in `nros-plan.json` (where it
      affects the plan) with round-tripping fixtures.
- [ ] `nros check` validates the new construct.
- [ ] The generated runtime exercises it, verified by an
      `orchestration_e2e` fixture (or a unit test where no generated
      binary is involved).
- [ ] Docs show the workflow for the new capability.

Config items (J–N) additionally:

- [ ] After J–N, a project carries at most `Cargo.toml` **or**
      `CMakeLists.txt` (build), `package.xml` (identity + msg deps),
      `.cargo/config.toml` (patch injection only), and one `nros.toml`
      (all nano-ros config). No `config.toml`; no `nros.toml`/bridge
      name clash.
- [ ] A single-node example builds + boots in **direct mode** from
      `nros.toml` (network + RMW + RT) with no launch file or generated
      `main`, on both a hosted and an embedded (`include_str!`) target.

## Notes

- A–I were listed under Phase 126's "Deliberate deferrals"; splitting
  them into their own phase keeps 126 archivable as a clean MVP while
  preserving the backlog. J–N are the configuration work absorbed from
  Phase 116 (archived) — see *Why configuration lives here* above.
- No ordering is forced between work items; pick by user demand.
  172.D (incremental build) and 172.F (`nros explain`) are the
  lowest-risk ergonomic wins; 172.A (lifecycle) and 172.G
  (multi-tier scheduling) are the largest. Among config items, 172.L
  (name-collision rename) and 172.M (RMW wiring) are small and unblock
  the rest; 172.J (network schema) + 172.K (`config.toml` retirement)
  are the bulk (88 examples + 86 `include_str!` + 8 board parsers).
