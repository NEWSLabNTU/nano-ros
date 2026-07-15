# Phase 289 — RTIC Runtime Delivery (tick + idle-yield)

**Goal.** The four `test_qemu_rtic_*_e2e` lanes deliver: an RTIC
mps2-an385 image opens its zenoh session, publishes/receives, and the
service/action roundtrips complete. Closes issue #178 layers 2–3 (layer 1
— deferring `Executor::open` out of `#[init]` — already landed).

**Implements.** RFC-0032 (entry-codegen pipeline, `Framework::Rtic`
branch). Completes the phase-216 Track-B e2e story ("emulator-first
validation"); phase-216 stays the design-of-record for the framework
integration itself.

**Status.** Design locked 2026-07-15 (this doc). Implementation not
started.

**Issue.** [0178](../issues/0178-rtic-executor-open-blocks-in-init.md) —
carries the full three-layer root-cause analysis and the failed
experiments this design routes around.

## Problem (condensed from #178)

`Executor::open` is a blocking zenoh-pico TCP connect driven by the
smoltcp poll loop. Under QEMU `-icount shift=auto`, the connect busy-wait
must `wfi` between iterations so host-timed slirp can deliver the
handshake packets (a pure spin races virtual time ahead of wall-clock →
the handshake never arrives). `wfi` in turn requires an armed periodic
IRQ **wired through RTIC's own vector table** — a board-crate
`#[cortex_m_rt::exception] fn SysTick` builds but never fires (verified
in #178; RTIC owns the vectors), and RTIC declares no monotonic today, so
nothing wakes `wfi`.

The direct-exec (non-RTIC) mps2 images deliver over the identical
slirp/smoltcp path, so everything below the entry scaffold is proven.

## Design

### Decision 1 — tick source: PAC-timer `binds` task, not rtic-monotonics

Two candidates were explored:

* **T1 — `rtic-monotonics` Systick monotonic.** RTIC-canonical, but
  `Mono::start(cx.core.SYST, hz)` consumes `SYST` while
  `RticBoardEntry::init_hardware_with_deploy(device, core)` takes
  `cortex_m::Peripherals` **by value** — a partial move of `core.SYST`
  in the macro emit makes the subsequent whole-struct pass ill-formed,
  so the trait signature would have to split the core peripherals. It
  also adds a `rtic-monotonics` dep + a module-level
  `systick_monotonic!` emit to every RTIC Entry. All of that buys
  nothing this phase needs (no RTIC-native timers are used yet).
* **T2 (CHOSEN) — CMSDK TIMER0 + `#[task(binds = TIMER0)]`.** The board
  impl already receives `device`/`core` and ignores both, and the mps2
  board crates do raw MMIO throughout (UART, LAN9118) — arming CMSDK
  TIMER0 (IRQ 8 per `mps2-an385-pac`) from `init_hardware` is one
  register write away. A `binds` hardware task is wired by RTIC into the
  real vector table — the exact mechanism the two `dispatchers` already
  use, so the wiring is validated by construction. No new deps, no trait
  signature change.

T1 stays the recorded upgrade path for when RTIC-native time (real
`Mono`-based nros timers) is wanted; `RticBoardSpec` isolates the choice
per deploy key so a later board can pick T1 without touching T2 boards.

### Decision 2 — surface changes

1. **`RticBoardSpec` (macro table, `main_macro.rs`) += `tick_irq`.**
   Per-deploy interrupt ident, same shape as `dispatchers`:
   `"rtic-mps2-an385" → TIMER0`, `"rtic-stm32f4" → TIM2`. The macro
   emits, inside `mod __nros_app`:

   ```rust
   #[task(binds = TIMER0, priority = 2)]
   fn __nros_tick(_cx: __nros_tick::Context) {
       <__NrosBoard as RticBoardEntry>::on_tick();
   }
   ```

2. **`RticBoardEntry` (trait) += two defaulted hooks** (additive, no
   existing impl breaks):

   ```rust
   /// Clear + re-arm the board's periodic tick IRQ. Called from the
   /// macro-emitted `binds` task. Default: no-op (a board whose spec
   /// declares no tick_irq never gets the task emitted).
   fn on_tick() {}

   /// Called once at the top of `__nros_run`, after `#[init]` returned
   /// and interrupts unmasked, BEFORE `open_executor`. The place to
   /// install idle-yield hooks that require a live IRQ source.
   fn on_interrupts_live() {}
   ```

3. **Board impls.**
   * `nros-board-rtic-mps2-an385`: `init_hardware_with_deploy` arms
     CMSDK TIMER0 (raw MMIO: base per CMSDK memory map, reload for a
     1 ms period at the 25 MHz sysclk, IRQ-enable; NVIC unmask is
     RTIC's job via the `binds` task). `on_tick` clears the timer
     interrupt flag — an uncleared flag is an IRQ storm that starves
     the priority-1 run task. `on_interrupts_live` calls the existing
     `nros_board_mps2_an385::enable_wfi_idle()` (installs `wfi` on both
     busy-wait sites: `sleep_ms` + `nros_smoltcp::do_poll`).
   * `nros-board-rtic-stm32f4`: arm TIM2 analogously; `on_tick` clears
     UIF. Compile-coverage only (no QEMU e2e lane); keeping the impl
     honest prevents the spec table from growing a "mps2-only" split.

4. **Macro emit (`__nros_run`)**: insert
   `<__NrosBoard as RticBoardEntry>::on_interrupts_live();` as the first
   statement, before `open_executor`.

### Decision 3 — priorities

`__nros_run` runs at priority 1. The tick task MUST preempt it (its
whole purpose is waking the `wfi` inside the run task's busy-waits) →
**priority 2**. mps2-an385 has `NVIC_PRIO_BITS = 3` (8 levels), so 2 is
valid; the two dispatchers stay at their defaults. Acceptance includes a
probe run demonstrating the tick fires *while* `__nros_run` is inside the
connect busy-wait (the #178 layer-3 verification that killed the SysTick
experiment).

### Non-goals / recorded alternatives

* **ETHERNET IRQ (13) RX-wake** — LAN9118 RX interrupt could wake `wfi`
  on packet arrival and cut handshake latency below the tick period.
  Deliberately out: the timer alone is sufficient (bounded 1 ms wake),
  and the LAN9118 IRQ path is unproven on this board. Recorded as a
  follow-up optimization slot.
* **rtic-monotonics migration (T1)** — see Decision 1.
* **Embassy sibling** — `Framework::Embassy` has the same masked-init
  shape but no QEMU lane; out of scope until it gets one (phase-216
  Track C).

## Work items

| # | Item | Where |
|---|------|-------|
| W1 | `RticBoardSpec.tick_irq` + `__nros_tick` binds-task emit + `on_interrupts_live()` call in `__nros_run` | `packages/core/nros-macros/src/main_macro.rs` |
| W2 | `RticBoardEntry::{on_tick, on_interrupts_live}` defaulted hooks | `packages/core/nros-platform/src/board/rtic_entry.rs` |
| W3 | mps2 board: arm CMSDK TIMER0 in init, clear in `on_tick`, `enable_wfi_idle` in `on_interrupts_live` | `packages/boards/nros-board-rtic-mps2-an385` |
| W4 | stm32f4 board: TIM2 twin (compile coverage) | `packages/boards/nros-board-rtic-stm32f4` |
| W5 | Drop the unused `rtic-monotonics` dep from `listener-rtic-mixed` (dead experiment residue) | `examples/qemu-arm-baremetal/rust/listener-rtic-mixed/Cargo.toml` |
| W6 | Rebuild `qemu-arm-baremetal` fixtures; verify tick-during-busy-wait probe; run the 4 lanes | build + `nros-tests::emulator` |

## Acceptance

* `test_qemu_rtic_pubsub_e2e`, `test_qemu_rtic_mixed_priority_pubsub_e2e`,
  `test_qemu_rtic_service_e2e`, `test_qemu_rtic_action_e2e` — all green
  on freshly rebuilt fixtures (never proven green post-`Executor<'s>`;
  treat this as first-light, per #178's history caveat).
* Non-RTIC baremetal lanes (ethernet 8, serial, xrce) stay green — W1–W3
  touch no shared runtime code paths, but the fixture family rebuild
  makes the sweep the gate.
* `just check` green (embedded clippy covers both board crates).
* Issue 0178 resolved + archived; phase-216 Track-B status updated to
  point here.
