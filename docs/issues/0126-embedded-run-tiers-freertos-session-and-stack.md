---
id: 126
title: "Embedded C/C++ run_tiers (FreeRTOS) does not run: tier-task stack overflow + shared session never connects under the multi-task structure"
status: open
type: bug
area: freertos
related: [phase-274, rfc-0015, rfc-0016]
---

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

### Defect B (session never connects) — STILL OPEN, but narrowed
With A fixed, `run_tiers` executes **completely** (init ok, tier spawned, boot spins — no crash),
and the connect **locator is correctly threaded** (`FreertosBoard::run_tiers` passes the
compile-time `NROS_ENTRY_LOCATOR = tcp/192.0.3.1:17851`). Yet the zenoh session never reaches the
host `zenohd` (zero connections seen, no `[ctrl]`/`[telem]` ticks). **Ruled out this pass:** tier
stack (A), tier-task starvation (a 3 s **uncontended boot-session warm-up before spawning tiers did
NOT help** — so it is not a scheduling-starvation race), a crash, and a missing/empty locator.
**Remaining:** `nros_cpp_init` returns OK (session object opened) but the zenoh-pico session never
completes its TCP/handshake to `zenohd` under the C `run_tiers` path — the single-tier
`run_components` (`nros::init` + spin) connects in the identical env. Next: diff the zenoh-pico
session **connect + read/lease task** bring-up between `nros_cpp_init` (C run_tiers) and the
`run_components` path — whichever drives the connection completion is missing/mis-ordered in the
former. That is the load-bearing remaining item.
