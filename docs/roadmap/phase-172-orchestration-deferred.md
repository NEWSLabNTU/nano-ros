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

- [x] **172.L — Resolve the `nros.toml` name collision.** DONE 2026-05-27 —
      bridge config renamed to `nros-bridge.toml` (`run_from_config` is
      path-agnostic; updated doc comments + the book page + SUMMARY link).
      Two
      incompatible schemas currently share the filename: the Phase 124
      **bridge** config (`nros_bridge::run_from_config`, runtime
      `[[node]]`/`[[bridge]]` multi-RMW forwarding) and the Phase 126
      **orchestration** config (build-time component/system). Different
      lifecycles — they cannot share a schema. Orchestration keeps
      `nros.toml`; rename the bridge config to `nros-bridge.toml`
      (update `run_from_config` default, docs `book/src/reference/nros-toml.md`,
      and any callers). No example ships the bridge file today, so the
      blast radius is small.

- [x] **172.M — Wire RMW from `system.target.rmw`.** DONE 2026-05-27 — the
      orchestration generator already threads `build.rmw`; the colcon task's
      hardcoded `zenoh` + dead `find_package(NanoRos)` were fixed (RMW from
      `NANO_ROS_RMW` env via `resolve_rmw()`; platform from the parsed token;
      zephyr `prj-<rmw>.conf` overlay).
      Make
      `system.target.rmw` the single source for RMW selection across
      every build path: the colcon task (`colcon_nano_ros/task/nros/build.py`)
      currently **hardcodes `-DNANO_ROS_RMW=zenoh`** and references the
      dead `find_package(NanoRos)` (removed in Phase 140) — both must be
      fixed; `nros build` threads the Cargo feature / CMake `-D` / Zephyr
      `prj-<rmw>.conf` from `target.rmw`; direct-mode `Config::from_toml`
      reads it. Manual `cargo`/`cmake` builds keep working by passing the
      selection by hand.

- [x] **172.J — Peripheral/network config in `SystemConfig`.** DONE 2026-05-27 —
      Phase 173.5 already parses `[[transport]]` (kind/ip/device/baudrate/rmw/
      locator); added the remaining `config.toml [network]` fields `mac` +
      `gateway` to `PlanTransport` (+ `BoardTransportConfig::{set_mac,set_gateway}`
      + generator emission + validate). (172.K.4/K.7 added wifi `ssid`/`password`
      + the `interfaces` multi-homing list on top.)
      Original scope: extend
      the Phase 126.A `SystemConfig` schema with `target.network`
      (ip/mac/gateway/prefix) and `target.transport`
      (ethernet/wifi/serial + their params). Today this lives only in
      the per-example `config.toml` and is invisible to the planner.
      Scope: schema fields + `nros check` validation + consumption in
      126.D generated `main` (bake into the generated binary) and in
      direct-mode `Config::from_toml`. This is the one genuine schema
      gap from 116.

- **172.K — Retire `config.toml` into `nros.toml` (direct mode).** Define
      the single-node **direct mode**: a hand-written one-node app reads its
      `nros.toml` via `Config::from_toml` (compile-baked with `include_str!`
      on embedded, fs/env on hosted) — no launch file, no planner, no
      generated `main`; the copy-out-template examples keep their hand-written
      `main()`. Schema = `[node]` (domain/namespace) + top-level
      `[[transport]]` (id-addressable session: kind/ip-CIDR/mac/gateway/rmw/
      locator/device/baudrate/ssid/password/interface) + `[node.rt]`
      (scheduling). Nodes bind to transports by `id` (0/1 implicit, N explicit).
      **Approved design: [`docs/design/configuration-and-transports.md`](../design/configuration-and-transports.md).**
      Migrate the 88 example `config.toml`, 86 `include_str!("config.toml")`
      sites, the 8 board `Config::from_toml` parsers, and the 5 board
      `build.rs`; then delete `config.toml`. Staged sub-items:

  - [x] **172.K.1 — direct-mode parser support (additive) + pilot.** Board
        `Config::from_toml` parses the new `[[transport]]`/`[node]`/`[node.rt]`
        shape **alongside** the legacy `[network]`/`[zenoh]`/`[scheduling]`
        (section parser handles `[[...]]` array-of-tables + dotted sections),
        so boards + examples migrate independently with no flag day. Pilot:
        `nros-board-mps2-an385` + the qemu-arm-baremetal rust talker → `nros.toml`,
        `cargo check` (thumbv7m) green. (`38d342a89`.)
  - [x] **172.K.2 — roll out the 7 remaining board `from_toml` parsers.**
        Done (`96120466d`): freertos (+`[node.rt]` scheduling, CIDR→netmask),
        threadx-linux (+`interface`), threadx-riscv64 (CIDR→netmask), esp32
        (+wifi via `IpMode`), esp32-qemu, stm32f4 (+`usart_index`),
        nuttx-qemu-arm (no MAC) — all additive alongside the legacy arms.
        freertos + threadx-linux compile-verified via their examples; the
        prefix/serial boards mirror the verified mps2 pilot (compile-checked in
        K.3 per-platform builds). The 5 board `build.rs` bakers move with their
        examples in K.3.
  - [x] **172.K.3 — migrate the 88 example `config.toml` → `nros.toml`.** DONE
        2026-05-27. **Rust** (40) — `include_str!` switched; board `from_toml`
        parses the shape. **C/C++** (47, freertos/nuttx/threadx-{linux,riscv64}
        × c+cpp) consume config via the CMake `nano_ros_read_config` →
        `NROS_APP_CONFIG` path, so **both** parser copies
        (`cmake/NanoRosConfig.cmake` + `packages/core/nros-c/cmake/NanoRosReadConfig.cmake`)
        were taught the `[node]`/`[[transport]]`/`[node.rt]` shape (additive) and
        each CMakeLists `nano_ros_read_config` path repointed. 0 source
        `config.toml` remain. Verified: representative Rust cargo-checks (mps2/
        freertos/threadx-linux) + both CMake parsers emit correct
        `NROS_APP_CONFIG` from a converted file. Full per-platform cross-build
        rides the K.6 `build-all`. (Board `build.rs` needed no change — they read
        `.cargo/config.toml` only.)
  - [x] **172.K.4 — planned-mode parity (submodule `colcon-nano-ros`).** DONE
        2026-05-27 (`ea695e3` on colcon-nano-ros main; superproject pointer
        bumped). `PlanTransport` gained `id` + wifi `ssid`/`password`;
        `TransportKind::Wifi` (+`cargo_feature "wifi"`); `validate_transports`
        Wifi kind + ssid/password=wifi-only; generator emits
        `c.set_ssid`/`c.set_password` in `apply_transport_config` (matching the
        new no-op-default `BoardTransportConfig::{set_ssid,set_password}`
        superproject setters); `SystemComponent` gained `transport: Option<String>`
        carrying the per-instance bind through system config → plan. Additive +
        serde-default (existing plans round-trip). 47 lib + all integration tests
        green. The full `SESSION_SPECS`-by-id wiring is the K.5 runtime step;
        K.4 lands the schema + generator + the binding field.
  - [ ] **172.K.5 — runtime `create_node_on`-by-id.** Bind a node to a session
        by transport `id` (not just `rmw`); only required for **case D**
        (segregated same-rmw sessions) in the transport taxonomy — deferrable
        until such a build exists.
  - [ ] **172.K.7 — multi-homing `[[transport]].interfaces` (list).** A single
        session spanning several NICs as one merged graph (taxonomy cases B/C —
        the common "node reachable on multiple interfaces" need, what stock
        DDS/zenoh do natively). Generalize the current single `interface` field
        to a list; generator maps it per backend (zenoh listen/connect per NIC +
        scouting iface; Cyclone `<Interfaces>`; Fast DDS whitelist). Distinct
        from K.5 (merge vs segregate). Design:
        [`docs/design/configuration-and-transports.md`](../design/configuration-and-transports.md)
        ("Two axes" taxonomy).
  - [x] **172.K.6 — drop the legacy arms + delete `config.toml`.** DONE
        2026-05-27. All 88 examples + 2 nros-bench fixtures on `nros.toml`
        (0 source `config.toml` repo-wide); legacy `[network]`/`[zenoh]`/
        `[scheduling]`/`[platform]`/`[wifi]`/`[serial]` arms removed from all 8
        board `from_toml` parsers + both CMake parsers (`NanoRosConfig.cmake`,
        `nros-c/NanoRosReadConfig.cmake`) — parsers accept only the direct-mode
        `[node]`/`[[transport]]`/`[node.rt]` shape. Last runtime consumers
        migrated first (3 logging-smoke bins; `nros new` scaffolder → `nros.toml`,
        colcon-nano-ros `d37a692`). **Verified: `build-all` green across every
        platform** (board drops + the CMake C/C++ path); both CMake parsers
        parser-driven. Also fixed along the way: zephyr cyclonedds graph-types
        build (177.36, landed on main `4c6ce2520`) + the converter `#`-in-serial-
        locator bug + the rust-example CMakeLists `nano_ros_read_config` repoint.

- [x] **172.N — Audit `.cargo/config.toml` to dep-injection only.** DONE
      2026-05-27. **Audit PASS:** every example `.cargo/config.toml` holds only
      legit cargo sections (`[patch.crates-io]` dep-injection + `[build]`/
      `[target]`/`[env]`/`[unstable]` cargo knobs) — zero nano-ros semantic
      config (locator/domain/ip/mac) leaked (the one grep hit was a comment).
      Rewrote `book/src/user-guide/configuration.md` around the one-lane-per-file
      model (file-ownership table + the `nros.toml` `[node]`/`[[transport]]`/
      `[node.rt]` shape + direct-vs-planned read modes + link-vs-active RMW).
      **Follow-up (separate doc sweep, not 172):** ~10 getting-started/reference
      book pages still show the retired per-example `config.toml` in tutorials
      (`first-node-rust`, `freertos`, `bare-metal`, `cli`, …) — update to
      `nros.toml` for new-user correctness.

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

- [x] **172.D — Incremental / staleness-aware build.** Skip
      regeneration + recompilation when the plan + sources are
      unchanged. Today `nros build` regenerates the package every
      run. Scope: content-hash the plan + component metadata; gate
      `generate_package` + the cargo invocation on staleness.
      **Landed** — `build_generated_package`
      (`packages/nros-cli-core/src/orchestration/build.rs`) now
      fingerprints the *generation* inputs (generator version + plan
      bytes + the paths baked into the manifest/build-script:
      `package_name`, `workspace_root`, `component_workspace`) with a
      `DefaultHasher` digest, records it in a `.nros-build-stamp` under
      the generated package root after a clean generation, and skips
      `generate_package` entirely when the stamp matches and the crate
      is present (printing "generated package up to date … skipping
      regeneration"). `nros build --force` / `NROS_BUILD_FORCE=1`
      bypasses the gate. **Recompilation is owned by cargo, not
      re-implemented:** the generated crate path-depends on the
      component crates, so cargo's own incremental fingerprinting is
      the authority on component-source staleness — `nros build`
      always invokes cargo (a no-op in ~0.06 s when nothing changed)
      rather than gate it on the plan hash, which would ship a stale
      binary whenever component source changed under an unchanged
      plan. The generator version is in the fingerprint so a CLI
      upgrade re-generates even on a byte-identical plan. Verified:
      unit tests for the fingerprint's input-sensitivity + the
      freshness predicate; the `orchestration_e2e` build test asserts
      the stamp is written; a real rebuild prints the skip line + cargo
      no-ops, and `--force` regenerates.

- [ ] **172.E — Hardened metadata-mode sandboxing.** The
      `nros metadata` mode compiles + runs component code to
      extract source metadata. Harden that execution (resource
      limits, filesystem/network restrictions) so untrusted
      component crates can't escape during metadata extraction.

- [x] **172.F — Polished `nros explain`.** A user-facing command
      that explains the generated plan: which launch node maps to
      which component, how params resolved, why a SchedContext was
      chosen, what each generated table contains. Scope: a
      readable, structured rendering of `nros-plan.json` + the
      generation trace. **Landed** — `nros explain [plan]`
      (`packages/nros-cli-core/src/cmd/explain.rs`, default
      `build/nros/nros-plan.json`). Read-only: deserializes the same
      `NrosPlan` schema `nros check` validates, touches no runtime
      code or schema. Renders, in order: system header + generation
      trace (`generated by` / `system config` / `launch record`),
      build target, components, instances (launch-instance→component
      map → nodes → endpoints with interface + QoS
      reliability/durability/history(depth) → resolved parameters with
      `value [source-kind @ artifact]` → callback→context sched
      bindings + remaps), the SchedContext table (class / prio /
      period / budget / deadline(policy) / core / task), transports
      (bridge mode), and lifecycle / callback-chain / callback-group
      summaries when present. `render<W: Write>` is split out so the
      `orchestration_cli` fixture captures and asserts the rendering
      off the real metadata→plan artifact.

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
