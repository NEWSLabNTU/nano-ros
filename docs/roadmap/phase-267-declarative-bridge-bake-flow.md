# Phase 267 — Declarative cross-RMW bridge: complete the bake→entry→build flow

Status: **FORWARDING GREEN (2026-06-28)** — W0 done; W1 done (investigation — the
live entry emitter, not the bake record, is the gap); **design DECIDED (W1c,
2026-06-27): config-driven runtime bridge** — user writes names-only `[[bridge]]`
+ plain `nros::main!`; `nros sync` resolves type+hash; macro bakes + drives the
runtime `PubSubBridge`. No build.rs, no codegen-relay dup. (W1b route-(a) codegen
+ S1 superseded.) **C1–C5 DONE (2026-06-27); C6 + W-B DONE (2026-06-28) — the
declarative zenoh↔cyclonedds bridge FORWARDS end-to-end.** Clean flow (no
build.rs, no user bridge code): talker declares `publishes` → `nros sync`
resolves /chatter→type **+ flat field schema** → `nros-bridge.toml` → plain
`nros::main!` emits `run_from_config_str(include_str!)` → `cargo build` links
cyclone+zenoh → runtime stages the Cyclone descriptor + pins per-session domains
→ a stock `rmw_cyclonedds_cpp` subscriber receives the forwarded `std_msgs/Int32`.
**C6 runtime gaps all FIXED:** backend force-link (issue 0106, `extern crate as _`)
+ DDS type-mangling (`interface_type_name`) + descriptor staging via config field
schema (**fix B**, issue 0107, W-B1/W-B2) + per-session domain plumbing (issue
0109). ·
Implements
[RFC-0009](../design/0009-bridge-topic-forwarding.md) (bridge topic-forwarding) ·
Resolves [issue 0099](../issues/0099-declarative-bridge-planner-population.md) ·
Completes [phase-263](phase-263-complete-workspace-examples.md) Track B / B3
(`ws-bridge-rust`).

> **Headline (2026-06-26, W1 investigation).** The data path is fine — the live
> `nros::main!` build (`nros-build` → `plan_system`) already produces an
> `NrosPlan` with `transports` + `bridges` (W0). The blocker is the **live entry
> emitter** `nros-build/emit.rs::emit_run_plan`: it renders a single-session
> `RuntimeCtx` register-dispatch and ignores `plan.bridges`. RFC-0009's
> Executor-based bridge relay lives only in `generate.rs`, reachable only through
> `build_generated_package`, which has **zero non-test callers** — a dead path.
> The phase's heart is wiring a live bridge entry shape (see W1).

> **Origin.** phase-263 B3 set out to ship `examples/workspaces/ws-bridge-rust` —
> a declarative cross-RMW gateway (`[[bridge]]` in `system.toml`) forwarding
> `/chatter` zenoh→cyclonedds in one process. A deep dive found the *config →
> plan → relay-codegen → runtime* pipeline is code-complete + unit-tested
> (RFC-0009), but a **cascade** of orchestration gaps sits between a declared
> `[[bridge]]` and a built, forwarding binary. The genuinely-blocking, reusable
> engine work landed (W0); the cascade is sized into the waves below. B3 is
> phase-sized, hence this doc.

## Why a separate phase

A declarative bridge is not "one more workspace example": it is the **first**
system that needs the *baked* orchestration entry on a pure-Rust workspace (every
other Track-B workspace uses the `nros::main!` macro, which emits no bridge
relay), and it exercises plan emitters + metadata collection + a second RMW
backend that the macro path never touches. Each gap below is independently real
and independently testable.

## W0 — Engine foundations. **DONE (2026-06-25).**

The reusable, RMW-agnostic core — benefits any bridge pair, not just cyclone.

- **Planner transform** (`4b5f6d8ff`, issue #99 step 0). `planner.rs` now emits,
  for a `[[bridge]]` system: `build.transports` (one `PlanTransport{rmw,domain,
  locator}` per distinct endpoint → `is_bridge()` true, `SESSION_SPECS` /
  `open_multi` light up) + `plan.bridges` (one `PlanBridge{name,connect,topics}`,
  endpoints byte-matching the transports so `bridge_endpoint_session_idx`
  resolves). Shared `resolve_bridge_endpoint` parses `"<rmw>:<domain>"` /
  bare-domain selectors; locator = `[system].locator` for the system-rmw endpoint,
  none for the DDS/multicast peer. 3 unit tests; validated end-to-end (`nros plan`
  on `ws-bridge-rust` → correct transports + bridges).
- **Cyclone as a first-class native Rust backend** (`bdc05d974`). The generated
  native Rust entry now links + registers `nros-rmw-cyclonedds-sys`
  (`render_one_backend` + `render_backend_register_fn`), gated on `board ∈
  {native, posix}` so non-native / CMake-corrosion builds stay byte-identical (no
  double-link; C/cpp workspaces don't use this generated-entry path). Test
  `cyclone_backend_dep_gated_on_native_board`. Found: `std_msgs/Int32` +
  `rmw_dds_common_graph` descriptors are baked into `nros-rmw-cyclonedds-sys/
  build.rs` by default, so the demo type needs no descriptor staging.
- **Workspace skeleton** (`088aea8e8`). `examples/workspaces/ws-bridge-rust`:
  `talker_pkg` (builds) + `demo_bringup/system.toml` (`[[domain]]`×2 + `[[bridge]]
  gw`). `nros plan` on it emits a correct bridge plan. README marks WIP.

## W1 — Carry the bridge plan through the bake emitter

**DONE as an investigation (2026-06-26) — the premise was wrong; it rewrote the
remaining waves.** The original W1 ("the bake's thin `render_plan_json` doesn't
carry bridges") assumed the bake plan feeds the entry build. Tracing the ACTUAL
flow proved otherwise — three findings, code-cited:

1. **The live entry build never reads the bake's thin record.** The native-Rust
   entry is the `nros::main!` proc-macro, whose `build.rs` helper is the
   `nros-build` crate (`packages/cli/nros-build/src/lib.rs:28`): it calls
   `planner::plan_system` to produce a **full `NrosPlan`** and emits from THAT.
   `cmd/codegen_system.rs::render_plan_json` (the `PlanComponent` thin record) is
   a host-side artifact for `nros check`/`explain` + the C `system_config.h` — NOT
   the entry-gen plan. So fixing the thin record is moot for the build.
2. **The live plan ALREADY carries bridges.** `plan_system` calls
   `schema_plan_json` (the W0 transform), so the `NrosPlan` `nros-build` consumes
   has `build.transports` + `plan.bridges` populated for a `[[bridge]]` system.
   The DATA is there in the live path.
3. **The live EMITTER ignores bridges; the bridge relay is stranded in a dead
   path.** `nros-build/src/emit.rs::emit_run_plan` (the live native-Rust emitter)
   renders `run_plan(runtime: &mut ::nros_platform::RuntimeCtx)` as a flat
   sequence of `<pkg>::register(runtime)` calls — it never reads `plan.bridges`,
   has no `Executor`, no `open_multi`, no `register_bridges`. The Executor-based
   bridge relay (`build_executor_bridge` / `render_register_bridges_fn` /
   `SESSION_SPECS`) lives ONLY in `orchestration/generate.rs`, reachable only via
   `orchestration/build.rs::build_generated_package` — which has **zero non-test
   callers** repo-wide (incl. `cargo-nano-ros`, `colcon-nano-ros`). It is
   dead/test-only; RFC-0009's relay codegen was never wired into the live entry
   path.

**Corrected gap (the real W1):** the live `nros-build` entry emitter does not
emit a bridge relay, and its `RuntimeCtx`-register entry shape cannot host one (a
bridge needs a multi-session `Executor` via `open_multi`, not the single-session
`RuntimeCtx`). The Executor-based relay exists but is unreachable.

**Work (now the heart of the phase):** make the live native-Rust entry path emit
the bridge relay for `is_bridge()` plans. Two routes (a design decision for the
next wave):
- **(a) Teach `nros-build`/`emit.rs` a bridge entry shape** — when `plan.bridges`
  is non-empty, emit an `Executor`-based multi-session entry
  (`Executor::open_multi(SESSION_SPECS)` + `register_backends` + the
  generic-sub→pub relay with `nros-bridge` origin codec), porting the proven logic
  from `generate.rs`. The `nros::main!` macro routes bridge systems to this shape.
- **(b) Revive `generate.rs`/`build_generated_package`** as the bridge entry
  builder and wire a live caller (a `nros` subcommand or a `cargo-nano-ros` branch)
  for native-Rust bridge workspaces.

Route (a) keeps one live emitter (`nros-build`) and is preferred; route (b)
resurrects a parallel emitter. Either way the unit-tested `generate.rs` relay +
`nros-bridge` codec are the reference implementation.

**Acceptance:** a `[[bridge]]` native-Rust workspace's generated/baked entry
contains `Executor::open_multi` + the per-topic generic-sub→pub relay with
`bridge_origin` echo suppression — built from the live path, not the dead one.

> **Re-sequence.** Old W1 (bake thin-record) is dropped — not consumed. Old W2
> (metadata→topics) and W3 (build lane) stand. The new heart is the emitter route
> above (was implicit in old W3); W4 (descriptors) + W5 (runtime) unchanged.

## W1b — Live bridge entry emitter. **ROUTE (a) CHOSEN (2026-06-26).**

Decided after a UX + maintainability comparison: **(a) teach the live path a
bridge entry shape**, not (b) revive the dead `generate.rs`/`build_generated_package`.

Rationale: (a) keeps bridges building via `nros::main!` + `cargo build` like every
other workspace (one build path, one mental model) and leaves ONE live entry
emitter — the proven relay (~150 lines: `build_executor_bridge` +
`render_register_bridges_fn` (59) + `SESSION_SPECS`) ports over and the stranded
`generate.rs` copy gets deleted. (b) would entrench two parallel entry emitters
(reversing the consolidation that made `generate.rs` dead) and add a bridge-only
build workflow. (a)'s cost is cheap because the macro already opens an `Executor`
(`main_macro.rs:1094`).

**Constraints found while mapping (a) — it spans macro + board-entry + deps, not
one emitter:**
- The `nros::main!` macro is **bridge-blind**: it resolves only the launch node
  set (`register_calls` from `pkg_idents`, `main_macro.rs:679`) — it never reads
  `system.toml`'s `[[bridge]]`/`[[domain]]`, so it has no `plan.bridges`. The full
  `NrosPlan` (with bridges) lives in the `nros-build` build.rs helper
  (`emit.rs::emit_run_plan`, which DOES take `plan: &NrosPlan`).
- The native (`Framework::OwnedSpin`) entry registers the board's **single** rmw
  (`main_macro.rs:948-954` — the board's `run()` opens a single-session `Executor`
  + calls its one `nros_rmw_<x>::register()`). A bridge needs `Executor::open_multi`
  + BOTH backends registered — the board run path doesn't expose that.

**Sub-steps:**
- **S1 — carrier. DONE (2026-06-26, `5c98511dc`).** `generate.rs` exposes
  `pub fn render_bridge_entry_fns(plan) -> Option<String>` (the single source of
  truth: `SESSION_SPECS` + `register_backends` + `build_executor_bridge` +
  `register_bridges` relay; `None` for non-bridge), reusing the existing private
  relay fns; `nros-build/emit.rs::emit_bridge_entry_fns` delegates to it. The live
  emitter (which already holds the full `NrosPlan`) can now produce the bridge
  entry — reachable but unused until S2 splices it. Unit-tested; cli-core suite
  green (395), no non-bridge behaviour change.
- **S2 — multi-session entry. BLOCKED (2026-06-26) — the macro cannot host a
  bridge; route (a)'s "`nros::main!` for bridges" UX is infeasible.** Findings:
  - `nros-macros` deliberately does NOT dep `nros-build`/`nros-cli-core` (issue
    0083 — it pulled the whole planner/codegen and bloated the proc-macro); it
    deps only `toml` + `nros-launch-parser` + `nros-pkg-index`. So the macro
    cannot call `render_bridge_entry_fns` (the relay codegen).
  - The macro builds `register_calls` from `pkg_idents` (launch node pkgs) and
    never resolves per-node **entities/topics** — the data a bridge relay needs.
    The full `NrosPlan` (entities, topics, bridges) lives only in the build.rs /
    `nros-build` path.
  - The native (OwnedSpin) entry is macro-emitted INLINE, not via `nros-build`.
    So the S1 carrier (`nros-build`) feeds the *framework* (RTIC/Embassy) build.rs
    path, not the native macro path.

  **Conclusion:** a bridge entry must use the build.rs + `include!` shape (where
  the full plan lives), reusing the S1 carrier — `nros::main!` bridges are not
  supported without re-bloating the macro (reversing 0083). This contradicts the
  UX rationale that picked route (a) over (b): bridges get a different, build.rs-
  shaped Entry regardless. **Decision needed** (the route (a)/(b) tradeoff shifts):
  - **(B1) build.rs bridge Entry, reuse S1** — the bridge Entry pkg has a
    `build.rs` that calls `nros-build` to emit `build_executor_bridge` +
    `register_bridges` to `OUT_DIR`; `main.rs` `include!`s it + drives
    `open_multi`. Architecturally correct (plan lives in build.rs); reuses S1; a
    distinct (non-`nros::main!`) Entry shape for bridges.
  - **(B2) re-bloat the macro** — re-add a (heavy) plan-resolution + codegen dep
    to `nros-macros` so `nros::main!` can emit bridges inline. Reverses 0083;
    bloats every entry's compile.
  Recommend **(B1)** — keeps the macro lean, reuses S1, plan lives where the data
  is. The original `Executor::open_multi` work below proceeds under (B1).
- **S3 — relay.** Port `build_executor_bridge` + `render_register_bridges_fn`
  (generic sub→pub per `(topic, ordered endpoint pair)` + `nros-bridge`
  `bridge_origin` echo codec) into the live emitter; emit only for `is_bridge()`.
- **S4 — deps + backend register.** The bridge Entry pkg deps both backends
  (board zenoh + `nros-rmw-cyclonedds-sys`) and registers both before
  `open_multi` (the W0 `render_one_backend`/`render_backend_register_fn` cyclone
  wiring is the reference; it must reach the LIVE emitter, not the dead one).
- **S5 — delete the dead path.** Remove `build_generated_package` + the stranded
  `generate.rs` bridge relay once the live path emits it — single source of truth.
- **S6 — build + gated runtime test** (folds into W3/W5).

**Acceptance:** `cargo build` of the `ws-bridge-rust` `native_entry` (plain
`nros::main!`) links both backends and its generated entry contains
`Executor::open_multi` + the relay; bridges build with no special workflow.

> **Note.** W1b is the phase's heart and a multi-component effort (proc-macro +
> board entry + runtime + deps). W2 (metadata→topics) feeds S3's topic list; do W2
> alongside S1–S3.

## W1c — DECIDED DESIGN (2026-06-27): config-driven runtime bridge

**Supersedes W1b's codegen-relay route (and S1's `render_bridge_entry_fns`
carrier — now unused).** S2 proved `nros::main!` cannot host a codegen relay
(macro is dep-lean per 0083 + has no entity resolution). The user's constraint —
**no user-side build.rs** — plus an explorer pass settled a cleaner shape: drive
the EXISTING runtime `PubSubBridge` from config the macro bakes. The relay logic
already lives in the runtime (`nros-bridge::PubSubBridge::new(sub, pubr, origin)`
+ `pump()`), so the macro emits only DATA + a runner call — no codegen relay, no
heavy macro deps, plain `nros::main!`.

**User-facing surface (locked):**

```toml
# system.toml — the user writes ONLY this (+ a plain `nros::main!` entry)
[[domain]]
name = "zen"; rmw = "zenoh"; id = 0
[[domain]]
name = "dds"; rmw = "cyclonedds"; id = 5

[[bridge]]
name = "gw"
from = "zenoh:zen"
to   = "cyclonedds:dds"
topics = ["/chatter"]      # names only; nros sync resolves type+hash
# bidirectional = false    # default one-way from→to; opt-in reverse relay
```
```rust
nros::main!(launch = "demo_bringup");   // unchanged; macro bakes the bridge
```

**Decisions (AskUserQuestion, 2026-06-27):** (1) topics are **names only** —
`nros sync` resolves each topic→type+hash from the publishing component's
metadata (the planner has it); the user never writes the opaque RIHS hash. (2)
direction defaults **one-way `from→to`**, `bidirectional = true` opts into the
reverse relay (echo-suppressed via `bridge_origin`).

**Data flow:** user `[[bridge]]` (names) → `nros sync` resolves sessions
(`[[domain]]`+`[system]` → rmw/locator/domain) + topics (name → type_name +
type_hash via component metadata) → writes a RESOLVED bridge spec sidecar the
macro reads → `nros::main!` bakes `const BRIDGES: &[BridgeSpec]` + emits
`Executor::open_multi(SESSION_SPECS)` + a spin-loop call to a runtime bridge
runner → `nros::bridge` constructs one `PubSubBridge` per (topic, direction) and
pumps them. `nros sync` is the resolver (the existing user step — NOT a build.rs).

**Re-sequenced waves (replacing S1–S6):**
- **C1 — schema. DONE (2026-06-27, `e6865d718`).** `SystemBridgeEntry` gains
  `topics: Vec<String>` (names) + `bidirectional: bool` (serde-default,
  back-compat); the planner honours explicit topics (over forward-all) + flows
  `bidirectional` into `PlanBridge`. Tested. The RESOLVED `BridgeSpec` type (the
  sync↔macro sidecar contract) is deferred to C3, where its exact macro-read
  format is defined alongside the sync writer.
- **C2 — runtime runner. DONE — already existed (`8358836f1`).**
  `nros_bridge::run_from_config(path)` (`packages/bridge/nros-bridge/src/config.rs`)
  reads a `nros-bridge.toml` (`[[node]]` rmw/locator/domain + `[[bridge]]`
  type/from/to), opens `open_multi`, creates the nodes, builds a `PubSubBridge`
  per `[[bridge]]` (a `PumpableBridge` trait erases the const-generic buffers),
  and spins+pumps forever. The runtime relay is reused, not rebuilt.
- **C3 — resolver core DONE (`8358836f1`); WIRING BLOCKED (2026-06-27).**
  `generate.rs render_bridge_runtime_config(plan) -> Option<String>` renders the
  `nros-bridge.toml` from a `[[bridge]]` plan (`[[node]]` per session,
  `[[bridge]]` per (topic, direction), ROS type resolved via
  `resolve_topic_interface`, empty `type_hash`). Unit-tested.
  **Blocker — "names only" can't resolve type pre-build:** `resolve_topic_interface`
  needs the topic→type mapping from the plan's `interfaces`, which come from
  component **entity metadata**. That metadata exists only after building the node
  pkgs (sidecar JSON) OR from a manifest `[topics]` declaration
  (`manifest.rs::collect_topics`) — NEITHER is available at `nros sync` time for a
  plain workspace, and the user rejected a build.rs (where a post-build resolve
  would live). So names-only + no-build.rs + macro-can't-resolve collide.
  **Decision (2026-06-27, AskUserQuestion): keep names-only via NODE ENTITY
  METADATA.** Each node pkg declares its published/subscribed topics+types in its
  Cargo `[package.metadata.nros.node]`; the planner reads them as SYNTHETIC
  metadata (pre-build, no sidecar) → `plan.interfaces` → `resolve_topic_interface`
  resolves the bridge's names. Keeps the clean names-only bridge config; cost is a
  node-authoring requirement + a metadata-pipeline addition. **Concrete sub-steps
  (a deep, intricate change — its own wave):**
  - **C3a — schema.** Add to `ComponentMetadata` (`cargo_metadata_schema.rs`):
    `publishes: Vec<TopicDecl>` + `subscribes: Vec<TopicDecl>` (`{ topic, type }`).
    (Services/actions later.) Carry to `CargoComponentSummary`.
  - **C3b — synthetic emission.** `summary_to_synthetic_json` (`workspace.rs`)
    must emit an `entities` array in the EXACT shape the planner's `schema_entity`
    consumes (`{ kind: "publisher"|"subscriber", resolved_name: <topic>, type:
    <interface{package,name}> }`), nested where `schema_instance` reads it
    (`nodes[].entities` / instance `entities`). This is the intricate part —
    match the post-build sidecar entity format so synthetic == sidecar.
  - **C3c — verify flow.** A planner test: a node with `publishes` Cargo metadata
    → `plan.interfaces` has the topic → `resolve_topic_interface` resolves it (no
    build, no sidecar).
  - **C3d — declarations.** `ws-bridge-rust` `talker_pkg` declares
    `[[package.metadata.nros.node.publishes]] topic="/chatter" type="std_msgs/Int32"`.
  - **C3e — sync wiring.** `nros sync` runs the planner per bringup (now resolving
    via synthetic metadata) → `render_bridge_runtime_config` → writes
    `<bringup>/nros-bridge.toml`.
  Done: `render_bridge_runtime_config` resolver core (`8358836f1`) — it consumes
  whatever `resolve_topic_interface` returns, so C3a–C3c are what make names-only
  resolve.

  **Pipeline EXPLORED (2026-06-27) — no hidden blocker.** Synthetic Cargo
  metadata flows through the SAME entity path as the post-build sidecar:
  `synthetic_metadata_artifacts` → `planner.rs:88` appends them →
  `build_node_instance` matches by `(package, executable)` via
  `find_source_metadata` → `source_entities`/`collect_entity_array` reads
  top-level `publishers`/`subscribers` arrays → `schema_entity` → `plan.interfaces`
  → `resolve_topic_interface`. So adding entities to the SYNTHETIC JSON is
  sufficient — NO planner change. The exact entity shape `collect_entity_array`
  accepts: `publishers: [{ id, topic, type: { package, name, kind: "message" } }]`
  (and `subscribers`). Concrete checklist: C3a schema (DONE `8308ef26f`+local) →
  extend `CargoComponentSummary` with `publishes`/`subscribes` → thread them in
  `synthesise_summary` → emit `publishers`/`subscribers` arrays in
  `summary_to_synthetic_json` (parse `"pkg/msg/Name"` → `{package, name}`).
- **C4 — macro bakes.** `nros::main!` reads the resolved sidecar (best-effort,
  like the lifecycle/param reads it already does), bakes `SESSION_SPECS` +
  `BRIDGES`, and emits the `open_multi` + runner spin loop when a bridge exists.
  Macro stays dep-lean (only `toml`; reads the generated sidecar).
- **C5 — `ws-bridge-rust`.** Names-only `[[bridge]]` + plain `nros::main!`;
  `nros sync` + `cargo build` links both backends (cyclone via the Entry's
  `nros-rmw-cyclonedds-sys` dep). Build-verify.
- **C6 — gated runtime e2e** (folds W5): zenohd + the entry + a stock
  `rmw_cyclonedds_cpp` peer sees `/chatter`. Reference test:
  `packages/testing/nros-tests/tests/bridge_zenoh_to_cyclonedds.rs` (zenohd +
  `talker_binary` + bridge + nano cyclone listener / stock ros2 echo, gated).

  **BLOCKED (2026-06-27) — the `run_from_config` cyclonedds path is untested + has
  3 runtime gaps.** Env prepared (zenohd `build/zenohd/zenohd`, the entry builds,
  the reference test identified); runtime-smoking `ws-bridge-rust/native_entry`
  against zenohd surfaced:
  1. **`open_multi` + cyclonedds → `OpenSession(Transport(InvalidArgument))`.**
     `nros_bridge::run_from_config` opens via eager `Executor::open_multi`, which
     has **never been runtime-tested** (only codegen-SHAPE tests assert the
     emitted `open_multi(&SESSION_SPECS)` string; `orchestration_generate.rs`). The
     WORKING imperative bridge (`bins/bridge-zenoh-to-cyclonedds-fwd`) uses
     `open_with_rmw` (primary) + lazy `node_builder().rmw("cyclonedds").domain_id()
     .build()` (extra session), NOT `open_multi`. `open_multi`'s cyclone-extra path
     needs debugging (cyclone `session_open` itself only returns INVALID_ARGUMENT
     on a null `out` — so the fault is in `open_multi`'s wiring of the extra
     session, or the zenoh primary spec).
  2. **Type-name form.** `render_bridge_runtime_config` emits the ROS type
     `"std_msgs/msg/Int32"`, but the raw zenoh keyexpr + Cyclone topic need the
     DDS-MANGLED `"std_msgs::msg::dds_::Int32_"` (the imperative bin's `TYPE_NAME`).
     The resolver must mangle ROS→DDS for the `nros-bridge.toml` `type`.
  3. **Descriptor staging.** `run_from_config` does NOT stage the Cyclone
     `dds_topic_descriptor_t` (the imperative bin calls
     `nros_rmw::register_type_descriptor`). `std_msgs/msg/Int32` is baked into
     `nros-rmw-cyclonedds-sys/build.rs` by default, but under the MANGLED name —
     verify the baked default key matches, else `run_from_config` needs to stage
     it (it has no schema; the baked default is the only no-schema path).
  These are real gaps in the data-driven bridge (`config.rs`) cyclone path,
  exercised here for the first time. C6 = fix them + the gated test. The BUILD
  (C1–C5) + the resolution flow + the zenoh↔zenoh / zenoh↔xrce `run_from_config`
  paths are unaffected.

  **Fixes landed (2026-06-27) — full runtime diagnosis, 2 of 3 fixed:**
  - **Gap 1 (open_multi InvalidArgument) — FIXED + issue 0106.** Root cause: the
    backend `.init_array` self-register ctor is dead-stripped because the Entry
    DEPS but never REFERENCES the backend crate → `open_named` resolves a null
    vtable → InvalidArgument. Fix: `ws-bridge-rust/native_entry` `extern crate
    nros_rmw_{zenoh,cyclonedds_sys} as _;` force-links them (confirmed: open_multi
    then succeeds). Issue 0106 recommends `nros::main!` emit the register calls so
    the Entry stays boilerplate-free.
  - **Gap 2 (type form) — FIXED.** `render_bridge_runtime_config` now emits the
    DDS-mangled wire type (`interface_type_name` → `std_msgs::msg::dds_::Int32_`)
    instead of the ROS form, so the raw zenoh keyexpr + Cyclone topic match.
  - **Gap 3 (descriptor staging) — issue 0107, FIXED at W-B (below).**

## W-B — Descriptor staging via config field schema (fix B). **DONE (2026-06-28).**

Chosen over the typed-macro path (A) and the generated-crate path (C) for being
fully data-driven + dep-lean — `nros-bridge.toml` becomes self-describing and the
Entry needs no msg-crate deps. Key insight that makes B clean: the raw-forward
path deserialises CDR into a `calloc(desc->m_size)` buffer and re-serialises from
it, so the descriptor offsets only need to be **self-consistent**, not match any
host `offset_of!` — sync emits field name+type, the runtime computes a C packing.

- **W-B1 (`nros-bridge` runtime).** `BridgeCfg` gains `ros_type` + `fields`
  (`[{name,type}]`). `run_from_config` builds a `&'static [Field]` (leaked,
  NUL-terminated names, packed offsets) and calls
  `nros_rmw::register_type_descriptor(ros_type, fields)` before
  `create_publisher_raw`. No-op for schema-less bridges (zenoh↔zenoh).
- **W-B2 (`nros sync`).** `render_bridge_runtime_config(plan, ws_root)` reads the
  forwarded type's `Message::FIELDS` from the generated crate
  (`generated/<pkg>/src/<kind>/<snake>.rs`) and emits `ros_type` +
  `fields`. Registers nothing for non-flat (nested/array/sequence) messages.
- **W-B3 (e2e + a new bug).** Verified zenoh talker → zenohd → native_entry
  declarative bridge → stock `rmw_cyclonedds_cpp` subscriber on domain 5 receives
  `std_msgs/Int32` (7/7 samples). Surfaced + fixed **issue 0109**: `create_node_on`
  dropped the configured `domain_id` so the cyclone egress opened on domain 0;
  added `Executor::create_node_on_with_domain` and threaded each `[[node]]`'s
  domain through `run_from_config`.
  **Net: end-to-end forwarding GREEN.**

**Cleanup:** remove the now-unused `render_bridge_entry_fns` + `emit_bridge_entry_fns`
(S1) and, once the runtime path lands, the dead `build_generated_package` relay.

## W2 — Component metadata so forwarded topics resolve

**DONE (2026-06-27, C1–C5).** Resolved via SYNTHETIC node metadata, not the
baked-relay path below: node pkgs declare `[[package.metadata.nros.node.publishes]]`,
the planner reads them into `plan.interfaces` → `resolve_topic_interface`, so
`plan.bridges[0].topics == ["/chatter"]` (`std_msgs/Int32`) with no separate
metadata-collection build. Proven by the W-B e2e. (Original gap analysis below.)

**Gap:** `forwarded_topics` resolves the bridge's topic list from the plan's
`interfaces`, which come from per-component publisher/subscriber metadata. A
launch-only `nros plan` (or a metadata-less bake) leaves `interfaces=[]` →
`topics=[]` → the bridge forwards nothing.

**Work:** ensure the bridge build collects component metadata before planning (the
fixture/workspace lane builds the node pkgs → sidecar metadata; the standalone
path does not). Confirm `talker_pkg`'s `/chatter` `std_msgs/Int32` publisher
surfaces in `interfaces`, so `plan.bridges[0].topics == ["/chatter"]`.

**Acceptance:** the resolved `ws-bridge-rust` plan has `topics=["/chatter"]`;
`validate_bridges` passes (topic resolves to `std_msgs/Int32`).

## W3 — Pure-cargo baked Rust entry build lane

**SUPERSEDED (W1c design pivot).** The config-driven runtime bridge uses a PLAIN
`nros::main!` entry + `run_from_config_str` — there is no baked
`build_generated_package` relay to build, so this whole lane is moot. `cargo build`
of `native_entry` links both backends directly (verified). **The one real residual
is a `[[workspace_fixture]]` lane for `ws-bridge-rust` in `examples/fixtures.toml`
+ a cyclonedds-gated CI build** — deferred to the test wave (with W5). (Original
baked-relay plan below, kept for context.)

**Gap:** existing Rust workspaces build the `nros::main!` macro entry (no bridge
relay). The bridge needs the BAKED orchestration entry (`build_generated_package`
→ `generate_package` → `src/{lib,main}.rs` + `Cargo.toml` with the backend deps +
`register_bridges`), the path C/cpp drive via CMake. No lane builds a pure-cargo
*baked* Rust entry.

**Work:** wire the command sequence for a Rust workspace bridge — `nros ws sync`
→ `nros codegen-system` → generate the orchestration entry → `cargo build` — and
add a `[[workspace_fixture]]` lane (`examples/fixtures.toml`) for
`ws-bridge-rust`. The generated entry must link `nros-rmw-zenoh` +
`nros-rmw-cyclonedds-sys` (W0) and compile the vendored C++ CycloneDDS
(submodule + `cyclonedds-ci`-style gate).

**Acceptance:** `cargo build` of the generated `ws-bridge-rust` entry links clean
(both backends, `register_bridges` present); fixture lane builds it in CI
(gated-skip if the cyclonedds submodule is absent).

## W4 — Per-type cyclone descriptor staging in the generated relay

**SUPERSEDED by W-B (fix B), DONE for flat types (2026-06-28).** Staging happens
in the config-driven runtime, not a generated relay: `nros sync` carries the flat
field schema in `nros-bridge.toml` and `run_from_config` stages the descriptor via
`register_type_descriptor` (self-consistent offsets — no idlc, no `M::FIELDS` dep on
the Entry). **Residual:** non-flat messages (nested / array / sequence) are not yet
stageable (`parse_fields_const` bails → no schema emitted); a bridge forwarding e.g.
`geometry_msgs/Pose` stages nothing. Follow-up — extend the schema emit + the
runtime field builder to nested/sequence (or fall back to the typed
`register::<M>` path for those). (Original idlc-relay plan below, kept for context.)

**Gap:** cyclone egress rejects a raw publisher whose type descriptor is not
registered. The generated `register_bridges` creates raw pubs by `(name, hash)`
only. Baked types (`std_msgs/Int32`, `rmw_dds_common_graph`) work; arbitrary
forwarded types do not.

**Work:** for cyclone endpoints, have the generated entry stage each forwarded
topic's descriptor before the publisher loop — wire `nros codegen
cyclonedds-descriptors` (already exists: `.msg` → IDL → `idlc` → `register.c` +
manifest) into the generated package's build, OR emit
`nros_rmw::register_type_descriptor(TYPE, <pkg>::msg::<Msg>::FIELDS)` (needs the
message crate as a generated-entry dep). Demo (`Int32`) is unblocked by W0's baked
default, so this wave can land after a green Int32 bridge.

**Acceptance:** a bridge forwarding a NON-baked custom type creates its cyclone
egress publisher without error.

## W5 — Runtime e2e (gated) + `ws-bridge-rust` completion

**DONE (2026-06-29) — gated fixture test landed.** The runtime forward, first
verified manually (7/7 `std_msgs/Int32`, honoring #53 egress-domain + #67 raw
path), is now codified:
- `[[workspace_fixture]]` `workspace-rust-native-bridge` in `examples/fixtures.toml`
  (config-driven, cyclonedds-gated) + `build_native_workspace_rust_bridge_entry()`
  resolver. Verified: the fixture-built `native_entry` forwards 7/7 to a stock
  `rmw_cyclonedds_cpp` subscriber.
- `tests/declarative_bridge_zenoh_to_cyclonedds.rs` — zenoh talker → the declarative
  fixture entry → nano cyclone C listener; gated/skips without zenohd/cyclone
  fixtures. Mirrors the imperative `bridge_zenoh_to_cyclonedds.rs` Path A.

**Endpoint overrides (issue #113, DONE).** The entry bakes its locator + cyclone
domain, but `run_from_config` now applies `NROS_BRIDGE_<NODE>_{LOCATOR,DOMAIN}`
over the baked config (`apply_node_env_overrides`), so a deployed/tested bridge
re-points at a different router / domain without a rebuild. The test uses an
ephemeral router + `unique_ros_domain_id()` via those overrides (no fixed-port /
fixed-domain caveat). The `ws-bridge-rust` README + phase-263 B3 are DONE; issue
#99 resolved upstream. (Original plan below.)

**Work:** boot zenohd + the baked `ws-bridge-rust` entry (talker + bridge) + a
stock `rmw_cyclonedds_cpp` subscriber; assert `ros2 topic echo /chatter` receives
the talker's counter — proving cross-RMW forward + ROS 2 interop. Honor #53
(egress domain threaded) + #67 (multi-RMW raw path). Flip the workspace README
from WIP to DONE; update the phase-263 B3 entry.

**Acceptance:** the runtime test passes where a live DDS peer is present (gated,
same contract as the existing `bridge-zenoh-to-cyclonedds-fwd` fixture); skips
cleanly otherwise.

## Sequencing

**Actual path (post-W1c pivot):** W0 (engine) → W1 (investigation) → W1c (config-driven
design) → C1–C5 (build end-to-end) → W-B (descriptor staging, fix B) → forwarding
GREEN. The original W2→W3 baked-relay sequence was superseded: W2 folded into the
synthetic-metadata resolver, W3 (baked entry) became moot (plain `nros::main!`), W4
became W-B. Only the gated automated test (W5 residual) remains.

## Acceptance (phase)

- [x] `examples/workspaces/ws-bridge-rust` builds via the documented config-driven
  flow (plain `nros::main!` + `run_from_config_str`), linking both backends.
- [x] zenoh→cyclonedds forwarding to a stock ROS 2 peer — **proven manually** (7/7
  `std_msgs/Int32`); a *gated automated* test is the one remaining follow-up.
- [x] Issue #99 resolved (upstream); phase-263 B3 + `ws-bridge-rust` README flipped
  to DONE.
- [ ] Non-flat forwarded types (W4 residual) + the xrce variant (`zenoh↔xrce`) —
  additive follow-ups; xrce skips W-B (lazy type registration).
