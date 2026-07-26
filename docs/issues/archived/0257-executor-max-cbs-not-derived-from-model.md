---
id: 257
title: "NROS_EXECUTOR_MAX_CBS is a hidden compile-time env the entry codegen could derive from the model"
status: resolved
type: enhancement
area: build
---

## Finding (autoware-safety-island-example P2/P3, 2026-07-24)

The executor callback-slot count is `NROS_EXECUTOR_MAX_CBS` (nros-node
build.rs env, default 4). A 3-node workspace registers ~9 entries → boot
died `create_timer (code=-6 Full)` with no build-time hint; the 4-node
island needed 32. Users discover the knob at runtime, and changing it
resizes the executor arena — mixed stale objects then SEGV in shutdown
(clean rebuild required).

The entry codegen consumes the SystemModel and KNOWS the per-node entity
counts (subs + services + timers + clients). It should derive the knob (or
at minimum emit a static_assert-style configure-time check: model entities
vs compiled capacity) instead of shipping a silent default-4.

Same class: `NROS_CYCLONEDDS_MAX_TYPES` (Rust registry) and the C++
descriptor registry cap (was silently-overflowing 64; raised + override in
the #253-adjacent fix).

## Executor part done (2026-07-26)

Counting + derivation live in ONE place both bakes call —
`nros_orchestration_ir::executor_sizing` (same non-drift rationale as
`board_path_for`):

- **Count** = the sliced node set's subs + service servers/clients + action
  servers/clients (one callback slot each). Publishers allocate no callback
  entry; param + lifecycle services live outside the callback arena. The model
  has NO timer/guard-condition entity, so the count is a LOWER BOUND — which
  is why the derivation adds headroom and the check only fires above capacity
  (never a false positive).
- **Derivation** `derived = counted + max(2, ceil(counted/4))` (25 %, floored
  at two slots), applied only when it lands above the build default, so a
  wiring-free model (every in-tree example today) emits byte-identical code.
  Explicit `[package.metadata.nros.entry] max_callbacks` still wins — but an
  explicit value BELOW the modelled count is now a hard bake error.
- **Loud check.** Only the hosted boards honor the per-entry sizing
  (`run_with_deploy_sized`); firmware boards drop it and open at the compiled
  `MAX_CBS`. For those, `nros::main!` emits a `const` assert against the REAL
  `nros::__macro_support::EXECUTOR_MAX_CBS`, so an over-capacity model fails
  to COMPILE, naming the count, and the `NROS_EXECUTOR_MAX_CBS` +
  clean-rebuild fix. `nros codegen-system` bails with the same numbers.

## Remaining

`NROS_CYCLONEDDS_MAX_TYPES` (Rust registry, default 32) is still a hidden
env; its overflow is at least loud at runtime (`dynamic_type.rs` names the
env). The distinct msg/srv types per entry ARE derivable from the model the
same way — unimplemented. The C++ descriptor registry cap needs nothing
(raised 64 → 256 + overridable + loud in `2092e7cff` / `ce186d35e`).

## Investigated + rejected (2026-07-26): exact count from source-metadata

The phase-172.E metadata mechanism (host harness links the component crate,
runs `Component::register` against `MetadataRecorder`, emits
`source-metadata.json`) DOES carry `timers` and would give an exact count —
its recorder and the runtime consume the same `EntityMetadata` declarations,
and per-kind slot accounting matches the arena (`spin.rs`: sub/timer/service
server+client/action server/guard = 1 slot each; publishers 0; param +
lifecycle service sets outside the arena).

It is nevertheless NOT a dependable bake input today:

1. **No automation.** `nros metadata --build` has zero call sites in `just/`,
   `cmake/`, `scripts/`, or the colcon extension — the books instruct the user
   to run it manually before `cargo build`. No ordering guarantee exists for
   the proc-macro bake, and manufacturing one means a nested `cargo run`
   during macro expansion (the 172.E docs explicitly defer that hardening).
2. **Wrong component shape.** `Workspace::component_declarations` enumerates
   only `nros.toml`/`component_nros.toml` `[component]` tables; the canonical
   lib-only Rust node pkgs declare `[package.metadata.nros.node]` and are
   never built for metadata (`book/src/getting-started/workspace-bringup.md`
   states the gap). The harness type path (`crate::module::Component`) is also
   stale vs the shipped `nros::node!(Class)` convention.
3. **No C/C++ producer.** The cmake `nros-metadata.json` carries
   name/pkg/class/class_header/shape/deploy/lang/callback_groups — no entity
   detail — so metadata-driven sizing would be Rust-only, forking the shared
   count the macro and CLI deliberately share.

Empirical: the only `*metadata.json` files in-tree are 9 hand-written test
fixtures; no example workspace ships one.

**Decision:** keep the model-wiring lower bound + headroom + the loud gate.
Recorded follow-ups if exactness is ever wanted: (a) revive 172.E — declare
`[package.metadata.nros.node]` pkgs, refresh the harness type path, add a C/C++
producer, wire the verb into the build; then (b) fold metadata in as
`max(model_count, metadata_count)` (monotone, never false-positive), cheapest
first in the CLI bake, which already scans the workspace
(`Workspace::source_metadata_files`).

Remaining 0257 scope is unchanged: `NROS_CYCLONEDDS_MAX_TYPES`.

**Phase-307 drafted (2026-07-26)** to do exactly the revival above:
W1 producer covers the shipping `[package.metadata.nros.node]` Rust shape
(the summaries are already parsed — just never turned into declarations)
+ harness type-path refresh; W2 automation so a bake has an ordering
guarantee (prefer a `nros ws sync`/`plan` refresh step over a
proc-macro-time nested cargo); W3 a C/C++ producer built on phase-235's
already-recording NativeBoard NodeContext (coordinate with phase-236);
W4 folds it in as `max(model, metadata)` with the tier-group filter and
closes this issue. See `docs/roadmap/phase-307-metadata-mode-completion.md`.

## Resolved (2026-07-26) — phase-307

The executor-sizing scope is closed. The "Decision" above (keep the
model-wiring lower bound, exactness not worth it) was overturned by doing
the revival it listed as a follow-up, and the three blockers it recorded
all fell:

1. **Nothing produced sidecars automatically.** `nros sync` now refreshes
   them as its last step (after codegen + the `[patch.crates-io]` tables,
   which the harness needs to resolve generated interface deps), with a
   content-addressed provenance stamp so a consumer can tell a current
   sidecar from museum data. No nested cargo runs during proc-macro
   expansion — the trap that killed the naive approach — because the
   producer is a build step and the macro only READS a file.
2. **The producer did not cover the shipping shape.** It does now:
   `[package.metadata.nros.node]` packages become component declarations,
   and the harness names the registered type from the declared `class`
   instead of guessing `<crate>::<module>::Component`.
3. **No C/C++ producer.** Still true, and still the reason this issue's
   fix is Rust-only — tracked as
   `docs/roadmap/phase-308-cpp-metadata-producer.md`. C/C++ node packages
   keep the model bound, which is exactly today's behaviour, so nothing
   regresses.

Consumption is `max(model_wiring, recorded)` PER NODE, in the shared
`nros-orchestration-ir::count_callbacks_with_recorded` that both the CLI
bake and the `nros::main!` macro call. The max is necessary, not just
safe: measuring real sidecars showed the model misses TIMERS and the
recorder misses service/action CLIENTS. Monotone, so a workspace with no
sidecars produces exactly the pre-fix result.

Regression coverage the issue never had:

* `nros-cli-core/tests/executor_sizing_bake_gate.rs` — an over-capacity
  system fails at BAKE and names the count ("7 callback entities … holds
  4") instead of shipping an image that dies at boot.
* `examples/workspaces/ws-sizing-rust` + `nros-tests/tests/executor_sizing_e2e.rs`
  — a six-timer node the model counts as ZERO. With its sidecar removed
  the entry dies `ExecutorFull("burst_pkg")`, which is this issue's
  reported failure reproduced in-tree; with it, the entry boots.

**Remaining scope, unchanged:** `NROS_CYCLONEDDS_MAX_TYPES` is still a
hidden compile-time knob. Distinct msg/srv types per entry are derivable
from the same source-metadata the sizing now uses; filed separately
rather than holding this issue open, since the callback-slot failure
mode it was opened for is fixed.
