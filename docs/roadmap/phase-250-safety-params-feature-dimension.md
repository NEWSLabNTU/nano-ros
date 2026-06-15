# Phase 250 — safety + params as declared, system-wide capability toggles

Status: **COMPLETE (Waves 1, 2a, 2b, 3, 5; Wave 4 deleted — 2026-06-16)** · Related: RFC-0031 (declared
selection → lowered build feature), RFC-0024 (declarative Node/Entry).

> **Model (settled 2026-06-16):** `safety` and `params` are **system-wide capability
> toggles** that lower to `nros` build features (`nros/safety-e2e`, `nros/param-services`),
> mirroring RMW selection. The user writes **normal ROS code** — `declare_parameter`,
> and optionally `.safety()`/`ctx.integrity()` to inspect integrity. There is **no**
> config→node behavior injection, per-node config, node-body codegen, or topic list. See
> "Design" below; the earlier "config-driven, no source edit" framing was wrong and is
> corrected there.

## Why

The native Rust examples carry per-example **Cargo feature gates** — `param-services`
(declare a `start_value` parameter + read it for the publish counter) and `safety-e2e`
(CRC + sequence-gap tracking on a subscription) — plus `link-tls` and
`unstable-zenoh-api` (zero-copy) variants. Fixtures build each example *with the feature
on* into an isolated `target-*/` dir, and `params.rs` / `safety_e2e.rs` / `zero_copy.rs`
run e2e against those variant binaries.

These are **application capabilities a user should toggle in config**, not reasons to
fork the example source or maintain `#[cfg(feature = …)]` variants. ROS expresses them
as node configuration, not as separate programs. The aim mirrors how RMW is already
declared (`system.toml [system].rmw`) and lowered (RFC-0031): make `params` and `safety`
**sibling declared axes**.

## Ground truth (2026-06-15) — corrects the prior premise

An earlier draft assumed phase-249 P3.5b had already migrated the native examples to the
declarative shape and removed these gates. **P3.5b was withdrawn.** Nothing was removed.
Current state:

- `param-services` / `safety-e2e` / `link-tls` / `unstable-zenoh-api` are **all still
  declared and used** in `examples/native/rust/{talker,listener}/Cargo.toml` +
  `src/main.rs`.
- `examples/fixtures.toml:370-412` still carries the per-feature variant rows
  (`param-services`, `link-tls` → `target-tls`, `safety-e2e` → `target-safety`, zero-copy
  → `target-zero-copy`).
- `params.rs` / `safety_e2e.rs` / `zero_copy.rs` are **active, never skip-gated**.
- Only `examples/native/rust/entry-poc` uses the declarative `nros::main!()` shape; every
  real example is still the imperative builder.

So phase-250 is a **greenfield conversion** of working feature-gated examples, not a
restore of something removed.

## Design — system-wide capability toggles, not per-node config injection (corrected 2026-06-16)

An earlier draft of this phase chased "config injects behavior into the node, no source
edit" — a runtime config→node overlay (declare params from config, flip `.safety()` on
named topics). The maintainer corrected the model, and it is both simpler and already
mostly built:

**Both axes are system-wide capability toggles (build features). The user writes normal
ROS code.** There is no config→node behavior injection, no per-node config, no node-body
codegen, no topic list, no declarative migration. Concretely:

- **safety-e2e is system-wide, not per-node.** It is an end-to-end integrity protocol —
  publishers attach CRC + sequence, subscribers validate — so it is meaningful only when
  the *system* runs it; enabling it on one node alone is pointless. It is therefore a
  **system-wide build feature** (`nros/safety-e2e`), exactly what a declared `[safety]`
  axis lowers to. A node that wants to *inspect* the per-message status uses the optional
  `.safety()` / `ctx.integrity()` surface (like `message_info`) — that surfaces the status,
  it does not gate validation. No topic list, no overlay.

- **params are normal ROS code; the axis toggles the external server.** The user writes
  standard `declare_parameter` / `get_parameter` in node source. The declarative runtime
  **already** lazily registers the 6 ROS 2 parameter services (get/set/list/describe/…) on
  the first declared parameter (`node_runtime.rs` `EntityKind::Parameter`, gated
  `param-services`). So the `[param_services]` axis only toggles whether that **external
  query/update server** is compiled in — it does not inject parameter values. Values come
  from code, as in standard ROS.

So the lowering is the whole job: a declared axis → the `nros` build feature on the entry,
mirroring RMW (RFC-0031). It stays a **compile** dimension (embedded pays the safety
arena/CRC and param-service code only when selected). The capability is then used by normal
node code — `declare_parameter` (params) and, optionally, `.safety()`/`ctx.integrity()`
(safety inspection).

**Why the old "no source edit / config-driven behavior" framing was wrong:** it assumed a
config→node-behavior wire (overlay or node-body codegen) that does not exist and that this
model does not need. The node is hand-written; the axis governs the *build*, not the node's
logic. The per-topic-safety / topic-capture concern dissolves: topics are already harvested
at build time (`MetadataRecorder` → `to_source_metadata_json`), and system-wide safety needs
no topic list at all.

**Out of scope:** `link-tls` / `unstable-zenoh-api` (zero-copy) are **transport / RMW-backend**
features (`nros-rmw-zenoh?/link-tls`; `nros/unstable-zenoh-api`), not node capabilities —
they belong with the transport/RMW declared axis, not params/safety.

## Scope

1. **Correct premise + model — DONE (this doc).** System-wide capability toggles + normal
   user code; no config→node injection.
2. **`[safety]` axis → `nros/safety-e2e` — DONE (Wave 1).** System-wide build feature.
3. **`.safety()` / `ctx.integrity()` inspection surface on the declarative node — DONE
   (Wave 2a/2b).** Optional; a node reads per-message integrity in its callback.
4. **`[param_services]` axis → `nros/param-services` — DONE (Wave 3).** Toggles the external
   param server; node params are declared in normal code (runtime auto-registers the services).
5. **Validation example + fixtures — Wave 5.** A declarative `examples/workspaces/rust/`
   build with the axes on/off + a transport e2e proving safety surfaces `ctx.integrity()` and
   the param server answers. The native imperative examples + their fixtures/tests stay as-is
   (the imperative API ships under D7) — this **augments**.

## Waves

- **Wave 1 — safety lowering (Layer 1) — DONE (2026-06-15).** A declared `[safety]`
  overlay block lowers to the `nros/safety-e2e` umbrella feature on the generated entry,
  mirroring the existing `[param_persistence] → nros/param-services` and
  `[lifecycle] → nros/lifecycle-services` paths in `generated_default_features()`
  (`packages/cli/nros-cli-core/src/orchestration/generate.rs`). Wiring:
  `collect_safety()` (planner) reads the block (last-overlay-wins; `enabled = false`
  disables; `crc` defaults true) → `NrosPlan.safety: Option<PlanSafety>` (additive,
  skip-when-absent → byte-identical plans) → `generated_default_features(.., safety, ..)`
  pushes `nros/safety-e2e`. Tests: `collect_safety_reads_block_with_defaults` (planner),
  `safety_axis_lowers_to_nros_feature` (generate). `params` is **not** in Wave 1 — it
  already has a lowering path via `[param_persistence]`; a plain `[params]` (declare-only,
  no persistence) axis lands with the Layer-2 codegen wave. Layer 1 alone is not yet
  observable end-to-end (the imperative examples gate on their *own* Cargo feature, a
  different namespace) — it is the foundation the later waves consume.

  **Schema (`[safety]`, an nros.toml / `[package.metadata.nros]` overlay block):**
  ```toml
  [safety]
  enabled = true   # optional, default true; false drops the capability
  crc     = true   # optional, default true; CRC-32 check alongside seq gap/dup tracking
  ```

- **D7 re-examination — DONE (2026-06-15).** phase-244 D7's Shape-B *mechanism prose* is
  stale post-P4b (it cites the linkme `RMW_INIT_ENTRIES` section; P4b replaced it with the
  `.init_array` ctor — the `#[used] __FORCE_LINK_*` static now anchors the ctor object, same
  DCE role, still not a `register()` call). But D7's **substantive** decision — the native
  single-file talker/listener stay imperative `Executor::open`, do **not** migrate — is
  P4b-independent and **stands**. So the planned "declarative migration" wave is **dropped**:
  the declarative talker/listener already exist (`examples/workspaces/rust/`, esp32), and the
  real Layer-2 prerequisite is extending the declarative Node API (next).
- **Wave 2a — declarative E2E-safety surface (mechanism + API) — DONE (2026-06-15).**
  Shape A (chosen): `IntegrityStatus` rides the existing callback alongside the message,
  mirroring the imperative `FnMut(&M, &IntegrityStatus)`. Landed (all gated `safety-e2e`,
  zero-cost off):
  - `nros-node` arena `SubBufferedRawSafetyEntry` — the type-erased analog of
    `SubSafetyEntry`; the validator lives in the `RmwSubscriber`, so `try_recv_validated`
    yields `(len, IntegrityStatus)` with no typed `M` (the declarative path is generic).
  - `register_subscription_buffered_raw_safety_on` + `create_generic_subscription_with_integrity`
    (the declarative analog of `.typed::<M>().safety()`).
  - `nros` `CallbackCtx`: gated `integrity` field + `new_with_integrity()` + `integrity()`
    accessor (`None` for timers/services/non-safety subs). Test: `callback_ctx_integrity_surface`.
- **Wave 2b — declarative `.safety()` opt-in + runtime branch — DONE (2026-06-15).**
  `EntityMetadata.safety` (ungated flag; the reading branch is gated) set by
  `NodeContext::create_subscription_for_callback_name_with_safety`; the `node_runtime`
  `EntityKind::Subscription` arm branches on it (under `safety-e2e`) to
  `create_generic_subscription_with_integrity` + `dispatch_into_cell_with_integrity` →
  `CallbackCtx::new_with_integrity`, else the basic path. Test:
  `safety_opt_in_records_metadata_flag`. The full transport e2e (publish → validated recv →
  `ctx.integrity()` in a real spin) lands with the declarative fixture in Wave 5.
- **Wave 3 — `[param_services]` axis → `nros/param-services` — DONE (2026-06-16).** The
  param SERVER toggle, mirroring Wave 1. `collect_param_services()` (planner) reads the
  `[param_services]` overlay block (last-wins; `enabled = false` disables) →
  `NrosPlan.param_services: Option<PlanParamServices>` → `generated_default_features` pushes
  `nros/param-services` when `param_persistence || param_services`. The user writes normal
  `declare_parameter`/`get_parameter`; the declarative runtime already auto-registers the 6
  param services on the first declared parameter (`node_runtime` `EntityKind::Parameter`).
  Tests: `collect_param_services_reads_block`, `param_services_axis_lowers_to_nros_feature`.

  **Schema (`[param_services]`, an nros.toml / `[package.metadata.nros]` overlay block):**
  ```toml
  [param_services]
  enabled = true   # optional, default true; false drops the external param server
  ```

- **~~Wave 4 — safety codegen~~ — DELETED (2026-06-16).** It assumed config lowering into a
  generated node body. Under the corrected model safety is a system-wide build feature
  (Wave 1) used by normal node code via `.safety()` (Wave 2) — there is nothing to generate.
- **Wave 5 — declarative safety transport e2e — DONE (2026-06-16).** New fixture
  `packages/testing/nros-tests/bins/declarative-safety-listener`: a board-less declarative
  node (`Node` + `ExecutorNodeRuntime::from_executor`) whose subscription opts in via
  `create_subscription_for_callback_name_with_safety` and reads `ctx.integrity()`. The test
  `test_declarative_safety_listener_receives_integrity` (`tests/safety_e2e.rs`) runs it as a
  cross-process subscriber against the imperative safety talker over zenohd and asserts the
  declarative `.safety()` path surfaces `IntegrityStatus` (the `[SAFETY] INTEGRITY` token =
  `ctx.integrity() == Some`, `seq_gap=0`) and **validates real CRC-32** (`crc=ok` ≥ 3, no
  `FAIL`). Verified locally green (`3 integrity-surfaced, 0 absent, 3 crc-ok, 0 crc-fail`).

  **Root-cause fix (pre-existing bug, surfaced here):** `crc_valid` was `None` because the
  CRC attach (publisher) + validate (subscriber) live behind the **zenoh backend's own**
  `safety-e2e` (`nros-rmw-zenoh`), and `nros/safety-e2e` does **not** forward to it. The
  `examples/native/rust/{talker,listener}` safety features (and this fixture) now enable
  `nros-rmw-zenoh?/safety-e2e` directly. This affected the imperative safety path too — the
  existing `test_safety_e2e_talker_listener` `crc=ok` assertion could not pass over zenoh
  before. → tracked for the orchestration path in [issue 0072](../issues/0072-safety-e2e-backend-feature-not-lowered.md).

  **Phase 250 — COMPLETE** (Waves 1, 2a, 2b, 3, 5; Wave 4 deleted).

## Acceptance

- A declared `[safety]` axis lowers to `nros/safety-e2e` system-wide; `[param_services]`
  lowers to `nros/param-services` — both mirroring the RMW lowering, omitted when absent
  (byte-identical plans). **DONE** (Waves 1, 3).
- A declarative node reads per-message integrity via `.safety()` / `ctx.integrity()`, and
  declares parameters in normal code with the external server auto-registered when the axis
  is on. **DONE** (Waves 2, 3 + existing runtime).
- Embedded targets pay the safety/param code size **only** when the axis is selected (a
  compile dimension, not a runtime always-on path). **Held** — both lower to build features.
- Wave 5: a declarative example builds with the axes on + off and a transport e2e proves
  `ctx.integrity()` is surfaced and the param server answers. **Pending.**

## Risks

- **Two API shapes coexist by design.** The imperative `Executor` (D7-blessed, native) and the
  declarative `Node` path both expose safety/params; that is intended, not a conflict. Don't
  try to delete the imperative surface.
- **System-wide, not selective.** safety-e2e is all-or-nothing across the system (end-to-end
  protocol). There is deliberately no per-node / per-topic safety knob; don't add one without
  a concrete need (it would reintroduce the topic-capture / overlay complexity this model avoids).
