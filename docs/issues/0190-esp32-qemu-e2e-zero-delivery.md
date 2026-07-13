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
