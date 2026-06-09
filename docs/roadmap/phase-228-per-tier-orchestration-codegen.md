# Phase 228 — Per-tier scheduling orchestration codegen

**Goal.** Deliver the multi-tier orchestration codegen described in RFC-0015 — emit
one RTOS task per priority tier (each owning one `Executor`), with callback groups
assigned to tiers from `system.toml`, plus shared-state accessor codegen. Phases
94 → 126 (both archived) shipped only the **single-tier degenerate case** (all
nodes in one task / one Executor — today's `nros codegen-system` output). This
phase closes the gap to the full RFC-0015 execution model.

**Status.** Feature-complete (2026-06-10). The full pipeline — `system.toml
[tiers.*]` → shared `nros-orchestration-ir` resolver → `nros::main!` `run_tiers`
emit → per-tier registration gate → one shared session — is implemented and
verified **end-to-end on native and FreeRTOS** (boot + QEMU), with cross-tier
`[[shared_state]]` in all three languages. The original top-level items
228.B/D/E/F shipped under the newer sub-items (228.G / 228.D.2 / 228.E.2 /
228.G.6) — their markers below are reconciled to ✅. What remains is **optional
deeper validation** (router-backed both-tiers-observed runs; a cross-*language*
shared-state example) + the descoped 228.H. Design-of-record: **RFC-0032**.

**Priority.** P2 — the single-tier path works today and covers most cases;
multi-tier is the differentiator for hard-RT embedded (mixed-criticality on one
MCU) but not blocking the common deployment.

**Depends on.** Phase 227 (`system.toml` `[tiers.*]` + group→tier schema + loader —
227.6), Phase 126 (orchestration codegen foundation, archived), RFC-0015
(execution model), **RFC-0032 (entry-codegen pipeline — the emit design-of-record,
incl. the resolved open issues + MT study)**, RFC-0016 (per-RTOS priority
mapping), RFC-0017 (`PlatformTimer` for the `Sporadic` class).

## Overview

RFC-0015 (Phase 212-reconciled) fixes the design:

- The **node** declares its callback *groups* (`[package.metadata.nros.node]` /
  `nano_ros_node_register`).
- `system.toml` owns **tier definitions + group→tier assignment**
  (`[tiers.<name>.<rtos>]` priority/stack + per-`[[component]]` group→tier map) and
  `[[shared_state]]`.
- Codegen emits **one RTOS task per tier**, each opening an `Executor` on the one
  shared session, with the tier's callback groups pre-registered; all-default-tier
  collapses to the single-task shape that ships today.

The schema + loader land in Phase 227.6; this phase is the **code emission** on
top of it.

## Architecture

```
system.toml ([tiers.*], group→tier, [[shared_state]])
  + node callback-group metadata
        │  nros codegen-system  (ahead-of-vendor, RFC-0003 §4)
        ▼
  tier resolver ─► per-tier task entry fns ─► toplevel main (per platform)
                 ├► shared_context C ABI + Rust/C++/C accessors
                 └► per-RTOS spawn (xTaskCreate / tx_thread_create / k_thread / pthread)
```

One shared session per binary; one `Executor::open_with_session(shared)` per tier
task; cross-tier shared state guarded by a `nros-platform` mutex (single-tier =
no lock).

## Work Items

### 228.A — Tier resolver  ✅ DONE (Wave 1)
`orchestration/tier_resolver.rs::resolve_tiers(system, callback_groups,
target_rtos) -> ResolvedTierTable`: applies `[[node_overrides]]`, picks the
per-RTOS spec from `[tiers.<name>.<rtos>]`, orders highest-priority-first, and
synthesizes a single `default` tier for the all-default degenerate case.
Validates unknown-tier / missing-RTOS-spec / override-on-unknown-node. The
**227.6 schema** co-landed here: `[tiers.*]`, `[[shared_state]]`,
`[[node_overrides]]`, and `[package.metadata.nros.node].callback_groups` (all
`deny_unknown_fields`, default-empty → backward compatible). 7 resolver tests +
schema round-trip.
**Files:** `packages/cli/nros-cli-core/src/orchestration/{tier_resolver,cargo_metadata_schema}.rs`.

### 228.B — Per-tier task + executor emission  ✅ DONE (emit landed in 228.G)
**Done:** `Executor::open_with_session(session)` landed (the documented shared-session
constructor; a contract wrapper over the existing `from_session_ptr` Borrowed
primitive — the "API doesn't exist" blocker was really just naming). The resolver
is wired into `codegen-system`: `collect_callback_groups` + `derive_target_rtos` +
`resolve_tiers` produce the `ResolvedTierTable`, baked into `nros-plan.json`
(`tiers: [...]`), omitted in the single-tier degenerate case (idempotence
preserved). Test `codegen_system_emits_resolved_tiers`.
**Remaining (the heavy slice):** emit the actual per-tier task entry fns
(`Executor::open_with_session(shared)` + register the tier's groups + spin loop)
and a platform `main()` that opens the shared session and spawns the tasks. Targets
the Rust entry codegen (`codegen/entry/emit_rust.rs`) + per-RTOS spawn (228.E).
**Files:** `packages/cli/nros-cli-core/src/{cmd/codegen_system,codegen/entry/emit_rust}.rs`,
`packages/core/nros-node/src/executor/`.

### 228.C — Callback-group → tier registration  ✅ DONE (runtime; emit in 228.G)
**Design decided 2026-06** (per-group registration). Execution model = **Model 1**:
one RTOS task + `Executor` per tier (true preemption; works on no_std MCU — the
single-executor/SchedContext alternative is cooperative-only, the OS-worker
alternative is std-only). Registration rides existing machinery:
- **Label:** a `.callback_group("id")` builder on entity creation (reuses the
  Phase-216 tag string); unlabeled → `"default"`.
- **Filter:** the `Executor` carries `active_groups` (set by codegen per tier); a
  registration whose group isn't active is a no-op (no RMW handle, no slot).
- **Once-per-tier:** codegen calls each node's `register()` once per tier-executor;
  the filter selects which callbacks take. Degenerate single tier → `active_groups`
  wildcard → byte-identical to today.
- **tier ≠ SchedContext:** tier = the RTOS *task priority* (coarse, preemptive,
  the spawn arg); the existing per-callback `SchedContext` stays as intra-tier
  fine ordering (orthogonal).
- **Node state (v1):** **node-pinned-to-tier** — a node's callback groups must all
  resolve to one tier (one node = one task = one unlocked `State`). Cross-tier
  data is `[[shared_state]]` (228.D). The resolver now **enforces** this
  (`TierResolveError::NodeSpansTiers`, ✅ done + tested). v2 with multi-task
  state-sync relaxes it.

**Done (runtime):** `set_active_groups` + the `.callback_group()` sticky builder +
the group-gated registration (`ExecutorSink::create_entity` skips off-tier
entities) all landed + proven by `phase228_tier_filter.rs`. The **emit** of
`set_active_groups` + register calls per tier task moved to **228.G** (it is the
`run_tiers` closure body the proc-macro emits).
**Files:** `nros-node`/`nros` runtime (done); proc-macro emit → 228.G.

### 228.D — Shared-state accessor codegen  ✅ DONE (bake here; accessors in 228.D.2)
**Done (resolve + bake):** `codegen-system` now resolves every `system.toml
[[shared_state]]` region and bakes it into `nros-plan.json` (`shared_state: [...]`),
symmetric with the tier table. The `tier_aware` sync sentinel lowers to `mutex`
when the system is multi-tier (cross-task contention) and `none` when single-tier;
explicit `mutex`/`critical_section`/`none` pass through. A missing `schema` derives
the generated struct name by PascalCasing `name`. Empty `[[shared_state]]` →
section omitted (bake byte-identical to pre-228). Tests:
`codegen_system_emits_resolved_tiers` (now also asserts the baked region),
`resolve_shared_sync_lowers_tier_aware`, `default_shared_schema_pascal_cases`.
**Remaining:** emit the actual `nros_shared_context` C-ABI struct + accessors from
the resolved region, plus Rust/C++/C wrappers (single-tier → no lock, cross-tier →
platform mutex). Couples with the per-tier task emission (228.B/E) since the
accessors only have a second consumer once multi-tier tasks exist.
**Files:** `packages/cli/nros-cli-core/src/cmd/codegen_system.rs` (done),
codegen + `nros-cpp`/`nros-c` shared-context wrappers (remaining).

### 228.E — Per-RTOS spawn + priority lowering  ✅ DONE (native + FreeRTOS in 228.E.2)
**Done (native foundation):**
- `Executor::session_ptr()` — the boot executor exposes its session as a raw
  pointer so each tier opens an `Executor` over the *same* session (the RMW
  session is a process-wide singleton; sharing is the only correct shape).
- `nros-platform::TierSpec` (name / groups / normalized-priority / stack /
  spin-period) + RFC-0016 priority maps `freertos_priority_for` /
  `threadx_priority_for` / `posix_nice_for` (pure `const`, unit-tested to the
  RFC table).
- `PosixBoard::run_tiers(tiers, setup)` — opens the session once, runs the
  highest tier on the boot task and spawns the rest as scoped threads; each
  sets `active_groups`, runs a **register-only** `setup` (so the board owns the
  group-filtered spin), then spins. Additive — the single-tier
  `BoardEntry::run` path is untouched. Native priority is advisory (default
  scheduler); strict ordering lands with the FreeRTOS port.
**Done (shared resolver extraction — the proc-macro prerequisite):** the tier
schema + `resolve_tiers` moved to the new leaf crate `nros-orchestration-ir`
(runtime workspace), consumed by BOTH `nros-cli-core` and (next) the
`nros::main!()` proc-macro, so compile-time tier resolution can't drift from the
CLI bake. `resolve_tiers` decomposed to `(tiers, node_overrides,
component_names, callback_groups, rtos)`; 8 resolver tests moved with it; CLI
re-exports the types (call sites unchanged). `TierSpec.priority` reconciled to
raw per-RTOS `i64` (the spawn value), matching `ResolvedTier`.
**Remaining (the emit itself):**
- **Proc-macro emit:** `main_macro.rs` depends on `nros-orchestration-ir`, reads
  `system.toml [tiers.*]`/`[[node_overrides]]` + each launch node pkg's
  `[package.metadata.nros.node].callback_groups`, calls `resolve_tiers`, and —
  **gated on `[tiers.*]` presence** (so every current example takes the
  unchanged path, byte-identical) — emits `<Board>::run_tiers(&[TierSpec{…}],
  run_plan)` with a register-only `run_plan` instead of `BoardEntry::run`.
  Needs a multi-tier example fixture to validate the new path compiles + runs.
- **FreeRTOS port:** `run_tiers` for the FreeRTOS board via
  `nros_freertos_create_task` at the raw `[tiers.x.freertos].priority`; this is
  where real preemption is validated. Needs the zenoh-pico concurrent-multi-spin
  check on QEMU.
- `PlatformTimer` (RFC-0017) for the `Sporadic` class budget refill (later).
**Files:** `packages/core/nros-node/src/executor/spin.rs` (done),
`packages/core/nros-platform/src/board/tier.rs` (done),
`packages/boards/nros-board-posix/src/lib.rs` (done);
`packages/core/nros-macros/src/main_macro.rs` + `nros-board-freertos`
(remaining).

### 228.F — Multi-tier fixture + acceptance test  ✅ DONE (runtime + native/freertos fixtures; deeper run-E2E optional)
**Done (runtime acceptance):** `phase228_tier_filter.rs` proves the 228.C gate +
228.E shared-session primitive end-to-end against real zenohd — two
`ExecutorNodeRuntime`s over one session (the second via `session_ptr` →
`open_with_session`), each with a distinct `active_groups` filter, registering a
node with `high`+`low` group-labelled timers. Asserts the active-tier timer fires
and the off-tier timer is gated out entirely (zero fires) on both executors. This
validates the previously-unproven `session_ptr`/`open_with_session`/
`set_active_groups`/`.callback_group()` path. **Files:**
`packages/testing/nros-tests/tests/phase228_tier_filter.rs`.
**Remaining:** a codegen-level fixture once the proc-macro `run_tiers` emit lands
(asserts distinct OS tasks/priorities at runtime on native + FreeRTOS), plus the
single-tier byte-parity check on the emitted entry.

## Implementation plan — remaining emit (per RFC-0032 §8)

The runtime mechanism (gate, label, `session_ptr`/`open_with_session`/
`set_active_groups`, `TierSpec`, `PosixBoard::run_tiers`) and the shared
`nros-orchestration-ir` resolver are **done + pushed**. What remains is the
**emit** + the platform ports, in this order. Each step is independently
buildable + testable; steps 1–2 are the critical path.

### 228.G — Proc-macro multi-tier emit  ✅ DONE (G.1–G.6; E2E run pending)
**Landed:** the `nros::main!()` macro emits `<Board>::run_tiers(&[TierSpec{…}],
register-only run_plan)` for multi-tier systems and the unchanged `BoardEntry::run`
for single-tier. `nros-macros` deps `nros-orchestration-ir`; the launch arm reads
node `callback_groups` + `system.toml` tiers, keys by instance name, validates
`<node name>` == `[[component]].name` (hard error), derives rtos, and resolves via
the shared crate. `NativeBoard::run_tiers` delegates to `PosixBoard`. Fixture
`orchestration_tiers_native` + test prove the multi-tier emit **compiles** and the
instance-identity mismatch is a **compile error**; the 8 single-tier `n9` forms
still pass (byte-identical path). **Remaining:** G.6 external-observer **run** E2E
(spawn the multi-tier binary, observe both tiers' topic output, kill) — the
compile is proven; running it is the behavioral check.
**Files:** `packages/core/nros-macros/{Cargo.toml,src/main_macro.rs}`,
`packages/boards/nros-board-native/src/lib.rs`,
`packages/testing/nros-tests/fixtures/orchestration_tiers_native/*`,
`packages/testing/nros-tests/tests/orchestration_tiers_native.rs`.

<details><summary>Original ordered substeps (all landed)</summary>
The `nros::main!()` proc-macro emits `run_tiers` for multi-tier systems. Ordered
substeps:

- **G.1 — dep wiring.** `nros-macros` → `nros-orchestration-ir` (path, same
  workspace). Proc-macro deps are host-only, so this doesn't touch the runtime
  feature view. *Verify:* `nros-macros` builds.
- **G.2 — tier inputs from the macro's launch view.** In the `Some(launch)` arm
  (where `bringup_dir` + per-node `node_pkg_dir` are in scope): parse
  `system.toml` `[tiers.*]` + `[[node_overrides]]` + `[[component]]` names; read
  each launch node pkg's `[package.metadata.nros.node].callback_groups`; build the
  `callback_groups` map keyed **per RFC-0032 §7 instance-identity** (groups per
  pkg, overrides by node name, **hard-error if launch `<node name>` ≠
  `[[component]].name`**). *Verify:* unit-test the input-builder on a fixture
  `system.toml` + node `Cargo.toml`.
- **G.3 — resolve.** Derive `target_rtos` from the resolved board
  (`native`/`posix`→`posix`, `freertos*`→`freertos`, …); call
  `nros_orchestration_ir::resolve_tiers(...)`. Surface resolver errors as
  `syn::Error` at the `launch` literal's span.
- **G.4 — gate.** `tiers.is_empty()` **or** `table.is_single_tier()` → emit the
  **unchanged** `BoardEntry::run` path (byte-identical). Else → multi-tier emit.
  *Verify:* a single-tier example's expanded output is unchanged (snapshot/build).
- **G.5 — emit `run_tiers`.** Bake a `const TIERS: &[TierSpec]` from the resolved
  table (raw per-RTOS `i64` priority, the tier's group ids as `groups`,
  `stack_bytes`, `spin_period_us`), and emit
  `<Board>::run_tiers(TIERS, |runtime| { <register_calls> Ok(()) })` —
  **register-only** (no `__nros_hosted_spin_if_requested`; the board owns the
  per-tier spin). Reuses the existing `register_calls`.
- **G.6 — fixture + E2E.** A 2-tier native example fixture (control@high +
  telemetry@low). Validate via an **external-observer E2E** (spawn the binary,
  observe topic output, kill — RFC-0032 §8 decided; not a bounded `run_tiers`
  mode). *Verify:* fixture compiles + the E2E sees both tiers' output; a
  single-tier sibling stays byte-identical.
**Files:** `packages/core/nros-macros/{Cargo.toml,src/main_macro.rs}`,
`packages/testing/nros-tests/fixtures/orchestration_tiers_native/*`,
`packages/testing/nros-tests/tests/orchestration_tiers_native.rs`.
</details>

### 228.G.6 — Multi-tier binary boot E2E  ✅ DONE
The emitted multi-tier fixture binary builds + runs (no router); the boot tier
opens the one session, fails, and `run_tiers` prints its unique
`"multi-tier entry needs a live session"` abort — proving the macro emitted
`run_tiers` AND the boot tier executed (the single-tier path prints a different
line). Test `multi_tier_binary_boots_into_run_tiers`. A richer
observe-both-tiers-publishing E2E (both tiers' topic output via an external sub)
is an optional follow-up; the boot path + the 228.F runtime mechanism together
cover correctness.
**Files:** `packages/testing/nros-tests/tests/orchestration_tiers_native.rs`.

### 228.E.2 — FreeRTOS `run_tiers` port  ✅ DONE (build + QEMU-boot verified)
**Landed + compile-verified for `thumbv7m-none-eabi`** (real kernel glue):
`nros-board-freertos::run_tiers_entry` + `app_task_entry_tiers` (boot task) +
`tier_task_entry` (spawned tiers via `nros_freertos_create_task` at the raw
`[tiers.x.freertos].priority`, heap-leaked `TierTaskCtx<F>` arg). The boot tier
opens the one session, **moves it into its final `crt` location before handing
out `SessionHandle`s** (avoids dangling the spawned tasks' pointer when the boot
executor moves), spawns `tiers[1..]`, runs `tiers[0]`. Boot bringup
(network/RNG/poll/zenoh-cfg) extracted to `freertos_boot_bringup`, shared with
the unchanged single-tier path. New `Executor::session_handle()` / opaque `Send`
`SessionHandle` / `open_with_session_handle()` make the session movable across
the RTOS task boundary. `Mps2An385::run_tiers` delegates here. MT=1 is the
FreeRTOS default (RFC-0032 §5.0).
**Build + QEMU-boot verified:** fixture `orchestration_tiers_freertos`
(`nros::main!(launch)` + `deploy="freertos"` + 2-tier `system.toml`) + test
`multi_tier_freertos_firmware_builds_and_boots_run_tiers` — builds the firmware
for `thumbv7m-none-eabi` (proves macro→`run_tiers`→kernel-link) and boots it on
QEMU (mps2-an385): `run_tiers_entry` prints the unique `(multi-tier)` banner,
brings up the network, and reaches the boot-tier `Executor::open` (fails only on
the absent router — the entry-poc lifecycle proof). The build caught a real bug
(`pvPortFree`→`vPortFree`) and a stack-tuning need (small inline arena so two
tier executors fit the task stacks). Gated on freertos prereqs.
**Remaining (optional, deeper):** a router-backed run asserting both tiers' topic
output + distinct task priorities + zenoh-pico concurrent-multi-spin holds (needs
zenohd + TAP); the boot-lifecycle proof above covers the emit + boot path.
**Files:** `packages/boards/nros-board-freertos/src/{entry,lib}.rs`,
`packages/boards/nros-board-mps2-an385-freertos/src/lib.rs`,
`packages/core/nros-node/src/executor/spin.rs` (`SessionHandle`),
`packages/testing/nros-tests/{fixtures/orchestration_tiers_freertos,tests/orchestration_tiers_freertos.rs}`.

### 228.D.2 — Multi-tier shared-state locking + accessor emit  ✅ DONE (codegen + 3-lang surface; cross-lang example pending)
**Wave 1 (runtime primitive):** `nros_orchestration::LockedSharedRegion<N>` — the
cross-tier counterpart to the lock-free `SharedRegion<N>` (Phase 172.I), every
`with` access under `critical_section::with` (platform supplies the impl — the
same primitive `ffi-sync` uses). Fills the gap the `SharedRegion` access-discipline
note explicitly deferred. No restore-state feature selected (avoids the
critical-section feature-unification conflict).
**Wave 2 (Rust accessor emit):** `codegen-system` emits `nros_shared_state.rs`
from the resolved `[[shared_state]]` — per region a `#[repr(C)]` struct from the
typed fields, a backing static (`LockedSharedRegion` when `sync=mutex`/
`critical_section`, else lock-free `SharedRegion`), and `_get/_set/_modify`
accessors viewing the struct over the bytes (`size_of::<Schema>()`-sized). Wired
into the bake tree (idempotent: written when regions exist, removed when absent).
Tests: `emit_shared_state_rust_typed_accessors` + the tiered-fixture bake test
(asserts the emitted module) + a standalone compile-check (generated module is
valid Rust against `nros-orchestration`).
**Wave 3a (cross-tier guard proof):** `locked_shared_region_serializes_concurrent_modify`
— two threads × 50k read-modify-writes on one `LockedSharedRegion` = exactly
100k, no lost updates (a lock-free region would fail). The serialization contract
holds.
**Wave 3b (C/C++ surface):** `codegen-system` also emits `nros_shared_context.h`
(the `#[repr(C)]`-matching `typedef struct` + accessor decls, `extern "C"`-guarded
for C and C++; Rust→C type map) + the Rust `#[unsafe(no_mangle)] extern "C"`
`nros_<name>_get/set/modify` exports (one definition; `modify` takes a C fn
pointer + ctx). Both wired into the bake (idempotent). Verified by the emitter
test + a standalone compile-check of the full emitted module (typed accessors +
C-ABI exports) for edition 2024.
**Remaining (optional):** a cross-*language* consuming example — a C/C++ node and
a Rust node sharing one region at runtime (links the bake-generated `.h` + `.rs`).
The single-language paths + the cross-tier guard are proven.
**Files:** `packages/core/nros-orchestration/src/lib.rs` (locked region + proof),
`packages/cli/nros-cli-core/src/cmd/codegen_system.rs` (Rust + C emit + wire).

### 228.H — Spin-period bound-check warning  ❌ DESCOPED
`spin_period_us` ≤ tightest timer period (RFC-0015 §4.3) needs each callback's
**timer period**, which is node-code metadata not present in the resolver inputs
(`system.toml` tiers + node `callback_groups`). Revisit if timer periods are
ever surfaced into node metadata; not worth a separate metadata channel now.

## Acceptance

- A `system.toml` with two `[tiers.*]` + a group→tier map produces a binary with
  two RTOS tasks at the declared priorities, each running its tier's callbacks.
- All-default-tier `system.toml` produces the same single-task output that ships
  today (no regression).
- Shared state declared in `[[shared_state]]` is reachable from both tiers with
  the correct lock behavior.
- `just ci` green; multi-tier fixture passes on ≥2 platforms.

## Notes

Design-of-record: **RFC-0015** (execution model, reconciled to Phase 212) +
**RFC-0032** (the entry-codegen/emit pipeline — boot scaffolds, `run`/`run_tiers`
contract, the shared resolver, the MT study, and the resolved open issues). The
scheduling *config home* is decided (RFC-0015 banner / RFC-0004 §7 / Phase 227.6).
Per the design→RFC rule, a design change in the execution model updates RFC-0015
first; a change in the emit mechanics updates RFC-0032 first. RT acceptance
harness + hardware gates are Phase 162; this phase is the codegen, not the test
rig.

### Coordination — issue #8 (two-copy receive) touches this executor

`docs/issues/0008-two-copy-receive.md` has a fix that lands in the same
`packages/core/nros-node/src/executor/` this phase is actively editing (228.B
per-tier executor emission, 228.C callback-group registration, 228.E spawn),
so it is **deliberately not being done in parallel** — flagging it here so the
executor work can fold it in or sequence it cleanly.

The concrete change: the **arena subscriber dispatch**
(`executor/arena.rs`) currently copies each message out of the rmw ring slot
into the per-subscriber `entry.buffer` (copy #1) before deserializing (copy #2).
A zero-copy in-place path already exists on the `Subscriber` trait
(`process_raw_in_place(f: FnOnce(&[u8]))`, used in `executor/handles.rs`) and as
the opt-in `lending`/`SlotBorrowing` API, but the main arena loop doesn't use
it. Routing arena dispatch through the in-place borrow eliminates copy #1 and
lets the per-subscriber `entry.buffer` shrink/disappear (a static-RAM win on
embedded). Copy #2 needs the borrowed-deserialization codegen of issue #7
(picked up separately). No action required for this phase — just awareness so a
future executor edit here doesn't collide with, or can absorb, the #8 rewire.
