---
id: 2540
title: "RTIC does not meet RFC-0112's fifth Tier 1 criterion: no generated priority mapping, and its CI cell measures no bound"
status: open
type: test-gap
severity: medium
area: [executor, boards, testing]
related: [rfc-0112, rfc-0113, rfc-0111, issue-2417, phase-521, phase-562]
found: 2026-11-14
---

**Home: [RFC-0112](../design/0112-framework-support-tiers.md) D3, criterion 5
(added by [RFC-0113](../design/0113-rt-unified-wait.md) D8)**

## What is open

Criterion 5 admits a framework to Tier 1 only if "the real-time promise holds
under the framework's scheduler (RFC-0113 D2): nano-ros generates the priority
mapping and the bounds are measured on the framework's CI cell". RFC-0112 said
"`rtic` meets all four" when there were five. Read against the code on
2026-11-14, RTIC fails both halves:

- **No generated priority mapping.** The RTIC branch of `nros::main!` emits
  fixed priorities: the run task at priority 1
  (`packages/core/nros-macros/src/main_macro.rs:2871`), user custom tasks at
  priority 1 (`:2733`), and the tick and RX hardware tasks at priority 2
  (`:2676`, `:2688`). Every callback runs inline in the one `__nros_run` task
  (RFC-0112 D2, "Defer" row). Nothing maps a sched task's timing contract onto
  an RTIC priority.
- **No bound measured on the CI cell.** The RTIC cells
  (`packages/testing/nros-tests/tests/emulator.rs`,
  `test_qemu_rtic_{pubsub,service,action}_e2e`) assert delivery only
  (`received > 0`, `published > 0`). RFC-0113's coverage row for RTIC cites an
  idle measurement (0 SysTick one-shots in 20 s), but no test asserts it, and
  it also names `test_qemu_rtic_mixed_priority_pubsub_e2e`, which was retired
  (issue 2417; the comment at `emulator.rs:680-686` says its two-priority
  split was gone since phase-244 D1).

PX4, the other Tier 1 framework, does meet criterion 5: its entry codegen
emits one work item per sched task and RFC-0113 records arrival→dispatch
bounds on its cell.

## Found by

phase-555's design review (W3, platform-board-rtos brief P6). The user ruled
(2026-11-14) to check RTIC against criterion 5 by reading the code, and to
file an issue if it fails.

## What closes it

One of RFC-0112 D3's own outcomes:

- RTIC's branch generates its task priorities from the sched-task contract,
  and an RTIC cell asserts a D2 bound (idle wake count, or arrival→dispatch);
  or
- RTIC is demoted per D3 ("a branch that fails them is demoted ... or
  deleted"), with the maintainer's ruling recorded in RFC-0112.

[phase-562](../roadmap/phase-562-image-framework-field.md) reworks how an
image chooses RTIC (RFC-0111) and is the natural place to schedule either.
