---
id: 221
title: "stm32f4 rtic action-server/-client examples flash per their README but panic instantly — board init_hardware() is todo!()"
status: resolved
resolved_in: "2026-07-17 — premise was stale docs: init_hardware has been real since phase-289 (rtic, c2227f527) / the 216.C follow-up (embassy); no todo!() remains anywhere in the path. Fixed the 8 stale example headers + the board-crate doc to the post-289 truth; action pair build-proven."
type: bug
severity: medium
area: examples
related: []
---

## Finding (deep audit 2026-07-17, J1)

`examples/stm32f4/rust/action-{server,client}-rtic/` READMEs walk the user
through build + flash with no warning, but the referenced board
`init_hardware()` path is `todo!()` — the image panics immediately on real
hardware. A copy-out example that compiles, flashes, and dies is worse than
an absent one.

## Fix sketch

Either implement the board bring-up for the rtic action pair (the pubsub
rtic examples have it), or gate the examples with a loud README warning +
`compile_error!` until the seam exists. Check whether the phase-289 RTIC
delivery work already provides the pieces.

## RESOLVED (2026-07-17) — the premise was stale documentation

The audit (J1) read the example headers' "Skeleton status" sections, which
still said `init_hardware` is `todo!()` and "a real flash will hit the
panic". That text predates phase-289: the RTIC board's
`RticBoardEntry::init_hardware` does the full bringup (clocks / RMII /
smoltcp / explicit `nros_rmw_zenoh::register()`, delegating to the shared
`nros_board_stm32f4::init_hardware`), and `c2227f527` (#178) filled the
run task — all four `test_qemu_rtic_*_e2e` lanes INCLUDING action are
green on the QEMU mps2 sibling that shares this entry scaffold. The
Embassy variant's bringup is likewise implemented. `grep todo!()` across
the boards + examples: only doc-comment mentions remained.

Fix shipped:
- All 6 RTIC + 2 Embassy stm32f4 example headers: "Skeleton status" →
  "Runtime status" reflecting the post-289 truth, with the honest
  residual caveats (RTIC: on-hardware bench validation not yet run — the
  QEMU lanes are the runtime proof; Embassy: no e2e lane at all yet, the
  build-stage macro check is the only automated proof).
- `nros-board-rtic-stm32f4` lib.rs "Still todo!()" section → "Residuals"
  (spin_once's `Err(())` is BY DESIGN — RTIC owns the spin loop; the
  stale "`__nros_spin` is still `core::future::pending`" bullet deleted).

Build-proof: `just stm32f4 build-fixtures` green incl. the action pair
(thumbv7em-none-eabihf). On-hardware validation on a physical
NUCLEO-F429ZI stays hardware-gated and is recorded as a caveat in the
headers, not an open bug.
