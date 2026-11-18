---
id: 2703
title: "The mps2-an385 bare-metal clock jumps forward ~171.8 s when a reader is preempted by another reader"
status: resolved
type: bug
area: [platform, boards]
severity: medium
found: 2026-11-18
resolved: 2026-11-18
resolved_in: "feat(rtic, phase-562 W10): derived levels on RTIC, proved on QEMU; RTIC is Tier 1 again"
related: [phase-562]
---

**Home: [phase-562](../../roadmap/phase-562-image-framework-field.md) W10**

## What

`nros-platform-mps2-an385`'s `clock_ns` and `clock_ms` extend the 32-bit
down-counting TIMER0 to 64 bits by counting wraps: read the counter, load
`LAST_VALUE`, count a wrap if the counter went up, store the counter. Those
were separate steps. A reader preempted between its counter read and its
`LAST_VALUE` load saw the newer, smaller value its preempter had stored,
took its own older value for a wrap, and moved the clock forward a whole wrap
(2^32 ticks at 25 MHz, ~171.8 s). The jump is permanent.

## Who could hit it

Anything that reads the clock from two contexts that can preempt each other.

- In the tree, only multi-level RTIC images (phase-562 W10): a level preempts
  a busy one, and the level-wake interrupt reads the clock. The W10 QEMU run
  saw the jump within seconds.
- No other in-tree mps2 bare-metal image could. Every clock read there runs
  in the main context or inside a critical section (the tickless idle), and
  no interrupt handler on the board reads the clock: not the UART TX, the
  Ethernet RX mask, the SysTick disarm, or the TIMER1 sporadic refill. The
  FreeRTOS mps2 board uses FreeRTOS's clock, not this one.
- An application that reads `nros::time` (or the platform clock) from its
  own interrupt handler, or from an RTIC task above another reader, could.

## Resolution (2026-11-18)

The read, the wrap test and the store are one critical section
(`elapsed_ticks` in `clock.rs`). It is the only wrap-counting clock in the
tree (`git grep WRAP_COUNT -- packages`).
