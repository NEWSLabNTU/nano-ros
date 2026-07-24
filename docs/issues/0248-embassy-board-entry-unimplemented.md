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
