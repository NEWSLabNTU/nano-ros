---
id: 570
title: "nuttx-riscv Rust realtime entry crashes: `nsh_main` hits 89.5% of its stack and the guest takes an assertion dump"
status: open
type: bug
area: boards
related: [issue-0565, issue-0569, issue-0246, phase-285]
---

## Symptom

`realtime_tiers_e2e`, row `nuttx-riscv/rust`. The guest console (printed by the
verdict since issue 0565) is a NuttX assertion dump — pages of `stack_dump:`
followed by `dump_tasks:`. The task table diagnoses itself:

```
dump_task:  PID GROUP PRI POLICY  TYPE    … STACKBASE   STACKSIZE  USED    FILLED  COMMAND
dump_task:    3     3 100 RR      Task    … 0x800caea8      65208   58364   89.5%!  nsh_main
dump_task:    4     3 100 RR      pthread … 0x801225f0      65248     912    1.3%   nsh_main
dump_task:    5     3 100 RR      pthread … 0x801325f8      65240     356    0.5%   nsh_main
```

`nsh_main` at **89.5%** of 65 208 bytes, flagged `!`. The test reports
`low-tier /telem never reached 5 deliveries`, which is what a crashed guest looks
like from outside.

## Reading

The pressure is on the TASK that runs the entry, not on the spawned tiers — they
sit at 1.3% and 0.5% of comparable stacks. So this is not the `#246` tier-stack
problem (spawned tiers requesting std's 2 MiB default); those stacks are already
explicit and nearly empty.

What runs on the main task here is the boot tier: `Executor::open`, the RMW
session, the boot tier's declares, and then `nuttx_spin_tier_forever`. The Rust
executor plus zenoh-pico's call depth is the plausible consumer.

## Where to look

* the entry task's stack budget — the `nsh_main` stack size in the riscv
  defconfig (`packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig`),
  against what the arm cell gets (arm does NOT crash, so the two budgets or the
  two call depths differ — establish which);
* whether 89.5% is the peak or the point of death: `USED` is a high-water mark,
  and `!` means NuttX flagged it, so the overflow may be just past this frame;
* `CONFIG_PTHREAD_STACK_DEFAULT` is 64 KiB and the tiers honour `stack_bytes`
  (`NUTTX_TIER_STACK_DEFAULT_BYTES`) — the main task is the one path that does
  NOT go through that knob.

## Note

Sibling row `nuttx-arm/rust` fails in the SAME test with a completely different
mechanism (a transport failure — issue 0569). They were one issue (0565) until
the verdict started printing the console; do not assume a shared fix.

## Acceptance

* the `nuttx-riscv/rust` row of `realtime_tiers_e2e` passes;
* the fix states the measured headroom, not just "it stopped crashing" — a
  stack raised until the symptom disappears is issue 0163's shape.
