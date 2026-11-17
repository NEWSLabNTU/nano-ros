---
id: 2539
title: "RFC-0064's four remaining open questions (vendor-SDK row, mandatory ABI subset, FVP `platform`, a generic-cmake shell)"
status: open
type: enhancement
severity: low
area: [boards, platform]
related: [rfc-0064, rfc-0072, rfc-0076, phase-524]
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

## Ruling (2026-11-17)

The user accepted the decision brief's four recommendations, as written. They
are recorded in RFC-0064's "Open questions".

1. **A vendor-IDE build is an integration shell (row 1).** Both shapes exist
   for NXP S32 parts today: `integrations/s32ds/`, and the per-board
   `cmake/board/nano-ros-board-s32z270-freertos{,-rtd}.cmake`. The shell is
   the intended home, because the vendor IDE owns the build. The per-board
   files stay only as the plain-CMake route. **Trigger:** confirm it on the
   CANHUBK344 bring-up, as RFC-0064 already planned.
2. **The mandatory platform ABI is derived, not guessed.** Link a
   serial-only, single-threaded image against a minimal platform
   implementation and record the symbols the linker demands. The porting
   guide then states that number. **Trigger:** the first minimal port.
3. **The index field is renamed so its meaning is plain.** Measured:
   `[board.fvp-aemv8r-smp]` installs only `arm-fvp`, the simulator, which no
   single OS owns. So the row's `platform` means "what to install", not
   "which OS". The rename (or a split into two fields) is a schema touch to
   `nros-sdk-index.toml` and its readers. The work is
   [phase-524](../roadmap/phase-524-emulator-twins.md) W6.
4. **No `integrations/generic-cmake/` shell until an RTOS is waiting for
   one.** There are five shells today (FreeRTOS, NuttX, PlatformIO, PX4,
   S32DS), and none is generic. Closed.

## What closes it

Question 4 is closed. Question 3 closes when the field is renamed. Questions 1
and 2 close when their triggers fire and the answer is written into RFC-0064.
Until then this issue stays open for them, by design.

Each question answered in RFC-0064 (a ruling, a measurement, or a fix to the
index row), or dropped there with the reason.
