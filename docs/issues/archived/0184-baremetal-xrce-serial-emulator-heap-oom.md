---
id: 184
title: "qemu-arm-baremetal XRCE + serial pubsub e2e: executor-backing OOM (74888-byte alloc fails) — the #176 heap fix missed these images"
status: resolved
type: bug
area: baremetal
related: [issue-0176, issue-0178, issue-0189, phase-271]
resolved_in: "2026-07-13 heap pin bump (24576 → 131072) in the three serial/XRCE images"
---

## Summary (as filed)

`emulator::test_qemu_xrce_pubsub_e2e` + `test_qemu_serial_pubsub_e2e` died at
boot with `memory allocation of 74888 bytes failed` — the #176 signature —
after #176 raised the mps2-an385 DEFAULT heap 64→128 KB.

## Root cause

The three images (`talker-xrce`, `serial-talker`, `serial-listener`) don't
use the board default: each pins `NROS_HEAP_SIZE = "24576"` in its
`.cargo/config.toml` `[env]` — the phase-204.5 size-minimal recipe, sized to
the pre-271 "zenoh-pico working set / XRCE session + margin". The phase-271
per-entry executor backing is a single ~74.9 KB allocation, so a 24 KB heap
can never boot a `nros::main!` image; #176 fixed only the default, and these
were the only three sub-default pins in the tree (`git grep NROS_HEAP_SIZE`).

Considered and rejected: shrinking the backing via the phase-271
`[package.metadata.nros.entry] max_callbacks` knob — the arena floor at
`cbs=1` (~18.7 KB) plus the XRCE session (~12 KB) still busts 24 KB, and the
`run_with_deploy_sized` seam is posix-only today (bare-metal boards would
silently ignore the knob).

## Fix

- The three pins → `131072`, matching the #176 board default (HEAP is
  `.bss` on the 16 MB MPS2-AN385 — no flash cost); comments updated with the
  #184 rationale.
- `book/src/user-guide/configuration.md` size-minimal recipe: the broken
  `NROS_HEAP_SIZE = "24576"` advice → `131072` with the ≥128 KB
  `nros::main!` floor explained (the published phase-204/207 footprint
  figures were measured on pre-271 images; the RAM rows are stale until
  re-measured on current images — per the CLAUDE.md perf-number rule).

## Verified / residual

Both images now boot past allocation (banner + `Serial ready.`; no alloc
failure). The lanes remain red one layer deeper — the session open itself
(zenoh-serial hangs at `Executor::open`, XRCE `ConnectionFailed`) — split to
**#189** (suspected #178-layer-2/3 family or the phase-282 tx-batching
rework; latent behind this OOM the same way #178 was latent behind #176).
