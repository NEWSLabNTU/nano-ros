---
id: 2538
title: "A board descriptor cannot contribute `[arch.*]`, `include_paths` or `required_env`: the RFC-0049 ladder stops at knobs"
status: open
type: tech-debt
severity: low
area: [boards, platform, build]
related: [rfc-0064, rfc-0049, phase-349]
found: 2026-11-14
---

**Home: [RFC-0064](../design/0064-board-support-organization.md) Sequencing
step 8**

## What is open

RFC-0064 step 8: "extend the RFC-0049 ladder to the build block: the board
layer contributes `[arch.*]`, `include_paths`, `required_env` (allowlist,
not open merge)". It never landed. Checked 2026-11-14:

- `packages/tooling/nros-platform-config/src/platform_config.rs` reads
  `arch_profiles`, `compile`, `required_env` and `[arch.*]` from the
  PLATFORM file only.
- None of the 22 tracked `nros-board.toml` files has an `[arch.*]`,
  `include_paths` or `required_env` key. Board descriptors carry
  `[capabilities]` and `[knobs]` deltas only.
- The `-mfpu` value the step was meant to move out of the platform file now
  lives in toolchain files and `NanoRosCargoProfile.cmake`, not in a board
  descriptor. So the specific debt the step named is paid another way; the
  general gap (a board on a new FPU variant cannot say so in its own
  descriptor) is not.

## Found by

phase-555's design review (W2/W3, platform-board-rtos brief P7, and the
0049-0064 pair). The user ruled (2026-11-14) that RFC-0064's steps 6 and 8,
which never landed, are filed as issues.

## What closes it

The board rung of the ladder accepts an allowlisted build block, read by
`nros-platform-config` and covered by a test; or a ruling in RFC-0064 that
toolchain files are the home for these facts, with the step marked dropped.
