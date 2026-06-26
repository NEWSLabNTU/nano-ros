# Phase 267 — Declarative cross-RMW bridge: complete the bake→entry→build flow

Status: **In progress (2026-06-26)** — W0 done; W1 done (investigation — the live
entry emitter, not the bake record, is the gap); **design DECIDED (W1c,
2026-06-27): config-driven runtime bridge** — user writes names-only `[[bridge]]`
+ plain `nros::main!`; `nros sync` resolves type+hash; macro bakes + drives the
runtime `PubSubBridge`. No build.rs, no codegen-relay dup. (W1b route-(a) codegen
+ S1 superseded.) **C1 (schema) done (`e6865d718`); C2–C6 remaining.** ·
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
- **C2 — runtime runner.** `nros::bridge` data-driven API: given the resolved
  specs + a multi-session `Executor`, open the per-endpoint nodes, create the raw
  sub/pub per topic, build one `PubSubBridge` each, and `pump()` them every spin.
  Reuses `PubSubBridge` (one relay impl). Unit-testable construction; runtime
  exchange gated (W5).
- **C3 — `nros sync` resolves bridges.** Resolve each `[[bridge]]` topic name →
  `type_name`+`type_hash` from the planner's component interfaces (folds in old
  W2), resolve `from`/`to` → session specs, emit the RESOLVED spec sidecar (a
  generated file the macro reads — does NOT mutate the user's `system.toml`).
- **C4 — macro bakes.** `nros::main!` reads the resolved sidecar (best-effort,
  like the lifecycle/param reads it already does), bakes `SESSION_SPECS` +
  `BRIDGES`, and emits the `open_multi` + runner spin loop when a bridge exists.
  Macro stays dep-lean (only `toml`; reads the generated sidecar).
- **C5 — `ws-bridge-rust`.** Names-only `[[bridge]]` + plain `nros::main!`;
  `nros sync` + `cargo build` links both backends (cyclone via the Entry's
  `nros-rmw-cyclonedds-sys` dep). Build-verify.
- **C6 — gated runtime e2e** (folds W5): zenohd + the entry + a stock
  `rmw_cyclonedds_cpp` peer sees `/chatter`.

**Cleanup:** remove the now-unused `render_bridge_entry_fns` + `emit_bridge_entry_fns`
(S1) and, once the runtime path lands, the dead `build_generated_package` relay.

## W2 — Component metadata so forwarded topics resolve

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

**Work:** boot zenohd + the baked `ws-bridge-rust` entry (talker + bridge) + a
stock `rmw_cyclonedds_cpp` subscriber; assert `ros2 topic echo /chatter` receives
the talker's counter — proving cross-RMW forward + ROS 2 interop. Honor #53
(egress domain threaded) + #67 (multi-RMW raw path). Flip the workspace README
from WIP to DONE; update the phase-263 B3 entry.

**Acceptance:** the runtime test passes where a live DDS peer is present (gated,
same contract as the existing `bridge-zenoh-to-cyclonedds-fwd` fixture); skips
cleanly otherwise.

## Sequencing

W1 → W2 → W3 unblock a *building* Int32 bridge (the visible milestone); W4 is
additive (non-baked types); W5 is the gated runtime proof. W1 is the immediate
blocker (the bake plan must carry the bridge before anything downstream sees it).

## Acceptance (phase)

- `examples/workspaces/ws-bridge-rust` builds via the documented bake flow, its
  generated entry linking both backends with the `register_bridges` relay.
- A gated runtime test proves zenoh→cyclonedds forwarding to a stock ROS 2 peer.
- Issue #99 resolved; phase-263 B3 flipped to DONE.
- The xrce variant (`zenoh↔xrce`) is reachable by the same flow (xrce is a wired
  Rust backend with lazy type registration — needs W1–W3, skips W4) — a
  lower-build-cost sibling, optional.
