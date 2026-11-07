---
id: 2362
title: "The Zephyr boot-report cell cannot exist on the workspace road: the arena oracle refuses its short arena at compile time"
status: open
type: limitation
area: [testing, zephyr, executor]
severity: medium
found: 2026-11-07
related: [1036, 0900, phase-531]
---

**Home: [phase-531 W6](../roadmap/phase-531-workspace-first-project-layout.md)**, batch 8.
**W6 acceptance (the W2 allowlists empty) is blocked on this decision.**

## What happened

The (ZephyrQemuCortexM, C, BootReport) cell (issue 1036) boots the C talker
on mps2/an385 with no console and a 32-byte executor arena. The first entity
registration runs out of arena, and the test reads the boot report out of the
halted guest's RAM to prove that the shortfall was recorded.

Batch 8 moved `examples/zephyr/c/talker` from a standalone leaf to a
single-node workspace, and the cell's row to `[[workspace_fixture]]`
(`zephyr-cortex-m-c-boot-report-zenoh`, image `mps2_boot_report`). On that
road the image's entities are **declared**: the bringup's `[[component]]`
composes into a model. So phase-412's arena oracle
(`packages/core/nros-node/src/executor/arena_oracle.rs`) prices the image and
refuses the build:

```
nros arena oracle (phase-412): NROS_EXECUTOR_ARENA_SIZE (Zephyr:
CONFIG_NROS_EXECUTOR_ARENA_SIZE) = 32 B, but the entities this image declares
are modelled at 2112 B (2080 B short ...)
```

The cell only worked on the leaf road because a standalone Zephyr leaf's
entity inventory did not compose (phase-481 W3 measured it refused for every
C/C++ leaf), so `arena_model::REQUIRED` was 0 and nothing was checked. The
oracle has no escape hatch, by design: a stated arena below the model is
exactly what it exists to refuse.

## Parked state

- The row stays and builds, but with the derived arena: its
  `west_overrides = { EXECUTOR_ARENA_SIZE = "32" }` was removed.
- `zephyr_cortex_m_c_boot_report_names_arena_exhaustion` is
  `#[ignore = "... (issue 2362)"]`.
- The image, its overlay
  (`packages/testing/nros-tests/fixtures/zephyr-boot-report/arena-exhausted.conf`)
  and the matrix cell are unchanged, so a decision can re-enable the cell
  without re-creating them.

## Options

**(a) Keep this one row on a leaf road.** This restores a standalone Zephyr
leaf for one fixture. It re-creates the shape phase-531 retires, and W7's
deletions would have to keep the leaf machinery alive for it, so it works
against the phase it sits in.

**(b) Re-scope the cell to the compile-time refusal.** The cell would assert
that the oracle refuses the image and names the knob and both numbers, as a
build-stage fixture. That is cheap and true to the tree's current design. It
LOSES the runtime-exhaustion coverage the boot report was built for (the
write of the shortfall into the boot record, and the fatal hook), unless some
scenario reaches that path which the oracle cannot foresee. What I found:

- Entities created past the declared set do not qualify. A C/C++ west
  application keeps its configure-time census check (issue 1419), which
  refuses a registration the contract does not declare. So "register more
  than declared" is refused one stage earlier, not at run time. This was
  read from `cmd/build.rs`, not measured.
- Executors whose arena is a runtime value do qualify. The oracle's own
  table lists "Rust `open_in` over a runtime slice / runtime sizing" and a
  sched task's executor as NOT checked: their arena is not a constant, and
  they report exhaustion through the same `report_arena_exhausted` path the
  cell watches. A boot-report image that opens its executor over a
  caller-supplied short slice would still exercise the runtime path. No such
  image exists today, and none exists for C.

**(c) Make the boot-report image's arena runtime-sized.** For example, a
variant of the talker, or a test-owned component, that opens its executor over
its own small buffer. The cell then keeps its runtime meaning on the workspace
road and the oracle stays exceptionless. This is a code change: a new entry
or component in the test tree and, for C, possibly an API that takes a
caller arena. It is not a fixture edit.

## Acceptance

One of the three is chosen and landed. The cell's test either runs again,
asserting the runtime path through (a) or (c), or is replaced by the
compile-time assertion (b). The `#[ignore]` goes either way.
