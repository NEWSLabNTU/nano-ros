# Phase 297 — ThreadX multi-tier execution

Implements RFC-0053 (ThreadX multi-tier via codegen static per-tier stacks +
native preemption-threshold). Builds on phase-296 W5.4 (the portable
`ExecutorNodeRuntime::apply_tier_sched_policy` every board shares).

**Status (2026-07-22):** design landed (RFC-0053). W1 C++ path DONE
(`cf69b09f2` + `650a4d7e9` — the tier→SchedContext lowering single-sourced in
`SchedContext::from_tier_policy`). W2 DONE (`nros_threadx_create_task` shim).
W3 DISSOLVED (byte-pool stacks — RFC-0053 revised from codegen-static Option A
to byte-pool Option B; no codegen change). W4 DONE (impl) — `run_tiers_entry`
on `nros-board-threadx` (boot tier + #144 chain-spawn + per-tier executors over
one shared session + `apply_tier_sched_policy`) + both board ZSTs wired;
`threadx-linux` builds + clippy-clean. W5 TODO (documented) — the 2-tier
`threadx-linux` runtime e2e that is the acceptance gate (fixture + test, retarget
`ws-realtime-rust`). The W1 Rust/C path is subsumed by W4 (the macro routes any
single *named* tier to `<Board>::run_tiers`).

**Common-backend principle (applies to every wave).** One backend serves all
languages; no logic is re-derived per codegen path. The tier→SchedContext
lowering lives once (`SchedContext::from_tier_policy`) and is reached by C,
C++, and Rust alike (W1, done). By the same rule: the ThreadX `run_tiers`
(W1-Rust / W4) must call `apply_tier_sched_policy` (never re-lower), and the C
`nros_threadx_create_task` shim (W2) is the single thread-creation backend the
Rust `run_tiers` and any C/C++ entry both call — mirroring the FreeRTOS
`nros_freertos_create_task` shape, not a parallel per-language implementation.

## Goal

Give `nros-board-threadx` the same multi-tier model as freertos/zephyr/nuttx
(one `Executor` per tier over one shared RMW session), with **codegen-baked
static per-tier stacks** (RFC-0053 Option A) and the tier's `preempt_threshold`
applied through ThreadX's **native** `tx_thread_preemption_change` — the one
platform where the six-dim `non_preempt_scope` is a kernel primitive, not
emulated.

## Waves

### W1 — v0 stepping stone: single-executor tier policy

The tier's RTOS-agnostic policy (class/budget/period/deadline) must reach the
single ThreadX executor. There are **two** entry paths, and the codegen
routing differs per language — both need the lowering:

- **C++ path — DONE (commits `cf69b09f2` then `650a4d7e9`).** The
  single-executor codegen path (`emit_cpp`, used by ThreadX + group-split plans
  per `ResolvedTierTable::has_group_split_node`) hardcoded `__sc.class_ = Fifo`
  and carried only `os_pri` + the spin cadence, so a `real_time` tier silently
  ran best-effort. **Per the common-backend principle** (one backend for all
  languages), the fix does NOT re-derive the mapping in the codegen. The
  tier→SchedContext lowering is single-sourced in
  `SchedContext::from_tier_policy` (nros-node); `apply_tier_sched_policy` (Rust
  runtime) and a new FFI `nros_cpp_create_sched_context_from_policy` (nros-cpp)
  both call it. `emit_cpp` now emits a `from_policy` call forwarding the **raw**
  tier fields (`class` string / periods / `os_pri`), re-deriving nothing — so a
  `real_time` tier lowers to the identical Sporadic SC on every language and
  the mapping cannot drift. `Fifo` behavior unchanged when no RT `class`.
  Backend tests `from_tier_policy_*` (nros-node); codegen test
  `typed_emit_single_executor_forwards_real_time_tier_to_backend`. Deferred:
  `time_triggered` single-executor (the backend returns the major frame, but
  the codegen would need to also emit the `register_time_triggered_dispatcher`
  call) and `deadline_action`/miss-policy carry across the FFI (the backend
  sets it; the `from_policy` FFI forwards `deadline_policy`, so this is
  actually covered — unlike the retired hand-derived path).

- **Rust-board path — PENDING, and it is `run_tiers`-shaped, not a
  `run_app_thread` tweak.** The `nros::main!` macro routes **any** tier table
  that is not the synthesized single `default` tier (`is_single_tier()`) to
  `<board>::run_tiers(&overlay, &[TierSpec{class, period_us, budget_us,
  deadline_us, preempt_threshold, …}], closure)`. So even a *single named*
  `real_time` tier on ThreadX routes to `run_tiers` — which ThreadX does not
  implement, i.e. it does not compile today. The C path (`emit_c`,
  `native_threadx_entry`) likewise emits `TierSpec` tokens, not
  `create_sched_context`, so it too needs a ThreadX `run_tiers`. Therefore the
  v0 Rust deliverable is a **`run_tiers` that handles the single-tier case**
  (boot tier only: build the executor, `apply_tier_sched_policy(tier[0])`,
  apply the tier's `priority` + native `preempt_threshold` to the app thread,
  spin) and errors clearly on `> 1` tier until W4 adds the per-tier threads +
  stacks. This is the `run_app_thread` boot-tier idea from RFC-0053's v0
  ladder, realized through the entry method the macro actually calls. The
  legacy synthesized single-`default`-tier ThreadX image keeps
  `run_with_deploy` → `run_app_thread` unchanged (no RT policy to apply).

- **Done when:** a single *named* `real_time` tier ThreadX image compiles,
  lowers budget/period to a Sporadic `SchedContext`, and applies its priority
  — same observable behavior as the posix/native single-tier path. No new
  stack machinery. Verified on `threadx-linux` (host sim) or
  `threadx-qemu-riscv64`.

### W2 — C FFI create-task shim — DONE

- `nros_threadx_create_task(name, entry, arg, stack_ptr, stack_len, priority,
  preempt_threshold)` — the SINGLE thread-creation backend (common-backend
  principle), added to `nros-board-common`'s shared `threadx_hooks.c` (compiled
  into every ThreadX overlay), NOT a per-overlay `c/` copy. Calls
  `tx_thread_create` with the caller-supplied stack (W3 static stacks). Details
  vs the original sketch:
  - **Entry is ThreadX-native `void(*)(ULONG)`**, not `void(*)(void*)` —
    `tx_thread_create`'s entry signature. `arg` (the Rust spawn context cast to
    `usize`) rides in as the ULONG thread input; no trampoline.
  - **`preempt_threshold` is passed straight to `tx_thread_create`** (its 8th
    param), so no separate `tx_thread_preemption_change` at creation. `-1`
    sentinel ⇒ `= priority` (no threshold); `>= 0` is the native
    `non_preempt_scope` value (RFC-0052).
  - **The TX_THREAD control blocks live in a bounded static array inside the
    shim** (`NROS_TX_MAX_TASKS`), not exposed to Rust — the port-specific
    `sizeof(TX_THREAD)` never crosses the FFI, and the RAM-heavy stacks stay
    caller-provided (Option A intact).
  - Rust binding + safe wrapper `spawn_tier_thread(name, entry, arg, stack,
    stack_len, priority, preempt_threshold: Option<u32>)` in
    `nros-board-threadx` (`#[allow(dead_code)]` until W4 calls it).
- **Done:** the C shim compiles clean (`gcc -Wall -Wextra -fsyntax-only`
  against the real ThreadX headers) and `threadx-linux` builds standalone
  (Rust binding + wrapper + C shim compile + link). The two-thread RUNTIME
  proof lands with W4's multi-tier e2e (which spawns real per-tier threads
  through this shim) — mirroring `nros_freertos_create_task`, which likewise
  has no standalone test and is exercised only via `run_tiers`.

### W3 — per-tier stacks — DISSOLVED into the byte-pool strategy (W4)

The original plan (codegen-baked static per-tier stack arrays, RFC-0053 Option
A) was **dropped** in favor of byte-pool stacks (Option B) — see the RFC-0053
revision. The premise for A ("consistency with the freertos/zephyr codegen")
was false: freertos spawns on its heap, zephyr on a static `k_thread` pool.
`nros_threadx_create_task` (W2/W4) allocates each tier's stack from the SAME
4 MB `TX_BYTE_POOL` the boot app thread already uses — no codegen change, no new
static RAM. So there is no separate W3 deliverable; the "stack" concern is
handled inside the W2 shim. Exact per-tier static stacks remain a future RAM
optimization for constrained MCUs (RFC-0053 §Revision).

### W4 — `run_tiers` multi-tier + native preempt-threshold — DONE (impl)

- `run_tiers_entry<B,C,F,E>` on `nros-board-threadx` (mirrors freertos
  `run_tiers_entry`): the boot tier (`tiers[0]`, highest priority) runs on the
  `tx_application_define` app thread; it opens the ONE session, runs the boot
  tier's `setup` FIRST (issue #144), then CHAIN-spawns `tiers[1..]` — each tier
  spawns the next only after its own `setup` returns, so no two tiers' entity
  declares race the shared session's interest handshake. Each spawned tier
  (`tier_task_entry`, a ThreadX-native `void(*)(ULONG)` whose `ULONG` input is
  the `TierTaskCtx` pointer) opens an `Executor::open_with_session_handle` over
  the shared session, applies its groups + `apply_tier_sched_policy` (the common
  backend, W1), registers, chain-spawns the next, and spins at its period.
- `preempt_threshold` flows through `TierSpec.preempt_threshold` →
  `nros_threadx_create_task` → `tx_thread_create`'s 8th arg (native
  `non_preempt_scope`); `-1` sentinel ⇒ `= priority`.
- Per-board ZSTs `ThreadxLinux::run_tiers` + `ThreadxQemuRiscv64::run_tiers`
  route the macro's `<Board>::run_tiers(&overlay, TIERS, setup)` here (mirrors
  `Mps2An385::run_tiers`).
- **Verified (static):** `threadx-linux` builds standalone + clippy-clean with
  the full `run_tiers` machinery + reworked shim (the whole spawn path compiles
  + links). `threadx-qemu-riscv64`'s method is structurally identical (its
  standalone build is blocked only by a pre-existing cc-rs cross-CFLAGS env
  issue, not this code).
- **Runtime acceptance is W5** (the 2-tier `threadx-linux` e2e).

### W5 — runtime e2e: 2-tier `threadx-linux` (acceptance) — TODO

Prove W4 at runtime by retargeting the existing RT-tiers workspace
`examples/workspaces/ws-realtime-rust` (`src/demo_bringup`: `control_node`
`/ctrl` on the `high` tier, `telem_node` `/telem` on the `low` tier) to
`threadx-linux`. This is fixture + test authoring — no more board code.
`threadx-linux` builds for the **host** `x86_64-unknown-linux-gnu` target
(ThreadX threads are pthreads; the C kernel needs `THREADX_DIR` set, already
wired) and uses NSOS host sockets, so no cross-toolchain and no QEMU — the
cheapest ThreadX runtime lane.

**Steps (each item is a concrete edit):**

1. **`src/demo_bringup/system.toml`** — add the ThreadX priority sub-tables
   next to the existing `posix`/`zephyr`/`nuttx` ones. ThreadX priorities are
   `0..TX_MAX_PRIORITIES-1` with **lower number = higher priority**, so the
   `high` tier must be the smaller number, and both should sit near the app
   thread priority (`nros_board_app_priority()` = 4). The boot tier is
   `tiers[0]` (the numerically-highest-priority = smallest number):
   ```toml
   [tiers.high.threadx]
   priority = 5
   [tiers.low.threadx]
   priority = 15
   ```
   (Optionally a `preempt_threshold` on `high` to exercise the native
   `non_preempt_scope` path — start without it to isolate the basic case.)

2. **`src/threadx_entry/`** — new entry crate mirroring `src/nuttx_entry`:
   - `Cargo.toml`: `[package.metadata.nros.entry] deploy = "threadx-linux"`;
     `[package.metadata.nros.deploy.threadx-linux]` with `rmw = "zenoh"`,
     `domain_id = 0`, `locator = "tcp/127.0.0.1:<PORT>"` (pick an unused port,
     e.g. `17297`; NSOS dials the host loopback directly — no slirp gateway
     like the nuttx rows). Deps: `nros` (`std`, `rmw-cffi`, `ros-humble`),
     `nros-board-threadx-linux` (feature `rmw-zenoh`), `nros-platform`
     (`platform-threadx`), `ctrl_pkg`, `telem_pkg`.
   - `src/main.rs`: `nros::main!(model = "demo_bringup");` (same one-liner as
     `native_entry`/`nuttx_entry`; the `deploy = "threadx-linux"` +
     `[tiers.*.threadx]` route the macro to `<ThreadxLinux>::run_tiers`).
   - `package.xml`: copy the nuttx_entry shape.

3. **`examples/fixtures.toml`** — add a `[[workspace_fixture]]`:
   ```toml
   id = "workspace-rust-threadx-linux-realtime"
   platform = "threadx-linux"
   lang = "rust"
   rmw = "zenoh"
   dir = "examples/workspaces/ws-realtime-rust"
   bringup = "src/demo_bringup"
   entry = "threadx_entry"
   target_dir = "target-fixtures/threadx-linux"
   target = "x86_64-unknown-linux-gnu"
   env = { NROS_LOCATOR = "tcp/127.0.0.1:17297", NROS_DOMAIN_ID = "0" }
   ```
   (Match the `codegen_out`/`target` fields of the existing
   `workspace-rust-threadx-linux` row; confirm whether the realtime rows use
   `codegen_out` by diffing against `workspace-rust-native-realtime`.)

4. **`packages/testing/nros-tests/tests/realtime_tiers_threadx_e2e.rs`** —
   mirror `tests/realtime_tiers_e2e.rs`: run the prebuilt `threadx_entry`
   fixture binary, subscribe `/ctrl` + `/telem` from an external observer over
   the baked locator, and assert the high tier publishes ~10× the low tier
   (both tiers scheduled over the one shared session). Use
   `nros_tests::output::*` marker constants, `unique_ros_domain_id()` if the
   harness seeds domains, and **fail** (`assert!`/`skip!`) on unmet
   preconditions — never bare `eprintln!`+return.

5. **Build + run:** add the fixture to the build step (`just build-test-fixtures`
   / `scripts/build/workspace-fixtures-build.sh threadx rust` — confirm the
   exact lane), then run the test. Rebuild the fixture after any board/core
   change (the mtime treadmill). Retest a red SOLO before filing (sim lanes
   flake under sweep load).

**Done when:** the `threadx-linux` 2-tier image boots one session, both the
boot tier (`control_node`) and the spawned tier (`telem_node`) deliver, and the
`/ctrl`:`/telem` rate ratio is ~10:1 — the runtime proof that `run_tiers` spawns
one executor per tier over one shared session with per-tier sched policy. This
simultaneously discharges the W2 shim's "two threads run" proof and the W1 Rust
`run_tiers` path.

**Watch-outs:**
- **Priorities:** ThreadX lower-number = higher-priority (opposite of NuttX
  SCHED_FIFO). A `high` tier numerically ABOVE `low` silently inverts the
  tiers. Keep both below `NROS_TX_MAX_TASKS`-worth of headroom and near the
  app-thread priority (4).
- **Byte-pool sizing:** each spawned tier `tx_byte_allocate`s a
  `nros_board_app_stack_size()` stack from the 4 MB pool. Two tiers ×
  (executor stack + zenoh) must fit; if `Executor::open` fails on the spawned
  tier, bump `BYTE_POOL_SIZE` in `threadx_hooks.c` or the overlay stack size.
- **Discovery:** the #144 chain-spawn means `telem_node` declares only AFTER
  `control_node`'s setup returns — expect telem's first `/telem` slightly
  later than ctrl's; the ratio assertion should tolerate a startup transient
  (count over a window after both are up, as `realtime_tiers_e2e.rs` does).

## Order and dependencies

W1 (SchedContext lowering, C++ path DONE; Rust path folded into W4) → W2 (shim,
DONE) → W3 (dissolved — byte-pool stacks, no codegen) → W4 (`run_tiers`
multi-tier, DONE impl) → W5 (runtime e2e, TODO — the acceptance gate). The macro
already routes any non-`default` tier table on ThreadX to `<Board>::run_tiers`,
so W4 also closes the W1 Rust/C path (a single named tier is just the one-tier
case of `run_tiers`). W5 needs only W4 (+ the existing `ws-realtime-rust`
workspace) — no other wave blocks it.

## Non-goals

- SMP core affinity (the ThreadX boards here are single-core — RFC-0053
  §Non-goals); measured/auto stack sizing; MPU per-tier isolation. The runtime
  `PlatformSched` `set_deadline`/`replenish` (kernel-native EDF/reservation)
  is a separate cross-board follow-up — ThreadX has neither EDF nor a
  reservation server, so the executor's own Sporadic `SchedContext` remains the
  budget mechanism there.
