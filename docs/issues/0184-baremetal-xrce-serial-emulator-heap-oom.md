---
id: 184
title: "qemu-arm-baremetal XRCE + serial pubsub e2e: executor-backing OOM (74888-byte alloc fails) — the #176 heap fix missed these images"
status: open
type: bug
area: baremetal
related: [issue-0176, issue-0178, phase-271]
---

## Summary

Deterministic (serialized rerun, fresh fixtures 2026-07-12):

```
emulator::test_qemu_xrce_pubsub_e2e:
  panicked at library/alloc/src/alloc.rs: memory allocation of 74888 bytes failed
  Bare-metal XRCE QEMU: published=0
emulator::test_qemu_serial_pubsub_e2e — same lane family
```

Identical signature to **#176** (resolved): the per-entry executor backing is
a single ~74888 B allocation, and #176's fix raised the mps2-an385 DEFAULT
heap 64→128 KB (`ae0aecaa6`). The `talker-xrce` / serial images still OOM →
they don't inherit that default (own linker/heap config, or a different
board entry path). Extend the 128 KB heap to the XRCE + serial baremetal
image configs (or route them through the same default the fix touched).
