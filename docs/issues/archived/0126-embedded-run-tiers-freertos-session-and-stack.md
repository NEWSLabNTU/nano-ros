---
id: 126
title: "Embedded C/C++ run_tiers (FreeRTOS) does not run: tier-task stack overflow + shared session never connects under the multi-task structure"
status: resolved
type: bug
area: freertos
related: [phase-274, rfc-0015, rfc-0016]
resolved_in: "phase-274 W3 follow-up (2026-07-03)"
---

> **RESOLVED (2026-07-03).** All three blockers fixed + verified on QEMU mps2-an385:
> (0) the "native single-tier emit" was a **stale `nros` CLI** — `just setup-cli` →
> correct `FreertosBoard::run_tiers` emit + link; (A) **256 KiB tier-task stack** →
> no HardFault; (B) the session-never-connects blocker was **`spin_once(storage, 0)`**
> in the C run_tiers spin loops — timeout 0 returns immediately and never drives the
> zenoh-pico session RX/TX/handshake, so the executor never ran. Passing the tier
> **period as the spin timeout** (blocking read, as `component_spin_loop` + the Rust
> `run_tiers_entry` do) fixes it. Result: both tiers schedule + **publish** at their
> declared periods — `[ctrl] tick` (10 ms) ~6× the rate of `[telem] tick` (100 ms),
> and each tick prints only on `publish_raw().ok()`, proving the shared session
> connects. RFC-0015 Model 1 (per-tier executors over one shared session) is proven
> on embedded FreeRTOS. See the resolution section at the end.

## Summary

phase-274 W3 landed the embedded C/C++ `run_tiers` path for FreeRTOS/mps2-an385 (RFC-0015 Model 1):
`freertos_run_tiers.c` (one FreeRTOS task per tier over a shared session, gated), `FreertosBoard::
run_tiers`, the codegen gate flip, and the `ws-realtime-cpp-mps2` fixture. It **compiles and links**
(verified: `arm-none-eabi-gcc -mcpu=cortex-m3` clean; the entry links + the ELF contains
`nros_board_freertos_run_tiers`; the generated entry uses `run_tiers` with the two tiers baked). But
it **does not run** under QEMU — two runtime defects, so the embedded Model-1 e2e is NOT yet proven.

## Defects (QEMU mps2-an385, verified)

**A — tier-task stack overflow → HardFault (fixable).** The default 64 KiB tier-task stack overflows:
`-d int` shows a Prefetch Abort at `0xa5a5a5a4` (FreeRTOS `tskSTACK_FILL_BYTE`) right after a context
switch. A diagnostic bump to 256 KiB removes the fault. Cause: the borrowed executor's spin/dispatch
needs more stack than 64 KiB (the boot tier survives on the 512 KiB `app_task`). Fix: size the
tier-task stack from `TierSpec.stack_bytes` (bake a sufficient value in `[tiers.<name>.freertos]`
stack) and/or raise the board default — mindful of QEMU/mps2 RAM (256 KiB × N tiers may not fit;
tune the executor arena + stack).

**B — shared session never connects under `run_tiers` (the deeper blocker).** Even with no fault
(256 KiB stack), the `run_tiers` boot **session never connects**: zenohd shows zero connections, no
`[ctrl]`/`[telem]` ticks, though the CPU is alive + context-switching. The single-tier
`run_components` control fixture connects + publishes `Published: 0..71` in the IDENTICAL environment
(network/zenohd/QEMU all work) — so this is **`run_tiers`-specific**, consistent with the
shared-session-across-FreeRTOS-tasks (zenoh-pico) soundness concern flagged in phase-274. Likely: the
boot task must bring the session fully up + connected BEFORE spawning tier tasks (or the zenoh-pico
read/lease task interaction with the borrowed executors on separate FreeRTOS tasks is unsound). Needs
dedicated debugging of the zenoh-pico session lifecycle under the multi-task run_tiers structure.

## Impact

Embedded C/C++ multi-tier (Model 1) is code-complete + builds/links but unproven at runtime on
FreeRTOS. Native C/C++ Model 1 (phase-274 W2) + Rust are proven. Single-tier embedded C/C++ is
unaffected (uses the existing `run_components` path). Zephyr/NuttX/ThreadX embedded `run_tiers` were
deferred to a follow-up and will likely hit the same session-lifecycle question (B).

## Fix direction

1. **B first** (the blocker): ensure the boot task's session is fully open + connected before tier
   tasks spawn; audit zenoh-pico session concurrency across FreeRTOS tasks (the Rust FreeRTOS
   `run_tiers` boots — compare its session bring-up ordering vs the C path). This is the load-bearing
   item.
2. **A**: bake/size the tier-task stack (from `TierSpec.stack_bytes`) to fit the executor; tune RAM.
3. Then the QEMU e2e (`ws-realtime-cpp-mps2`, both tiers scheduling) closes it.

## Update (2026-07-02) — build fixed (stale CLI), defect A FIXED, defect B narrowed

Verified on a known-good machine.

### The "no run_tiers emit / undefined `app_main`" symptom was a STALE CLI, not a code bug
Building the fixture regenerated the C++ entry as `NativeBoard::run_components` (native single-tier)
→ link failed `undefined reference to 'app_main'`. Root cause: the `nros` CLI binary
(`packages/cli/target/release/nros`) predated the 274-W3 `emit_cpp` changes (binary mtime ~7 days
older than the 274-W3 commit), so the build used **old codegen with no FreeRTOS `run_tiers` gate**.
**Fix: rebuild the CLI (`just setup-cli`).** After that the fixture regenerates correctly —
`static const NativeTierSpec __nros_tiers[2] = {...}` + `nros_app_main` calling
`FreertosBoard::run_tiers(..., __nros_tiers, 2u)` — and **links clean** (ELF produced). (Lesson: the
workspace-fixtures build does not rebuild the CLI; a stale `nros` silently emits the pre-274-W3
shape. Worth a staleness guard.)

### Defect A (tier-task stack overflow) — FIXED + verified
Raised the `freertos_run_tiers.c` spawned-tier default stack from 64 KiB to **256 KiB**. Under QEMU
mps2-an385 the firmware now runs the full `run_tiers` path with **no HardFault** (diagnostic
semihosting trace: `nros_cpp_init ok → tiers spawned → entering boot spin`). Caveat: the codegen
does NOT propagate `[tiers.*.freertos].stack_bytes` into `NativeTierSpec` (the emitted spec has
`stack_bytes = 0`), so config-driven per-tier sizing needs an `emit_cpp` fix; the 256 KiB C default
is the working stopgap.

### Defect B (session never connects) — RESOLVED (root cause: `spin_once(…, 0)`)
Isolated by a control experiment: the single-tier sibling (`workspace-cpp-freertos`,
`run_components`) **connects + publishes** (`Published: 0..N`) in the exact same QEMU/zenohd/slirp
harness (`net=192.0.3.0/24,host=192.0.3.1`, zenohd on the fixture's locator port) — so the harness,
network, backend register, RNG seed, and locator are all fine. The bug was **run_tiers-specific**:
the C spin loops called `nros_cpp_spin_once(storage, 0)`. Timeout **0** returns immediately without
driving the zenoh-pico session's RX/TX/handshake from the spin path, so the executor never ran and
the session never completed its transport. `component_spin_loop` (`run_components`) spins
`spin_once(10)` and the Rust `run_tiers_entry` spins `spin_once(period_ms)` — both **blocking
reads**. **Fix:** pass the tier `period_ms` as the `spin_once` timeout in both the boot-tier and
spawned-tier loops. After the fix, both tiers schedule + publish: `[ctrl] tick` (10 ms period) runs
~6× the rate of `[telem] tick` (100 ms), and each tick prints only on `publish_raw().ok()` — i.e.
the shared session is connected and both tiers publish over it. RFC-0015 Model 1 (per-tier executors
over one shared session) verified on embedded FreeRTOS/mps2-an385.

The earlier warm-up-before-spawn experiment (which did NOT help) correctly ruled out
tier-task starvation — it was never a scheduling race; the executor simply wasn't being driven.
