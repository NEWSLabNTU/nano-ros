---
id: 248
title: "Embassy board entry is a stub — every Board/EmbassyBoardEntry method todo!(), callbacks never fire"
status: open
type: limitation
severity: medium
area: boards
related: [issue-0178]
---

## Finding (release-prep audit 2026-07-24; documented in phase-216)

The Embassy half of the phase-216 bare-metal framework track is half-landed:

- `packages/boards/nros-board-embassy-stm32f4/src/lib.rs` (~line 228): every
  `Board` / `EmbassyBoardEntry` method beyond init/println is `todo!()`.
- Phase-216 doc (`phase-216-baremetal-framework-integration.md`, Deferred
  strategy section): the Embassy examples "compile + boot but won't actually
  fire `on_callback`" — the C.3 dispatch body is a placeholder.

The RTIC twin is COMPLETE (archived issue 0178, phase-289 — all four QEMU
lanes green), which makes the Embassy gap easy to miss: the two tracks look
symmetrical in the tree but only one runs.

## Release decision needed

Ship options:
1. **De-advertise** — mark Embassy as scaffold-only wherever it appears
   (book, examples README, matrix carve-out reason) and keep the crates as
   the landing pad for the follow-up; or
2. **Finish** — implement the dispatch body + board methods and stand up a
   QEMU lane mirroring the RTIC set.

Until one happens, an Embassy user gets a booting image that silently never
executes callbacks — worse than a compile error.

## Update (2026-07-24) — de-advertised; finishing remains open

Option 1 landed: `book/src/user-guide/embassy-integration.md` now opens with
a status admonition — Pattern A (hand-written main/tasks,
`examples/stm32f4/rust/talker-embassy`) is the supported shape; the
`EmbassyBoardEntry`/Deferred path is marked scaffold-only pending this
issue. Remaining scope: implement the C.3 dispatch body + board entry
methods and stand up a QEMU lane mirroring the RTIC set (phase-289 shape).

## Decision (2026-07-24) — ship de-advertised; finish is a future phase

**Chosen for the release: option 1 (as landed).** The release story is
"Embassy: hand-written Pattern A supported and documented; streamlined
board-entry path scaffold-only." This issue STAYS OPEN as the tracker for
the finish.

**State ledger (phase-216.C):** C.1 trait landed (`9de4b227e`). C.2
half-landed — `EmbassyRuntime` channel + `signal_callback` real
(`fc4213c4e` + `d7cbd8148`), `init_hardware` placeholder, entry methods
`todo!()`. C.3 half-landed — macro Embassy arm exists, dispatch-task body
is a placeholder (a Deferred image boots, signals callbacks into the
channel, nothing drains it). C.4 unstarted.

**Structural constraint that shaped the decision:** the Embassy crates pin
to stm32f4, which is hardware-gated (#221 — QEMU has no F4 ethernet), so
finishing "as stm32f4" can never earn a CI runtime lane — it would produce
exactly the untested-but-advertised state this issue exists to prevent.
The RTIC twin only reached Complete (phase-289) by living on
qemu-arm-baremetal (MPS2).

**Recorded finish plan (the future phase, when scheduled):** mirror
phase-289 — an Embassy variant on qemu-arm-baremetal (embassy-executor is
chip-agnostic; needs a SysTick time-driver on MPS2), complete the C.3
dispatch body, one pubsub fixture + QEMU runtime lane. The stm32f4 crate
then inherits the proven dispatch path and stays build-only until hardware
CI exists (full stm32f4 `init_hardware` HAL bring-up is parked behind a
hardware-rig decision).
