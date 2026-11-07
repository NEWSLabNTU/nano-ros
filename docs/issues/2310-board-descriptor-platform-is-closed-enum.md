---
id: 2310
title: "A board descriptor's `platform` is a closed enum, so no board can name an out-of-tree platform"
status: open
type: bug
area: cli, boards, platform
related: [2292, 2291, phase-545]
found: 2026-11-07
---

**Home: [phase-545](../roadmap/phase-545-rmw-provider-scan-build-road.md).** Found while fixing issue 2292 (phase-537 W12 follow-up).

## Symptom

A porter follows the Extending road for a new platform:

```
nros new platform acmeos
nros new board acme --for-platform acmeos
```

Both board spellings (`nros new board --for-platform` and `nros board new`)
scaffold the package. Every catalog reader then refuses it:

```
unknown variant `acmeos`, expected one of `posix`, `freertos`, `bare-metal`,
`nuttx`, `zephyr`, `threadx-linux`, `threadx-riscv64`, `stm32`
```

## Code site

`packages/cli/nros-cli-core/src/orchestration/board_descriptor.rs`: the
descriptor's `platform` field deserializes into the closed `PlatformKind`
enum. A platform the user's workspace provides therefore needs a core edit
before any board can name it.

## Why it matters

RFC-0049 promises a porter zero central-file edits, and RFC-0064 R5 D1 says
`--out-of-tree` is the whole difference between nano-ros's boards and a
user's. Both are false for a new platform while the set is closed. The book's
Add a platform and Add a board pages lead a porter straight into this refusal.

## Options

1. **Open the set.** `platform` becomes a name resolved against the platform
   catalog (`nros-platform.toml` `names`, which issue 2293 makes the scaffold
   write). Every `PlatformKind` match (feature selection, family, driver
   road) then reads the platform package's own facts, or treats an unknown
   platform as "no built-in behaviour".
2. **A catch-all variant.** `PlatformKind::External(String)` keeps the
   built-in matches and routes an unknown name to the platform package's
   descriptor. This is smaller, but every match needs an `External` arm.

Either way it is a design decision: opening the set touches every
`PlatformKind` match. It is not a scaffold fix.

## Done when

`nros new platform acmeos && nros new board acme --for-platform acmeos`
followed by `nros board info acme` resolves the board, with no edit outside
the user's workspace. A test covers that road.
