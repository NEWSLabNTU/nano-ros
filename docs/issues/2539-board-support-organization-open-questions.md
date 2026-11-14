---
id: 2539
title: "RFC-0064's four remaining open questions (vendor-SDK row, mandatory ABI subset, FVP `platform`, a generic-cmake shell)"
status: open
type: enhancement
severity: low
area: [boards, platform]
related: [rfc-0064, rfc-0072, rfc-0076]
found: 2026-11-14
---

**Home: [RFC-0064](../design/0064-board-support-organization.md) "Open
questions"**

## What is open

Checked 2026-11-14. Four of RFC-0064's open questions are still open:

1. **Is a vendor SDK build (S32DS/RTD) row 1 or row 2** of the customization
   ladder? `integrations/s32ds/` landed (step 2), but the RFC never recorded
   the answer.
2. **What is the mandatory subset of the platform ABI?** RFC-0076 lets a port
   say it does not do something, but no document states which symbols a
   serial-only, single-threaded port must provide.
3. **`[board.fvp-aemv8r-smp] platform = "bare-metal"`** in
   `nros-sdk-index.toml`, while the board is a Zephyr board
   (`packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/`). Drift, or
   does the index's `platform` mean "provisioning family"? If the latter, the
   field is misnamed.
4. **Should there be an `integrations/generic-cmake/` shell** for an RTOS
   with a CMake build and nothing else? `integrations/` has `freertos`,
   `nuttx`, `platformio`, `px4` and `s32ds`.

The other open questions in the RFC are answered (the `net_stack` reader,
the bare-metal `[arch.cortex-m4f]`, the Zephyr Cortex-M witness, the three
scaffold board modules, the hosted non-Linux row); the RFC records each.

## Found by

phase-555's design review (W3, platform-board-rtos brief P7). The user ruled
(2026-11-14) that an RFC becomes Stable only after its open questions are
filed as issues and the RFC points at them.

## What closes it

Each question answered in RFC-0064 (a ruling, a measurement, or a fix to the
index row), or dropped there with the reason.
