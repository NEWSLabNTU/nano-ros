---
id: 190
title: "esp32 QEMU e2e: images boot, 0 delivery (talker↔listener, esp32↔native, ws-entry)"
status: resolved
resolved_in: "2026-07-15 — heap 48 KB (stack was the linker leftover; 96 KB heap left an 18 KB stack → overflow into .bss caused every 'corruption') + ws-entry test gets NROS_SUB_TYPE=int32"
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

## RESOLVED (2026-07-15) — it was a stack overflow, plus one test defect

The "session-init memory corruption" (this issue's residual AND the
pre-existing `esp32_emulator.rs`-header residual: the 0xffffffff
config-pointer fault, the stomped OpenSyn cookie, wild jumps
`mepc=0x9ae65930`) was never an allocator or zenoh-pico bug. On esp32-c3
the `.stack` region is the LINKER LEFTOVER — link.x fills DRAM from the
end of `.bss` up to 0x3fcce400 — so yesterday's 96 KB heap fix shrank the
stack to **18.4 KB** (`readelf -S`: `.stack` 0x3fcc9c14 + 0x47ec), while
the zenoh-pico handshake + nested smoltcp-poll path needs far more (#64
measured ≈98 KB). Deep frames wrote straight down into `.bss`
(`__stack_chk_guard` and the log statics sit first), which is exactly why
every symptom looked like heap corruption:

- InitAck cookies "full of DRAM pointers + `Z_TRANSPORT_LEASE`" — a
  zenoh-pico snapshot probe showed the cookie was garbage AT DECODE time
  (rx == pre-copy == post-send), i.e. the decoder's memory was trampled.
- The trap frame of the wild-jump crash carried a1=0x3fc89468 /
  a2=esp-alloc-heap-end+8 / a6=28000 — stack-frame debris.
- Instrumented `FreeListHeap` (foreign-pointer guard + counter):
  `foreign_free_count` stayed 0 — no cross-allocator free exists.

**Fix: heap 48 KB** — big enough for the phase-271 executor arena (the
16 KB OOM half, fixed yesterday), small enough to leave a ~67 KB stack.
Both directions of the pair e2e + esp32↔native pass.

**Fourth lane (`test_esp32_workspace_entry_e2e`): test defect.** The ws
Entry's `talker_pkg` publishes `std_msgs/Int32` on `/chatter` (pcap:
declares `0/chatter/std_msgs::msg::dds_::Int32_/…`), and the message type
is baked into the wire keyexpr — but the test spawned the external native
listener WITHOUT `NROS_SUB_TYPE=int32`, so it subscribed as String and
could never match. The listener's own doc comment documents the env; the
test now sets it.

Verified: `esp32_emulator` suite **8/8**; qemu-arm-baremetal emulator
suite 11/11 (the `zpico-alloc` guard is shared); `just check` green.

Hardening kept from the triage: `FreeListHeap::free`/`realloc` now refuse
pointers outside their arena (leak-not-corrupt) + a
`foreign_free_count()` diagnostic.

Board comment now warns: `.stack` on esp32-c3 is whatever DRAM `.bss`
leaves behind — check `readelf -S` after resizing ANY large static; there
is no runtime stack-overflow guard on this target.
