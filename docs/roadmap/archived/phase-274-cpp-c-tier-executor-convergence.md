# Phase 274 — C/C++ tier-executor convergence (RFC-0015 Model 1 for all languages)

Brings **C / C++ onto RFC-0015 Model 1** — one RTOS task per tier, each an `Executor` over the one
shared session, `active_groups` gating — the same execution model the Rust `nros::main!` path already
runs via `run_tiers`. The **user surface is unchanged** (the RFC-0047 / phase-273 callback-group
model: `create_callback_group` + `create_*_in` + `system.toml group_tiers`); only the C/C++ backend
converges. See RFC-0015 (revised banner, "Model 1 is the single execution model for ALL languages")
and RFC-0047 ("Reconciliation with RFC-0015 Model 1"). Staged: native first (W1–W2), embedded next
(W3); sub-node v2 + `sched_context` intra-tier are follow-ups.

## Why

Today two execution backends coexist: Rust runs Model 1 (per-tier tasks + gating); C/C++ run a
single `Executor` with per-callback `sched_context` binding (phase-272/273). Two backends = drift +
divergent semantics. RFC-0015 already **decided** Model 1 as the single model; this phase makes C/C++
conform. `sched_context` is **re-scoped** (not removed) to the single-`Executor`/no-tier fallback + an
optional intra-tier fine-scheduling knob (RFC-0017); the phase-273 group API + `group_tiers` config
carry forward as Model 1's gating input.

## Waves

### W1 — C/C++ Model 1 primitives (session ⊥ executor + gating FFI)
The Rust boards already: open ONE session, spawn a task per tier, each task opens an `Executor` over
the **same** session (the `Borrowed` store) + `set_active_groups(filter)` + registers only its tier's
callbacks. C/C++ today (`CppContext`) own ONE executor bound to the session. W1 adds the primitives:
- **Session ⊥ executor split (C/C++):** an FFI to open **N executors over one shared session** — the
  Borrowed-store open the Rust side uses (`Executor::open_with_session_handle` /
  `SessionHandle`). `nros_cpp_session_open` + `nros_cpp_executor_open_over_session(session_handle)` (or
  equivalent), so each tier task gets its own `Executor` on the shared session.
- **Gating FFI:** `nros_cpp_executor_set_active_groups(executor, groups[], n)` over
  `Executor::set_active_groups` (reachable — `CppExecutor = nros_node::Executor`; not yet exposed).
- **Acceptance:** unit/integration — open a session + two executors over it; `set_active_groups(["ctrl"])`
  on one → only `ctrl`-group callbacks register there (mirror the Rust `active_groups` tests). No board
  loop yet. `cargo build -p nros-cpp -p nros-c` + `just check` green.

### W2 — native C++/C `run_tiers` board + codegen emit + native e2e
- **Board:** `nros::board::NativeBoard::run_tiers(tiers, setup)` (C++) + a C analog — mirror the Rust
  `BoardEntry::run_tiers` (`board/tier.rs`): open the session once, spawn one `std::thread` per
  `TierSpec` (highest-priority tier on the calling thread), each opening an executor over the shared
  session + `set_active_groups(spec.groups)` + running the setup's registration (gated) + spinning at
  the tier's period/priority. Reuse the RFC-0016 0–31→POSIX priority mapper.
- **Codegen:** `emit_c`/`emit_cpp` emit `run_tiers(TIERS, setup)` (from the resolved `group_tiers` →
  per-tier `active_groups`, the shared `nros-orchestration-ir` resolver) INSTEAD of the phase-273
  `bind_group_sched` + single `run_components` when multi-tier; single-tier keeps `run_components`
  (degenerate = one tier). Retire the C/C++ `bind_group_sched` tier emit (the `sched_context` table is
  no longer the tier mechanism — keep the FFI for the fallback).
- **Component→tier registration:** each tier task runs the SAME setup (construct + configure
  components); the executor's `active_groups` filter makes each tier take only its groups' callbacks.
  Node-pinned-to-tier (v1): the resolver enforces a node's groups map to one tier.
- **e2e:** a native C/C++ multi-tier workspace (reuse/convert `ws-realtime-{c,cpp}`) → assert each
  tier's callbacks run at their priority/period, matching the Rust `realtime_tiers_*` behavior — now
  via **real per-tier threads**, not `sched_context`. Built + run.
- **Acceptance:** `just check` green; native C/C++ multi-tier e2e passes (per-tier threads); the Rust
  `realtime_tiers_*_e2e` unchanged; single-tier entries byte-identical.

### W3 — embedded C/C++ `run_tiers` (per-RTOS tasks)
Extend `run_tiers` to the embedded C/C++ boards (FreeRTOS `xTaskCreate`, Zephyr `k_thread`, NuttX
task, ThreadX `tx_thread`) — one RTOS task per tier over the shared session, mirroring the Rust
embedded boards (RFC-0016 per-RTOS priority mappers + stack sizing from `TierSpec`). Shared-session
access across tasks must be sound on each backend (the zenoh-pico/DDS session concurrency model —
verify per platform).
- **Acceptance:** at least one embedded C/C++ multi-tier fixture builds + (QEMU where available) runs
  with per-tier tasks; the per-RTOS priority mapping matches `[tiers.<name>.<rtos>]`. Build-gated
  skips reported honestly.

## Sequencing
W1 (primitives) → W2 (native board + codegen + e2e — the proof C/C++ runs Model 1) → W3 (embedded).
Each wave green + landable; Rust unaffected throughout (it already runs Model 1).

## Acceptance (phase)
- C / C++ run RFC-0015 Model 1 (per-tier executors + `active_groups` gating), native (W2) + embedded
  (W3), same as Rust — a single execution model across languages.
- The user surface is unchanged (phase-273 group API + `group_tiers`); `sched_context` re-scoped to
  fallback + intra-tier (RFC-0047 reconciliation); the C/C++ `bind_group_sched` tier emit retired.
- Native + embedded C/C++ multi-tier e2e pass via real per-tier tasks; single-tier byte-identical;
  Rust realtime e2e unchanged.

## Outcome (2026-07-02) — native DONE (proven); embedded code-complete, runtime → #126

| Wave | Commit | Result |
| --- | --- | --- |
| W1 primitives | `bab684ae4` | C/C++ `session_handle` + `open_over_session` (borrowed executor, shared session) + `set_active_groups` FFI over the Rust `Executor` methods; MockSession gating tests |
| W2 native | `4fc3d5a22` | C++/C `NativeBoard::run_tiers` (thread per tier over one session, gated, per-tier priority) + codegen emits `run_tiers`; **`realtime_tiers_{cpp,c}_e2e` PASS via real threads** (ctrl:telem ≈ 5.5×), shared session sound; node-pinned-to-tier re-applied on the run_tiers path |
| W3 embedded | `3f095def6` | FreeRTOS/mps2 `freertos_run_tiers.c` + `FreertosBoard::run_tiers` + codegen + fixture — **compiles + links** (arm-none-eabi, ELF carries the symbol, entry uses `run_tiers`); alloc via `nros_platform_alloc` (RFC-0034). **Runtime blocked → #126** (tier-task stack overflow + `run_tiers` boot session doesn't connect on FreeRTOS — the zenoh-pico multi-task session issue) |

**The convergence is achieved + proven for native:** C, C++, and Rust now run the *same* execution
model — RFC-0015 Model 1, per-tier executors over one shared session with `active_groups` gating —
behind the identical phase-273 group API + `system.toml group_tiers`. `sched_context` is re-homed as
the single-`Executor`/no-tier fallback + intra-tier knob (RFC-0047). **Embedded** (FreeRTOS) is
code-complete and builds/links, but the runtime is deferred to **#126** (the hard-tail zenoh-pico
session-lifecycle-under-multi-task issue) — Zephyr/NuttX/ThreadX embedded `run_tiers` follow the same
pattern + likely the same #126 fix. Native + Rust realtime e2e unchanged; single-tier byte-identical.

## Risks / decisions
- **Shared session across tasks:** the RMW session (zenoh-pico / DDS) accessed from N tier tasks must
  be concurrency-safe — the Rust boards already rely on this (RFC-0015 §2.3); verify the C/C++ FFI
  path holds the same invariant (single session, per-task executors). Biggest risk — validate in W1.
- **Node-pinned-to-tier (v1):** the resolver enforces a node's groups → one tier; sub-node (a node on
  multiple tier tasks) is v2 (a follow-up) — do NOT build it here.
- **`sched_context` fate:** kept as the single-`Executor` fallback + intra-tier knob (RFC-0017); do
  not remove the W1/phase-273 table — retarget the *tier* emit only.
- **Codegen path parity:** `nros::main!` (Rust) already emits `run_tiers`; the C/C++ emitters must
  reach parity via the same shared tier resolver — no third tier-resolution copy.
- **Highest-priority-tier-on-boot-task:** mirror the Rust board's "boot task runs the top tier, rest
  spawned" to avoid an idle boot task; verify on native + each RTOS.
