---
id: 190
title: "esp32 QEMU e2e: images boot, 0 delivery (talker↔listener, esp32↔native, ws-entry)"
status: open
type: bug
area: build
related: [issue-0181, issue-0064]
---

## Summary

With the esp32 lane restored to the fixture sweep (#181: lane added to both
sweep drivers, `esp32_qemu_*` underscore ELF names, harness consumes prebuilt
ELFs, `.bin` flash images packed), `test_esp32_qemu_talker_boots` and
`logging_smoke_esp32_qemu_emits_every_severity` are GREEN — the images build,
boot, and log. The four cross-delivery tests still fail with zero samples:

```
esp32_emulator test_esp32_talker_listener_e2e     — 0 received
esp32_emulator test_esp32_to_native               — native listener got 0
esp32_emulator test_native_to_esp32               — 0
esp32_emulator test_esp32_workspace_entry_e2e     — 0
```

## Notes

- #64's resolution (2026-06) had this lane e2e-GREEN (heap 96→16 KB fix etc.);
  the lane then dropped out of every sweep (#181's silent-gap era) and rotted
  unwatched. First triage step: diff today's images/boot output against the
  #64-era notes (OpenEth bring-up, locator .bss-static, heap plan).
- Suspect classes, in order: identical-identity pair collapse (the #179/#181
  ZID lesson — check the talker/listener baked IP/MAC), baked-port drift vs
  the harness's per-(variant,lang) table (the C/C++ lesson), then the #64
  heap/stack budget.

## Progress (2026-07-15) — OOM fixed; residual = session-init memory corruption

**Cause 1 (fixed): #64-era 16 KB heap.** All four delivery tests died at
`memory allocation of 17032 bytes failed` right after `Ethernet ready.` —
`esp_alloc::heap_allocator!(size: 16 * 1024)` predates the phase-271
executor rework (~75 KB backing allocation; the #184 class). 128 KB
overflowed DRAM at link (`.bss … overflowed by 13968 bytes`); **96 KB**
links and boots (boot + logging lanes green).

**Cause 2 (open): the pre-documented esp32-c3 session-init corruption**
(the `esp32_emulator.rs` header residual), now deterministic and
packet-characterized:

- socat-level: TCP handshake + zenoh InitSyn/InitAck complete; the guest's
  OpenSyn is answered with a close — router (`RUST_LOG=debug`) says
  `Decoding cookie failed at …/establishment/accept.rs:493`.
- pcap diff of the 49-byte cookie: the guest echoes it with bytes 7–14
  replaced by `58 94 c8 3f 58 94 c8 3f` — the pointer 0x3fc89458 (esp32-c3
  DRAM, ≈ heap start; app segment 3 loads at 0x3fc89298) written TWICE =
  allocator free-list node stomped into memory that still carries live
  handshake bytes (invalid/double free or use-after-free).
- Happens on the FIRST connect attempt (single TCP session in the pcap),
  so the header's `connect_with_retry` re-entrancy lead is ruled out for
  this failure mode.
- `_z_slice_copy` of the cookie is a deep copy and the bare-metal zenoh
  config (`[platform.bare-metal]`, batching on) is byte-identical to the
  GREEN cortex-m3 qemu-arm-baremetal lanes — the differentiator is the
  esp32 allocator/arch side, not the shared zenoh-pico config.

Also found: `qemu.rs::wait_for_output_pattern` returns `Ok(output)` on
TIMEOUT when any output was collected — the pair test's step-1 gate
("Listener connected and subscribed") passes without the listener ever
reaching `Waiting for messages...` (a string that, additionally, no
current image prints). Fail-loud cleanup candidate when this lane is
revived.
