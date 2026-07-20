---
rfc: 0050
title: "SystemModel — one resolved system artifact from launch + contracts + system config, shared with play_launch"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0050 — SystemModel

## Summary

A **SystemModel** is a fully-resolved, checked, YAML-serialized description of
one concrete system variant: the node graph, its timing/QoS **contracts**, and
its **execution/deployment** assignment. It is produced by the play_launch
front-end (`play_launch resolve`) from three inputs — the ROS 2 launch tree,
the per-scope contract manifests (ros-launch-manifest, phases 34–35), and the
integrator's system config (tiers + deployment) — and consumed by exactly two
kinds of back-end:

- the **nano-ros build system**, which bakes each MCU node's slice into its
  image (executor tiers, QoS, budgets, domain/locator, wiring), and
- the **play_launch Linux runtime**, which spawns/supervises the Linux nodes
  and drives its rcl-interception monitors from the same contract numbers.

The shared type definitions live in the `ros-launch-manifest` repository (a
new `model` crate beside `types`/`check`), which BOTH projects already vendor.
One schema, no hand-mirroring — the cross-repo application of the FFI
struct-mirror lesson (issue 0160).

```
launch tree (XML/py) ──┐
contract manifests ────┼→ play_launch resolve: parse → bind args → filter
system config ─────────┘   conditions → merge scopes → check (14 rules)
                                     │
                          SystemModel  (system_model.yaml)
                        ┌────────────┴─────────────┐
              nano-ros build system       play_launch Linux runtime
              (bake per-image config)     (supervise + monitor contracts)
```

## Motivation

nano-ros and play_launch hold complementary halves of a real-time story:

- play_launch's manifest redesign (ros-launch-manifest phases 34–35) gives
  declarative **timing contracts** — endpoint rates/ages/jitter, node/scope
  path latency budgets, drop budgets, QoS — with a 14-rule static checker
  (rate-hierarchy, budget-overflow, graph-aware critical path, causal-dag,
  drop-sanity, Z3 satisfiability, cross-scope consistency) and a Linux
  runtime monitor (rcl interception: max_age on take, rates, burstiness).
- nano-ros RFC-0015/RFC-0047 give the **scheduling mechanism** — callback
  groups mapped to tiers, per-tier spin periods and RTOS priorities,
  declared in `system.toml`, baked at build time on embedded targets.

Nothing connects them. A cross-machine end-to-end budget (MCU sensor node →
Linux perception pipeline) is only meaningful if both sides read the same
numbers from the same artifact. The SystemModel is that artifact.

## The model

### One model = one variant (early binding)

`resolve` takes a **concrete argument assignment**, evaluates every
`if:`/`unless:` condition, substitutes every `$(var)`, resolves every name to
its FQN, merges scope declarations, and runs the checker. The emitted model
contains **no variables and no conditions**. Rationale:

- The embedded consumer has no choice: a baked MCU image is one variant.
- On Linux the resolver is present at run time (play_launch is resolver +
  runtime in one binary), so "operator picks a variant at launch" is just
  resolve executed at launch time — the artifact needs no internal variables.
- The reviewed artifact is byte-identical to what runs — the property any
  safety argument built on the contract checks will need.
- Variant-completeness stays where it is today: the Z3 satisfiability check
  over the whole arg space runs at the manifest level, pre-resolution.

Consequence: **resolve must be cheap and cache-keyed** (hash of arg binding +
manifest/launch/system-config content). Linux re-resolves per launch; CI
emits one model per matrix cell; a fleet binds per vehicle at image-build or
first-boot time.

### Three layers

1. **Structure** — resolved node instances (FQN, package, executable or
   composable plugin), merged topics/services/actions with types, endpoint
   wiring, the scope tree (kept for diagnostics + budget attribution).
2. **Contracts** — the ros-launch-manifest fields verbatim, post-merge:
   per-endpoint `min_rate_hz`/`max_rate_hz`/`max_age_ms`/`jitter_ms`,
   `state`/`required` subscriber modes, `max_response_ms`, node paths
   (`max_latency_ms`, `correlation`/`tolerance_ms`), topic
   `rate_hz`/`max_transport_ms`/`max_drop_rate`/`max_consecutive`/QoS,
   scope paths (E2E latency + drop budgets).
3. **Execution/deployment** — the nano-ros extension, integrator-owned:
   - `deploy`: node → target assignment (`linux` process vs `mcu:<board>`
     image), host/domain/locator;
   - `tiers`: the RFC-0015 tier table (spin periods, per-RTOS priorities)
     joined from `system.toml`;
   - callback-group → tier mapping per node.

Producers and consumers touch different layers: manifests own layer 2, the
launch tree owns layer 1, `system.toml` owns layer 3. play_launch runtime
reads the `linux` slice of layer 3; nano-ros build reads the `mcu:*` slice.

### Validity by construction

`resolve` **refuses to emit** a model when any checker rule reports
Error severity. Warnings are embedded in the model
(`diagnostics:` section) so downstream consumers and dashboards see them
without re-running the checker. A SystemModel in hand is therefore always a
checked one; consumers do not re-validate structure.

### Serialization + provenance

Canonical form is **YAML** (`system_model.yaml`), serde-defined in the shared
crate. A `meta` section carries:

- `version` — schema version (independent of the manifest format version);
- `args` — the exact binding this model was resolved from;
- `inputs` — content hashes of every manifest/launch/system-config file;
- `resolver` — tool + version;
- `diagnostics` — embedded checker warnings.

The `inputs` hashes make the model reproducible and give the cache key.

## Ownership + repository layout

| Piece | Lives in | Rationale |
|---|---|---|
| `SystemModel` types + serde + schema doc | `ros-launch-manifest` (new `model` crate) | both projects already vendor this repo; single schema |
| `resolve` (parse/bind/merge/check/emit) | play_launch (library fn + `play_launch resolve` verb) | it already owns the launch parser, manifest loader, and checker wiring |
| Linux runtime consumption (spawn + monitors) | play_launch | its runtime re-reads the model instead of re-parsing launch |
| Embedded consumption (bake per-image slice) | nano-ros (`nros` CLI + CMake seam) | maps layer 2+3 onto executor config, QoS, domain, codegen |
| Embedded runtime monitors | nano-ros executor | contract checks on-target (see below) |

Deployment assignment is **integrator-owned**: it is declared in the bringup
package's `system.toml` (a `[deploy]` section beside the existing
`[tiers.*]`), never in the component manifests — the component developer
declares contracts, the integrator decides placement.

## nano-ros consumption (sketch — phase doc will detail)

1. **Build side**: `nros` gains a verb to ingest `system_model.yaml` and emit
   each `mcu:*` node's slice: tier table → RFC-0015/0047 sched-context
   binding, QoS + budgets → per-endpoint config, domain/locator → boot
   config (RFC-0045 baked rung), wiring → topic names. The existing
   `system.toml`-driven flow becomes the degenerate no-contracts case.
2. **Runtime side**: the executor gains cheap on-target monitors driven by
   layer-2 numbers baked into the image — `max_age_ms` check at take (needs
   `header.stamp`), publish-rate/jitter accounting per callback-group tick.
   This replaces play_launch's rcl interception on targets where no rcl
   exists. Violations surface through the existing diagnostics path.
3. **Cross-machine budgets**: a scope path spanning MCU and Linux nodes is
   checked statically by the resolver's graph traversal; at run time each
   side monitors its local slice, and `max_age_ms` at the Linux subscriber
   catches the end-to-end total (age is E2E by construction).

### Input model; causality + execution modeling per consumer (2026-07-20)

**Supersedes the 2026-07-19 "SSoT structure" note** (maintainer decision).
The earlier note had `play_launch resolve` embed a *resolved scheduling
structure* (chains + per-path ranks) into the model. That is **reverted**
(the landed `model.execution.sched` / `ExecutionSched` — play_launch 45.2/45.3
— is removed; see the rework below). The settled split:

- **play_launch is a parser.** It gathers **all input** into the model —
  launch tree (nodes/wiring/topics), contracts (rates/ages/budgets/paths/QoS),
  system config, and the integrator's **declared** assignments (`deploy`,
  `tiers`, callback-group `bindings`). The model is the complete **input**;
  nothing resolved-for-execution lives in it. Phase-46 (Unified SystemModel —
  merging `record.json`/LaunchDump) is exactly this parser job and continues.
- **Causality + execution modeling is the consumer's job.** The chain/DAG
  resolution, scheduling, and task/thread creation are done by each
  **runtime**: play_launch's *Linux runtime* does its (chain_aware/PiCAS)
  modeling; nano-ros does its RTOS modeling (RFC-0052). Neither is embedded in
  the shared model — so **no `execution.sched` / no resolved chains / no
  per-path ranks in the model**.
- **The algorithm is shared, not the output.** `ros-launch-manifest-sched` is
  already the shared, pure crate (no parser deps) both runtimes vendor. The
  reusable value is the *algorithm*: split `chain_aware_mapper` into (a) a
  platform-agnostic **core** (feasibility + clock-segmentation + a priorityless
  chain/segment **ranking**) and (b) the Linux **realizer** (`rt_priority_band`
  → `ResolvedTierTable`). Both runtimes call (a); each applies its own realizer
  ((b) for play_launch's Linux runtime; an RTOS realizer for nano-ros, RFC-0052).
  **Derivation stays per-consumer**, sharing the `MapperInput` type — play_launch
  derives it from its `LaunchDump` (`sched_derive`, parser-coupled); nano-ros
  derives it from the SystemModel (`node_paths` + wiring + declared tiers). One
  agnostic core, two derivations, two realizers, zero embedded output.
- **Declared config stays.** `deploy`/`tiers`/`bindings` are integrator input
  the parser gathers; they stay in `execution:`. nano-ros reads them and
  models its RTOS execution on top (it does not treat them as a finished
  scheduling plan).
- **Runtime E2E monitoring stays stamp-based, no chain-id.** `age = now −
  header.stamp` at the sink (`sub_endpoints.max_age_ms`); the subscription
  topic disambiguates the budget; the message carries its own origin time. The
  one behavioral dependency is **stamp preservation**: a relay forwards the
  incoming `header.stamp`; a re-stamping node is modeled periodic
  (`input: []`), resetting the age clock by design.

**Rework items (2026-07-20):** (1) ~~revert `model.execution.sched` /
`ExecutionSched` + the `sched`-struct re-exports~~ **DONE** (rlm `f090400`);
(2) split `chain_aware_mapper` (in the already-shared `ros-launch-manifest-sched`)
into a platform-agnostic core + a `posix` Linux realizer, exposing a
priorityless ranked/segmented output; (3) nano-ros builds its own
`SystemModel → MapperInput` derivation + RTOS realizer over that core (phase-296
W5); play_launch keeps `sched_derive` + the Linux realizer.

## Non-goals

- Dynamic reconfiguration / mode switching inside a running model — resolve
  a second model and restart the affected subtree.
- Late binding inside the artifact (variables/conditions in the model) — see
  early-binding rationale.
- Automatic deployment placement — integrator-owned, explicit.
- Replacing the manifest format — manifests remain the authoring surface;
  the model is a derived artifact.

## Open questions

- Schema of the `[deploy]` system.toml section (per-node vs per-package vs
  per-callback-group granularity of placement).
- How much of layer 1 the embedded slice needs (full graph vs this node's
  neighborhood) — image size vs diagnosability.
- Whether `nros plan` (launch → orchestration plan) folds into SystemModel
  consumption or stays a separate lighter path during migration.

## References

- ros-launch-manifest `docs/launch-manifest.md` (format, phases 34–35) +
  `docs/contract-theory.md` (contract composition rules)
- play_launch `docs/design/system-model.md` (producer-side design, sibling
  of this RFC)
- RFC-0015 scheduling tiers, RFC-0045 boot-config resolution, RFC-0047
  sched-context binding, RFC-0048 CMake consumption (the seams the embedded
  slice lands on)

## Cross-track note — play_launch Phase 45 (Scheduling SSoT), 2026-07-18 — REVERTED

**Superseded 2026-07-20 (maintainer decision):** embedding the resolved sched
plan (chains + per-path ranks) into the model is **reverted** — see §"Input
model; causality + execution modeling per consumer." Phase-45.2/45.3 *landed*
(rlm `78f637d` etc., `model.execution.sched`) but are being backed out; the
algorithm is instead extracted into standalone reusable crate(s) each runtime
calls. play_launch is a parser; execution modeling is the consumer's job.
Phase-46 (unified **input** model, below) continues. Original note kept for
provenance:

play_launch is unifying its RT-scheduling track onto the SystemModel as the
single source of truth for scheduling: `resolve` embeds the full derived
sched plan (mapper identity, resolved FQN-qualified chains, per-path ranks)
into the model, and all consumers read it. The model's `execution:` layer
gains these as additive fields (Phase 45.2, shared `model` crate — joint
decision with this track). See play_launch
`docs/design/system-model-sched-ssot.md` +
`docs/roadmap/phase-45-sched_ssot_unification.md`, and RFC-0052's
cross-track note for the RTOS-consumer implications (per-path ranks feed
callback-granularity mapping).

## Cross-track note — play_launch Phase 46 (Unified SystemModel), 2026-07-20

play_launch is unifying its two artifacts into **one complete SystemModel**.
Today `resolve` emits `system_model.yaml` (portable) + `record.json`
(LaunchDump, Linux spawn detail), bound by `meta.record` sha256. Phase 46
merges them: the model carries ALL launch-derived info (+ optional contract +
system config), and each consumer derives its own platform specifics (Linux:
exec path via ament, argv, injected env; nano-ros: bake). `record.json`
retires; the user perceives one kind of dump. Design of record: play_launch
`docs/design/unified-system-model.md`; work breakdown:
`docs/roadmap/phase-46-unified_system_model.md`.

What this means for the nano-ros bake:

- **`<node machine=>` → `execution.deploy[fqn].host`** — the one field you
  genuinely need that the resolved model dropped (issue #236). play_launch is
  landing the fix now (Phase 46.1), ahead of the rest, to unblock the
  phase-296 R4 multihost migration. Your side (issue #236 step 3): the
  `host =` filter reads `deploy.host`.
- **`NodeInstance` gains launch fields** — `remaps`, `ros_args`,
  `respawn`/`respawn_delay`, launch-declared `env` — additive/optional, in the
  shared `model` crate. Your bake ignores the ones it doesn't consume (you
  already bake from resolved `structure.topics`, not raw remaps; you have no
  argv/respawn/env model). They ride along so the model is the one complete
  record of the launch. No `deny_unknown_fields`, so this is a safe staged
  rollout.

Asks: (1) confirm you've reviewed the unified-model design; (2) flag any field
your bake reads that the resolved model still omits (beyond `machine`, which is
being fixed) — so Phase 46 fills it while it reworks the launch→model path.

### nano-ros reply (2026-07-20)

**(1) Reviewed.** The unify-into-one-model + each-consumer-derives-its-own-
platform-specifics direction matches our canonical-path decision (RFC-0052
§Canonical path): the bake reads system semantics from the model and never
touches `record.json`. Merging the two artifacts and retiring `record.json`
removes the `meta.record` sha-pairing footgun for us — good. The additive
`NodeInstance` launch fields (`remaps`/`ros_args`/`respawn`/`env`) are inert to
our bake (we consume resolved `structure.topics`, not raw remaps; no
argv/respawn/env target) — the no-`deny_unknown_fields` staged rollout is safe
on our side. No objection.

**(2) One omitted field — an *unplaced* deploy target.** `machine → host` (46.1)
closes the field we named; #236 step 3 is done and validated. But 46.1 defaults
a machine-only deploy to `target: linux`, and that collides with board≠host
orthogonality: one host (`robot1`) can be realized by *different boards*
(`native_entry_robot1` vs `zephyr_entry_robot1`). A launch-only model has no
board placement, so a defaulted `target: linux` is ambiguous — our board slice
can't tell "explicitly on linux, a zephyr image must skip it" from "board
unplaced, the entry decides". Ask: when a deploy is derived from `machine`
alone (no system-config placement), emit an **unplaced** target — a new
`Target::Unspecified` (or make `Deploy.target` optional) — instead of defaulting
to `linux`. Then a launch-only model marks board as entry-determined and any
board's entry bakes its host's nodes; an explicit system-config `target: linux`
keeps its board-rejecting meaning. Detail + our consumer side in issue #236
(“Remaining sub-gap”). This is the only field blocking a board-heterogeneous
multi-host workspace from fully leaving the `launch =` arm.
