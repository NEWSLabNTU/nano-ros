---
id: 156
title: "logging-smoke-nuttx-qemu-arm image boots QEMU and prints nothing (45 s, no networking involved)"
status: open
type: bug
area: nuttx
related: [issue-0152]
---

## Summary

The `logging_smoke_nuttx_qemu_arm_emits_every_severity` test times out at
45 s with ZERO output from the guest. The bin
(`packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm`) is a
kernel-linked NuttX arm-virt image whose only job is to emit one log line
per severity over UART — it skips `Executor::open`, needs no network, and
should print within seconds of boot. A freshly built image (unfiltered
`scripts/build/fixtures-build.sh nuttx rust`, post-#130 tree) reproduces:
`QemuProcess::start_nuttx_virt` runs, `wait_for_output_pattern("[FATAL]
smoke: fatal payload", 45s)` times out, captured output is empty.

Notes:
- The build lane itself was the first blocker (row has no `rmw` field →
  every rmw-filtered invocation drops it; archived issue 0152 records the
  working verb). This issue is only about the RUNTIME silence.
- The nuttx entry-path images were also silent during the same window and
  were fixed by the phase-281 stream's #130 eth0/entry work — but this bin
  predates/bypasses the entry path (no networking), so it either has a
  distinct boot regression (board `run()`/UART writer registration?) or
  the image link lost the app the same way 0149's C lane did. Compare a
  known-good image date via git history of the bin + board crate.

## Repro

```
scripts/build/fixtures-build.sh nuttx rust   # unfiltered — builds the bin
cargo nextest run -p nros-tests --test logging_smoke logging_smoke_nuttx_qemu_arm_emits_every_severity
```
