---
rfc: 0052
title: "SystemModel → RTOS primitives — the execution/contract mapper for embedded consumption"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-296]
supersedes: []
superseded-by: null
---

# RFC-0052 — SystemModel → RTOS primitives

## Summary

RFC-0050 defined the SystemModel; play_launch phase 43 made it real (the
Linux runtime consumes it end-to-end). This RFC defines the **nano-ros
half**: how the model's execution layer (tiers, bindings, deploy) and
contract layer (rates, ages, budgets, QoS) map onto the RTOS primitives a
baked image actually has — task priorities, stacks, core pinning,
preemption thresholds, executor scheduling contexts, and on-target
monitors. One mapping table, applied uniformly across POSIX, FreeRTOS,
Zephyr, ThreadX, and NuttX, landing on the seams that already exist
(RFC-0015 tiers, RFC-0016 priority normalization, RFC-0047 sched-context
binding, RFC-0045 boot config, RFC-0048 CMake verbs).

## Baseline facts (2026-07 exploration)

- `system.toml` tiers are resolved by `nros-orchestration-ir::resolve_tiers`
  (the CLI/`nros::main!` shared SSoT) and narrowed to the runtime
  `TierSpec { name, groups, priority, stack_bytes, spin_period_us }`
  (`packages/core/nros-platform/src/board/tier.rs`).
- **Lossy narrowing**: `core`, `preempt_threshold`, `sched_class`, `class`,
  `period_us`, `budget_us`, `deadline_us`, `deadline_policy` die at the
  codegen boundary (`codegen_system.rs::PlanTierDoc`); FreeRTOS's
  `emit_cpp` additionally drops `stack_bytes` (documented in
  `freertos_run_tiers.c`).
- The executor already HAS sporadic-budget ticking and time-triggered
  windows (`sched_context.rs::tick`, `tt_window_offset_us`) — but only via
  the programmatic API, never fed from tier tables.
- There is **no message-age concept** in `nros-node`: the take path
  (`handles.rs::try_recv`) never extracts `header.stamp`; no rate/jitter
  accounting exists.
- The vendored `ros-launch-manifest` `model`/`sched` crates define
  `TierDef`/`TierPlatformSpec` that duplicate `nros-orchestration-ir`'s
  `TierDef` — two hand-mirrored tier schemas (the FFI-mirror-drift class,
  cross-repo).

## Model ingestion (build side)

`nros codegen-system` gains a model mode: `--model system_model.yaml`
(mutually exclusive with the launch/system.toml pair). Per RFC-0050 the
model is checked-by-construction; ingestion is selection + mapping, not
re-validation:

1. **Slice selection**: `execution.deploy` picks this image's nodes —
   entries with `target: mcu:<board>` matching the build's board (from
   `nano_ros_use_board` / `package.xml <export><nano_ros board=…>`). A
   model with no `deploy` section and exactly one candidate node set is
   accepted with a note; ambiguity is a hard error.
2. **Tiers + bindings** → `ResolvedTierTable` (existing tier_resolver
   path) → `nros-plan.json` + the `run_tiers` const table. Bindings keys
   are node FQNs or `FQN/callback_group` — exactly RFC-0047's binding
   granularity.
3. **Boot config** → RFC-0045 baked rung: domain/locator from
   `execution.deploy[node]` (falling back to `meta`-level system config),
   emitted into `system_config.h` like today's `codegen_system` output.
4. **Wiring** → topic names for the node's endpoints (layer 1), replacing
   what the launch XML carries today.
5. **Contracts** → a generated per-node `const` monitor table (see
   "On-target monitors"), plus QoS onto endpoint config.

The vendored `model` crate is the parser — no schema re-declaration in
nano-ros. **Schema unification**: `nros-orchestration-ir::TierDef` gains
`From<ros_launch_manifest_sched::TierDef>` (explicit conversion, not type
replacement — orchestration-ir must stay `no_std`-friendly for the
proc-macro; drift is caught by a round-trip test over every field, the
FFI-mirror lesson applied).

## The mapper

### Execution layer → task/scheduling primitives

| Model field | POSIX | FreeRTOS | Zephyr | ThreadX | NuttX |
|---|---|---|---|---|---|
| `priority` (+ `sched_class`) | `pthread_setschedparam` SCHED_FIFO/RR (upgrade from today's advisory-only) | `xTaskCreate` priority (RFC-0016 normalize) | `k_thread` priority (negative coop admitted) | `tx_thread_create` priority | pthread SCHED_FIFO priority |
| `stack_bytes` | `pthread_attr_setstacksize` | task stack words (**fix the emit_cpp drop**) | `K_THREAD_STACK` size | thread stack size | `pthread_attr_setstacksize` |
| `core` | `pthread_setaffinity_np` | `vTaskCoreAffinitySet` (SMP builds; ignore+note on UP) | `k_thread_cpu_pin` | SMP core exclusion mask | `pthread_attr_setaffinity` |
| `preempt_threshold` | reject (validate-time error) | reject | reject | `tx_thread_preemption_change` | reject |
| `spin_period_us` | tier spin loop (exists) | exists | exists | exists | exists |

Rejection semantics: a platform-inapplicable field in the SELECTED
target's sub-table is a **bake-time error** (same philosophy as
play_launch 43.3's missing-sub-table hard error): the integrator wrote a
knob the platform cannot honor; silently ignoring it is the domain-0
class of bug. Fields in OTHER platforms' sub-tables are ignored (one
model serves all targets).

### `class` → executor scheduling mode

| `class` | Mapping |
|---|---|
| `best_effort` (default) | plain tier task; no deadline machinery |
| `real_time` | fixed-priority preemptive tier + deadline monitor when `deadline_us` set |
| `time_triggered` | executor TT window (`tt_window_offset_us` — exists): `period_us` = window period, tier spin aligned; requires `period_us` |
| `interrupt` | out of scope v1 — bake-time reject (ISR-context executors are their own RFC) |

`budget_us` + `period_us` → the existing sporadic-budget `SchedContext`
(`tick`), fed from the tier table instead of only the programmatic API.

`deadline_policy` → monitor action: `ignore` (no monitor), `warn`
(diagnostics entry), `skip` (executor skips the group's remaining
callbacks this cycle — `set_active_groups` seam), `fault` (platform fault
hook: `nros_fault()` → board-defined, defaults to panic).

### Contract layer → on-target monitors

Generated per-node `const` table (no heap, no YAML on target):

| Contract | Monitor | Seam |
|---|---|---|
| sub `max_age_ms` | `now - header.stamp` at take; requires the **new** stamp extraction: CDR peek of the leading `std_msgs/Header` when the type has one (codegen knows; non-stamped types get no age monitor) | take path (`handles.rs::try_recv`) |
| pub `min_rate_hz` / `jitter_ms` | per-endpoint publish counter + EWMA period, checked per spin tick (~the play_launch 5 s cadence, scaled to `spin_period_us`) | publish path + spin loop |
| node path `max_latency_ms` | take-timestamp → publish-timestamp delta for the declared (input, output) pair | executor sched-context (already threads a monotonic clock) |
| topic QoS | baked endpoint QoS config (exists — RFC-0006 axes) | codegen |
| scope paths / drops | NOT monitored on-target v1 — cross-machine E2E is the Linux side's job (`max_age_ms` at the final subscriber catches the total, per RFC-0050) | — |

Violations surface through the existing diagnostics path
(`nros-diagnostic-updater`), one entry per rule id, mirroring
play_launch's severity vocabulary — the two runtimes report the same
contract in the same words. The assumption/guarantee split (sub = assume,
pub/path = guarantee) rides into the diagnostic payload for the 4-quadrant
diagnosis RFC-0050 describes.

Cost discipline: every monitor is compile-time-gated by the presence of
its contract field in the model — an uncontracted image bakes zero
monitor code (`const` table empty → dead-code elimination).

## CMake surface

`nano_ros_add_executable(... MODEL path/to/system_model.yaml)` as the
alternative to `LAUNCH "pkg:file.xml"` — the seam ASI already sits on
(its `system.launch.xml` is exactly what the resolved model replaces).
`MODEL` implies model-mode `codegen-system`. The launch-file path stays
supported; deprecation is a later decision.

## Non-goals

- `interrupt` tier class (v1 rejects).
- On-target scope-path/drop monitoring (Linux side owns E2E).
- Retiring `system.toml` — it survives as the integrator's AUTHORING
  input (`[deploy]`/tiers) that play_launch `resolve` consumes (closing
  RFC-0050's open question). What retires is nano-ros's OWN
  resolution/bake path — see §Canonical path below.
- Dynamic model reload — baked images are one variant by construction.

## Canonical path (maintainer decision, 2026-07-17)

The SystemModel is the **canonical** configuration path. nano-ros's own
resolution machinery — `launch_synth`, `nros plan`'s launch-XML parsing,
`codegen-system`'s direct system.toml+launch consumption — is
**transitional** and retires once model parity lands. Consequences:

- New configuration features land model-side (play_launch resolve or the
  shared ros-launch-manifest crates), never in the legacy bake path.
- play_launch is improved along the way as nano-ros needs surface:
  `Deploy{domain, locator}` schema fields, `resolve` reading the
  integrator's `system.toml [deploy]`, per-target resolves for
  multi-target systems.
- Even embedded-only projects run `play_launch resolve` at BUILD time
  (build hosts are Linux); the target never parses anything — it gets
  the baked slice, as today.
- Retirement trajectory staged in phase-296 (§Retirement).

## Parity gap analysis (2026-07-17 exploration)

Everything the legacy path (`system.toml` + launch XML + `nros-plan.json`)
expresses, versus the model schema. Status: **model** = needs a schema
addition (shared crates); **resolve** = play_launch resolve work;
**local** = stays a nano-ros build knob (never system semantics);
**covered** = already expressible.

| Legacy feature | Source | Status → home |
|---|---|---|
| Node parameters (resolved key→values, baked into component configure) | launch `<param>` / `PlanInstance.parameters` | **model** — `structure.nodes[].params`. ROS parameters ARE system semantics; the "no spawn info" exclusion was about cmd/env/params-FILES, not resolved param values. Embedded has no record to read them from. |
| Remaps | launch `<remap>` | **covered** — the model's wiring carries RESOLVED topic FQNs; entry codegen binds endpoints to those names. Verify at W4.1. |
| Component class (`pkg::Class`) | `[[component]].class` | **covered-by-mapping** — model `NodeInstance.plugin` carries the class for library-component nodes (exec unused); the exec→class lookup for launch-sourced systems stays in cmake metadata. Document in W4.1. |
| RMW selection (`[system].rmw`, `[deploy.*].rmw`, `--rmw`) | system.toml | **model** — `execution.deploy[].rmw` + a system default (meta or execution header). The bake cannot pick a backend without it. |
| domain/locator (system + per-deploy override ladder) | system.toml | **model** — `Deploy{domain, locator}` (already filed as R1 ask #1). |
| `[[transport]]` network identity (ip/mac/gateway/interfaces, wifi ssid/psk, serial/can device+baud, per-transport rmw/locator/domain) | system.toml → `PlanTransport` | **model** — `execution.transports` (typed, per deploy target). Integrator-owned system config; the embedded boot bake (RFC-0045) and bridge sessions read it. Largest single gap. |
| `[[bridge]]` in-binary relays (from/to/topics/bidirectional) | system.toml (RFC-0009) | **model** — `execution.bridges` passthrough; `nros sync`-style type resolution moves behind resolve (types come from layer 1). |
| `[[domain]]` multi-domain routing | system.toml | **model** — folds into `execution.transports` (a transport = (rmw, locator, domain) session). |
| `[lifecycle] autostart` | system.toml | **model** — per-node `lifecycle_autostart` (`none\|configure\|active`) on `NodeInstance` or deploy entry; the contract-layer `lifecycle` flag stays the managed-node marker. |
| Capability axes (`features = [..]`, `[safety]`, `[param_services]`) | system.toml | **model** — `execution.features: [String]` (system-level; per-deploy `features` see build tuning below). Unknown names stay a bake-time error (capability_resolver). |
| Per-deploy build tuning (`profile`, `optimize`, cargo `features`), `kind`, `framework` | system.toml `[deploy.*]` | **model (passthrough)** — `Deploy.extra: map` (consumer-defined, documented keys). In the end state nano-ros must not parse system.toml at all, so even build knobs ride the model — as an open map, not typed schema (they are not cross-runtime semantics). |
| Per-endpoint QoS (manifest per-endpoint QoS + 211.H `qos_overrides.*` launch params) | manifests + launch params | **model** — endpoint contracts (`PubContract`/`SubContract`) gain optional `qos`; the 211.H launch-param overlay retires into it. |
| Board selection | `[deploy.*].board` / cmake | **covered** — `mcu:<board>` target encodes it. |
| `default_launch` / `default_target` | system.toml | **retire** — resolve-input ergonomics; obsolete once resolve is the front door. |
| RFC-0033 message capacities (`nros-codegen.toml`) | workspace file | **local** — message-generation knob, not system semantics. |
| Monitor tables (contracts → baked consts) | new (W3b.4) | **resolve→nano-ros** — emission from model contracts, already planned. |
| Actions wiring | manifest `actions:` | **resolve** — play_launch's ManifestIndex never merges actions; `structure.actions` is always empty today. |

Ordering: the transports/bridges/rmw block is the R1 critical path for any
real embedded system; params + per-endpoint QoS next; lifecycle/features/
extras are small.

## Open questions

- Stamp extraction ABI: peek-decode `Header` in the take path vs codegen
  emitting a per-type `stamp_offset` const (leaning const — no runtime
  type introspection).
- Whether POSIX tier priorities upgrade to real SCHED_FIFO by default or
  behind a knob (today advisory; play_launch's rt_helper precedent says
  knob + privilege check).
- `bindings` targeting a callback group that the node code never declares:
  bake-time error vs warn (leaning error — same fail-loud family).

## Cross-track note — play_launch Phase 45 (Scheduling SSoT), 2026-07-18

play_launch's RT-scheduling track (vocab v2 + the `chain_aware` mapper,
play_launch Phases 41/42/44) and the SystemModel track are being unified so
that **the SystemModel is the single source of truth for scheduling**:
`play_launch resolve` runs the mapper once and embeds its *complete* output
into the model; every consumer — including this RTOS mapper — reads
scheduling from the model and never re-derives it. Design of record:
play_launch `docs/design/system-model-sched-ssot.md`; work breakdown:
play_launch `docs/roadmap/phase-45-sched_ssot_unification.md`.

What this RFC's consumer side gains, and the cross-track asks:

- **The model's `execution:` layer gains resolved scheduling structure**
  (Phase 45.2, in the shared `model` crate — a joint decision with this
  track): `mapper` identity, resolved `chains` (FQN-qualified `via` topics +
  the segment/boundary decomposition), and **per-path ranks**
  (`ChainAwareDetail`, one per (node, path)). Today `execution` is
  `tiers` + `bindings` only; these are additive fields.
- **Per-path ranks are exactly what this RFC's callback-granularity mapping
  wants.** play_launch's POSIX apply layer projects per-path ranks down to a
  per-node max (a documented lossy compression); an RTOS executor can
  discriminate at callback granularity (this RFC's `sched_context.rs` already
  has the machinery). Embedding the per-path ranks means nano-ros need not
  inherit play_launch's POSIX lossy projection — the finer fact is carried in
  the artifact.
- **Type sharing (Phase 45.3):** the resolved chain/trigger structs
  (`types::{Trigger,EffectiveTrigger,ChainDecl,...}`, `sched::{ResolvedChain,
  ChainElement,MapperPath,ChainAwareDetail}`) are shared from
  ros-launch-manifest — no third hand-mirror in the `model` crate. The
  translation `types::ChainDecl` + launch DAG → `sched::ResolvedChain` is
  `sched_derive::resolve_chains` (play_launch); it must be shared, not
  reimplemented. This is the same "one schema, no hand-mirroring" rule
  RFC-0050 already states (issue 0160 / FFI struct-mirror lesson).

Ask of this track: (1) confirm where the resolved chain data lands —
`execution:` (alongside `tiers`/`bindings`) vs `contracts:` layer — and
(2) the per-path-rank consumption model on the RTOS side (does the executor
bind a callback-group priority from `ChainAwareDetail.path`, or project to a
per-node task priority like POSIX). play_launch's 45.2/45.3 are held pending
this coordination.
