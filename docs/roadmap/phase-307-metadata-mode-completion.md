# Phase 307 — metadata-mode completion: make the exact entity count real

**Status (2026-07-26): Draft.** Finishes the phase-172.E metadata-mode
*driver* (landed 2026-05-28) into a mechanism the bakes can actually depend
on, and consumes it where exactness matters. Motivated by issue 0257: the
executor callback-table sizing currently derives from the SystemModel's
wiring layer and is a documented LOWER BOUND, because the model has no
timer entity — while `source-metadata.json` already carries `timers` and
would be exact. See the "Investigated + rejected" section of
`docs/issues/archived/…0257…` (or the open issue) for the evidence trail.

**172.E itself is NOT this phase.** That item was the *sandbox hardening*,
dropped 2026-05-29 as security theater (metadata-mode components are the
user's own local crates, and cargo already runs their build scripts +
proc-macros unsandboxed before extraction). Its **driver** landed and works.
What is missing is producer coverage and automation — this phase.

## Why the count would be exact

The recorder and the runtime consume the SAME `Component::register`
declarations: `record_node_metadata::<C>` → `register_node::<C>` →
`C::register(&mut NodeContext)`, versus the live `register_node_borrowed` →
`ExecutorSink::create_entity`, both switching on the same `EntityMetadata`.
Per-kind slot accounting checks out against the arena (`spin.rs`,
`executor/action.rs`): subscription / timer / service server / service
client / action server / guard condition = 1 slot each; publishers = 0;
parameter + lifecycle service sets register outside the callback arena.

Known deltas to handle rather than ignore:
- **Tier gating** — `create_entity` early-returns for entities whose
  callback group is inactive on the running tier; the recorder has no such
  gate, so a multi-tier entry's metadata OVER-counts. Filterable via the
  recorded `group` field (over-count is the safe direction regardless).
- **Host-vs-target cfg** — the harness compiles the component on the HOST
  with `nros[std]`; `#[cfg(target_os)]` / feature-gated `create_*` calls
  record differently than the firmware build. Must be documented as a
  contract on component authors (declare unconditionally, gate behavior not
  declaration) or detected.
- **Non-declarative registrations** — board glue / macro-emitted timers and
  guard conditions bypass `register()` and are invisible to BOTH mechanisms.
  They are also a fixed, known set: count them in the bake as a constant.

## Waves

### W1 — producer covers the shipping Rust shape

`Workspace::component_declarations` enumerates only `nros.toml`-style
`[component]` tables plus the cmake summaries; the canonical lib-only Rust
node pkgs declare `[package.metadata.nros.node]`, are already PARSED into
`Package::cargo_component_metadata` (`discover_cargo_component_metadata`),
and are simply never turned into declarations. Add the third loop in the
same dedup pass (mirroring the phase-219.L cmake loop) with a
`cargo_summary_to_component_config`. Also refresh the harness type path:
`component_type_path` assumes `crate::module::Component`, while the shipped
convention is `nros::node!(Class)` with `impl Node for Class` — derive the
path from the summary's `class` instead of a positional guess.
**Done when:** `nros metadata --build` produces a sidecar for an unmodified
`examples/workspaces/rust` node pkg.

### W2 — automation (the ordering guarantee)

Today `nros metadata --build` has ZERO call sites outside the books — no
`just` recipe, no cmake hook, no colcon step — so no bake can rely on it.
Decide and implement ONE producer trigger:
- (a) a `nros ws sync` / `nros plan` step that refreshes stale sidecars
  (mtime vs component sources), or
- (b) a cargo/cmake build-graph edge for entry targets.
(a) is strongly preferred: it keeps the nested-cargo invocation out of
proc-macro expansion (the trap that killed the naive 0257 approach) and
fits the existing "sync then build" workflow. Sidecars must carry a
staleness stamp so a bake can tell fresh from museum data.
**Done when:** the documented workflow produces sidecars without a manual
verb, and a stale sidecar is detectable.

### W3 — C/C++ producer

The cmake `nros-metadata.json` is a (pkg, exec) → class/header/groups map
with no entity detail, so C/C++ components can never contribute a count
today. The C++ side already has the raw material: phase-235's NativeBoard
builds a *recording* NodeContext (`nros-cpp/include/nros/main.hpp`) whose
ops are no-ops — that is a metadata recorder in embryo. Emit its recording
into the same `source-metadata.json` schema from a host-compiled probe,
mirroring the Rust harness. **Coordinate with phase-236** (which turns that
recording context into a real runtime — the two must not fork).
**Done when:** a C++ node pkg produces a schema-valid sidecar.

### W4 — consume it: exact sizing

Fold metadata into the 0257 derivation as
`max(model_wiring_count, metadata_count)` — monotone, never regresses an
existing build, never false-positives — with the tier-group filter from
"Known deltas". Land it in the CLI bake first (`codegen-system` already
scans the workspace: `Workspace::source_metadata_files`), then in the macro
IF W2 gives a real ordering guarantee; otherwise the macro keeps the bound
and the CLI bake carries the exact gate. Same treatment for the sibling
knob `NROS_CYCLONEDDS_MAX_TYPES` (0257's remaining scope): distinct
msg/srv types per entry are derivable from the same source.
**Done when:** an over-capacity workspace fails at bake naming the exact
count, and 0257 closes.

## Non-goals

- Sandboxing the harness (the dropped 172.E item — re-open only under a
  hosted build-service threat model).
- Making the recorder see dynamically-created entities: nothing outside
  `register()` reaches the declarative path at runtime either.

## Acceptance

- [ ] Unmodified Rust + C++ example node pkgs both produce sidecars.
- [ ] Sidecars are produced by the documented workflow, not a manual verb,
      and staleness is detectable.
- [ ] The executor-capacity gate uses the exact count where a sidecar
      exists (timers included) and the model bound where it does not.
- [ ] 0257 closes; the "why not metadata?" note in
      `nros-orchestration-ir/src/executor_sizing.rs` is replaced by the
      real rule.
