---
id: 132
title: "Rust RTOS pubsub fixture resolvers reference binaries no build lane produces — those e2e combos can never run"
status: open
type: bug
area: testing
related: [phase-277, phase-275]
---

## Summary

Found during phase-277 W4: several Rust RTOS pub/sub e2e combinations in
`packages/testing/nros-tests` resolve fixture binaries by names/paths that no
`just <plat>` build lane actually produces (pre-existing drift, confirmed at
the pre-W4 baseline — e.g. 12 stale `threadx_cpp_*` resolver names at
`ea825a341`). The tests skip/fail on the missing fixture, so the combos are
silently uncovered.

Related discovery from the same wave: before W4, the FreeRTOS/NuttX **Rust**
listeners logged nothing on receive, so even a runnable combo's
`count_pattern` assertion would have been vacuous. W4 added the `I heard:`
logging; the resolver drift is what still blocks the combos from running.

## Evidence

- `packages/testing/nros-tests/src/fixtures/binaries/mod.rs` resolver entries
  vs the artifact lists of `just/freertos.just`, `just/nuttx.just`,
  `just/qemu-riscv64-threadx.just` lanes (see phase-277 working notes,
  `tmp/sdd-277/task-9-report.md`).

## Next steps

1. Inventory: for each rust RTOS pubsub resolver, check the lane that should
   produce its binary; fix name/path or add the missing fixture row.
2. Extend the phase-275 W6-style coverage check so a resolver naming an
   unbuildable fixture is a CI failure, not a silent skip.
