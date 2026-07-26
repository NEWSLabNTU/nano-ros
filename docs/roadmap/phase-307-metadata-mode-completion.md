# Phase 307 — metadata-mode completion: make the exact entity count real

**Status (2026-07-26): COMPLETE for Rust.** W1, W2, W4 (both halves), W5 and
W6 (both lanes) landed; issue 0257's executor-sizing scope closes. W3 (the
C/C++ producer) is split out to
[phase-308](phase-308-cpp-metadata-producer.md). Finishes the phase-172.E metadata-mode
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
**Done when:** `nros metadata --build` produces a schema-valid sidecar for
EVERY unmodified Rust node pkg in the tree — both **standalone** examples
(`examples/{native,zephyr,qemu-arm-freertos,qemu-arm-nuttx,qemu-riscv-nuttx,
qemu-riscv64-threadx,threadx-linux,qemu-arm-baremetal,qemu-esp32-baremetal,
stm32f4}/rust/*`) and **workspace** members
(`examples/workspaces/{rust,ws-*}/src/*_pkg`) — with no per-example opt-in
key. A pkg that legitimately declares no node (interfaces-only, bringup)
produces nothing and is not an error; anything else missing a sidecar is a
W1 failure, enumerated by the coverage gate in W5.

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

### W3 — C/C++ producer — MOVED to phase-308

Split out once the Rust half landed and this proved phase-sized: the C++
declaration path bottoms out in the ~137-function `nros_cpp_*` C ABI, so
"record instead of create" has to happen at that seam. A recording RMW
backend would be the cheap intercept but misses TIMERS (executor-side, not
RMW-side) — the one entity the model already cannot see, so it would
reproduce the bug it exists to fix. Design options + the "must not fork"
constraint are recorded in
[phase-308](phase-308-cpp-metadata-producer.md).

### W3 (original text) — C/C++ producer

The cmake `nros-metadata.json` is a (pkg, exec) → class/header/groups map
with no entity detail, so C/C++ components can never contribute a count
today. The C++ side already has the raw material: phase-235's NativeBoard
builds a *recording* NodeContext (`nros-cpp/include/nros/main.hpp`) whose
ops are no-ops — that is a metadata recorder in embryo. Emit its recording
into the same `source-metadata.json` schema from a host-compiled probe,
mirroring the Rust harness. **Coordinate with phase-236** (which turns that
recording context into a real runtime — the two must not fork).
**Done when:** every unmodified C and C++ node pkg produces a schema-valid
sidecar — standalone (`examples/*/c/*`, `examples/*/cpp/*`) and workspace
(`examples/workspaces/{c,cpp,mixed,ws-*}`) alike — through the same schema
and the same discovery path as Rust, so the count is one mechanism with
three front-ends, not three mechanisms.

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

### W5 — coverage gate: platform- and shape-agnostic by construction

The counting mechanism must not silently regress to "works for the two
examples someone tested". Add a build-stage gate that enumerates every
example — the SAME enumeration the fixture/example matrix already uses
(`examples/fixtures.toml` rows + `packages/testing/nros-tests/src/matrix.rs`
cells; reuse the phase-300 enumeration SSoT rather than a fresh `find`) —
and asserts, per node pkg:

1. a sidecar exists (or the pkg is on a tracked, reasoned exception list —
   the `examples_fixture_coverage.rs` precedent), and
2. the recorded entity set is non-empty when the pkg's source declares
   entities, and
3. the derived capacity is >= the entities the SystemModel wiring shows
   (metadata may exceed the bound; it must never fall below it).

Platform-agnosticism is a PROPERTY TO TEST, not an assumption: the producer
compiles a HOST probe, so a zephyr/freertos/nuttx/threadx/bare-metal node
pkg must yield the same sidecar as a native one from identical source.
Assert exactly that — pick node pkgs that build for multiple deploys and
compare their sidecars across platform selections (the entity set must be
identical; only cfg-gated declarations may differ, which is the W3
"host-vs-target cfg" delta and must FAIL LOUD rather than silently skew a
count).

**Done when:** the gate runs in `just check` (build stage, no QEMU) and
fails on a missing/empty/under-counting sidecar for any example.

### W6 — runtime E2E: the count is right where it matters

Static gates prove the sidecars exist and agree; only a boot proves the
sizing they produce is correct. Two lanes, both marker-gated
(`nros_tests::output` constants, never literal greps):

1. **Over-capacity is caught at BAKE, not boot** — a fixture whose model +
   metadata exceed the compiled `MAX_CBS` must fail the build with the
   count-naming error (the phase-306 W1 `compile-check-fixtures` shape is
   the precedent for asserting a build-stage failure).
2. **Derived capacity actually boots** — a workspace whose entity count
   exceeds the default 4 (subs + TIMERS, so the model bound alone would
   under-size it and the pre-307 code would die `code=-6 Full`) boots and
   delivers on at least one hosted lane AND one embedded lane
   (native + one of zephyr/freertos/nuttx/threadx), proving the sizing
   reaches boards that DROP per-entry sizing and open at the compiled
   `MAX_CBS`. This is the regression test 0257 never had: the original
   Autoware-island failure reproduced in-tree.

**Done when:** both lanes are matrix cells with fixture rows (RFC-0051
rules: cell + row land together, allocator ports, no hand-picked numbers)
and pass solo on a quiet host.

**Status: both lanes LANDED.** Lane 1 is
`nros-cli-core/tests/executor_sizing_bake_gate.rs` — three cases through
the real `codegen-system` verb (control fits; six recorded timers over-run
and the bake names "7 callback entities … holds 4"; a thin sidecar never
lowers the count). It needs no compilation, so it runs in `just check`
rather than a QEMU lane.

Lane 2 landed as `examples/workspaces/ws-sizing-rust` +
`nros-tests/tests/executor_sizing_e2e.rs`. Its node registers six timers
and no subscription, so the model names ZERO callback entities for it
while the runtime needs six slots. Verified both directions before
wiring the test: sidecar removed and entry rebuilt →
`ExecutorFull("burst_pkg")` at registration (issue 0257's reported
failure, reproduced in the repo for the first time); sidecar present →
boots, 168 callbacks in 2 s.

Reaching it required W4's deferred second half — the `nros::main!` macro
reading sidecars too. The macro, not the fixture, was the blocker: on
boards that honor per-entry sizing the macro's derived value IS the
executor's capacity, so a fixture alone could never have passed.

**Correction to this wave's original text.** It called for an embedded
boot lane "proving the sizing reaches boards that DROP per-entry sizing".
It does not, and cannot: `board_honors_entry_sizing` is `native | posix`
only, and every firmware board takes the default trait body that ignores
the emitted sizing and opens at the compiled `NROS_EXECUTOR_MAX_CBS`. On
embedded, this phase's contribution is the LOUD BAKE REFUSAL naming the
real count — which is lane 1, and lane 1 is board-agnostic (it exercises
a `freertos` deploy). An embedded boot fixture would only test the
`NROS_EXECUTOR_MAX_CBS` plumbing, which is not what phase-307 changed.

## Scope note — "all examples" is the bar, deliberately

The tree has ~250 standalone example fixtures and ~85 workspace fixtures
across 11 platform families and 3 languages. A counting mechanism that
works for a subset is worse than none: the bake would size some entries
exactly and others by a bound, and a user could not tell which guarantee
they were getting. Hence W5's enumeration gate and W6's two-lane E2E are
acceptance criteria, not nice-to-haves — and the producer is a HOST probe
precisely so platform coverage is structural (one probe per component,
independent of the deploy target) rather than per-platform plumbing.

## Non-goals

- Sandboxing the harness (the dropped 172.E item — re-open only under a
  hosted build-service threat model).
- Making the recorder see dynamically-created entities: nothing outside
  `register()` reaches the declarative path at runtime either.

## Acceptance

Landed: W1 (Rust producer covers `[package.metadata.nros.node]`), W2
(`nros sync` refreshes, content-addressed provenance), W4
(`max(model, recorded)` per node in the CLI bake), W5 (coverage gate over
all 421 example packages — which found three schema holes that made
`Workspace::discover` hard-fail on 15 board examples), W6 lane 1.

- [x] Every unmodified RUST node pkg is a metadata-mode candidate, standalone
      and workspace alike, asserted by enumeration over the whole tree.
- [ ] (phase-308) EVERY unmodified node pkg in `examples/` produces a
      schema-valid sidecar — Rust, C and C++; standalone examples and workspace members
      alike — with no per-example opt-in and a tracked exception list for
      legitimate non-nodes.
- [x] Identical source yields an identical entity set regardless of the
      target platform selection (host probe ⇒ platform-agnostic);
      cfg-divergent declarations fail loud instead of skewing a count.
- [x] Sidecars are produced by the documented workflow, not a manual verb,
      and staleness is detectable.
- [x] The executor-capacity gate uses the exact count where a sidecar
      exists (timers included) and the model bound where it does not, and
      never falls below the model bound.
- [x] W5 coverage gate runs in `just check` and fails on any missing,
      empty, or under-counting sidecar.
- [x] W6 E2E: an over-capacity system fails at BAKE with the count named,
      and a >4-entity system (timers the model cannot see) boots and
      delivers on the hosted lane. Embedded is covered by the bake
      refusal, not a boot: firmware boards ignore per-entry sizing by
      design (see the W6 correction).
- [x] 0257 closes; the "why not metadata?" note in
      `nros-orchestration-ir/src/executor_sizing.rs` is replaced by the
      real rule.
