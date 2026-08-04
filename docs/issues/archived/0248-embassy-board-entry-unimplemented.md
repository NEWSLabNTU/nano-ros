---
id: 248
title: "Embassy board entry is a stub — every Board/EmbassyBoardEntry method todo!(), callbacks never fire"
status: resolved
type: limitation
severity: medium
area: boards
related: [issue-0178, issue-0415, phase-337, rfc-0064]
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

## Resolution (2026-08-04, phase-337 W7.a) — resolved by deletion, not by finishing

`nros-board-embassy-stm32f4` is gone. The issue's own analysis is why: it
recorded that finishing "as stm32f4" *"can never earn a CI runtime lane — it
would produce exactly the untested-but-advertised state this issue exists to
prevent."* RFC-0064 turned that observation into a rule (a board with no
Runtime cell cannot be tier 1 or 2, enforced by `check-board-tiers`), and W7.a
applied it to the whole STM32F4 family.

What actually changed, and what did not:

- **Deleted:** the board crate, the two Embassy examples, the two
  `embassy_main_macro*` compile-check fixture rows, and the
  `embassy-stm32f4` deploy key in `board_path_for` / `framework_for`.
- **Kept:** the seam. `EmbassyBoardEntry` (`nros-platform/src/board/embassy_entry.rs`),
  the `nros::main!()` Embassy emit branch, and `nros ws check`'s
  Embassy-requires-Deferred lint all stay — they are what an out-of-tree
  Embassy board consumes, and RFC-0064's model is that boards arrive from
  integrators while nano-ros keeps the framework seams.
- **The C.3 dispatch-body gap is retired with the crate**, since the only
  image that could exercise it is gone. The recorded finish plan (an Embassy
  variant on qemu-arm-baremetal, mirroring phase-289's RTIC lane) is still the
  right shape if Embassy is ever wanted with a witness; it would start from
  `nros-board-mps2-an385`, not from a chip-specific crate.
- **New, narrower successor:** issue 0415 — `nros::main!()` selects the
  framework from a deploy-key table rather than the board crate's
  `framework = "embassy"` metadata, so an out-of-tree Embassy board silently
  gets `OwnedSpin`. That is the one live defect this deletion leaves behind.

`book/src/user-guide/embassy-integration.md` now opens with the no-in-tree-board
status and points at both the out-of-tree worked example and 0415.
