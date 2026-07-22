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
`threadx-linux` builds + clippy-clean. Remaining: a 2-tier `threadx-linux`
runtime e2e (retarget `demo_bringup`). The W1 Rust/C path is subsumed by W4
(the macro routes any single *named* tier to `<Board>::run_tiers`).

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
- **Verified:** `threadx-linux` builds standalone + clippy-clean with the full
  `run_tiers` machinery + reworked shim (the whole spawn path compiles + links).
  `threadx-qemu-riscv64`'s method is structurally identical (its standalone
  build is blocked only by a pre-existing cc-rs cross-CFLAGS env issue, not this
  code). **Remaining acceptance:** a 2-tier `threadx-linux` runtime e2e — retarget
  the existing `demo_bringup` (`[tiers.high]`+`[tiers.low]`, ctrl 10 ms / telem
  100 ms) to `threadx-linux` (add `[tiers.*.threadx]` priority sub-tables + a
  fixtures.toml entry + a `realtime_tiers_threadx_e2e` test), asserting both
  tiers deliver over one session.

## Order and dependencies

W1 (SchedContext lowering, C++ path DONE; Rust path folded into W4) → W2 (shim,
DONE) → W3 (dissolved — byte-pool stacks, no codegen) → W4 (`run_tiers`
multi-tier, DONE impl; runtime e2e pending). The macro already routes any
non-`default` tier table on ThreadX to `<Board>::run_tiers`, so W4 also closes
the W1 Rust/C path (a single named tier is just the one-tier case of
`run_tiers`).

## Non-goals

- SMP core affinity (the ThreadX boards here are single-core — RFC-0053
  §Non-goals); measured/auto stack sizing; MPU per-tier isolation. The runtime
  `PlatformSched` `set_deadline`/`replenish` (kernel-native EDF/reservation)
  is a separate cross-board follow-up — ThreadX has neither EDF nor a
  reservation server, so the executor's own Sporadic `SchedContext` remains the
  budget mechanism there.
