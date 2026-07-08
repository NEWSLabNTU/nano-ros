---
id: 156
title: "logging-smoke-nuttx-qemu-arm image boots QEMU and prints nothing (45 s, no networking involved)"
status: resolved
type: bug
area: nuttx
related: [issue-0152]
resolved_in: (this commit)
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

## Resolution (2026-07-08)

NOT runtime silence — a fixture-resolver PROFILE mismatch. `build_test_fixture`
(the `bins/` resolver) used the env-default profile dir (`nros-fast-release`),
but every NuttX Rust fixture is built at the `release` profile: `nros-fast-release`
(lto=off) hits the non-deterministic armv7a-nuttx-eabihf cross-CGU miscompile
(reboot before `main` = the exact "boots silent" symptom), so
`fixtures-build.sh nuttx rust` forces `NROS_CARGO_PROFILE=release` and the nuttx
entry resolvers already hardcode `release`. So the resolver looked in
`target/armv7a-nuttx-eabihf/nros-fast-release/` for a binary the build wrote to
`release/` — resolving a stale/absent (or miscompiled) image while a fresh,
working one existed one dir over.

Fix: `build_test_fixture` forces `release` for `target == "armv7a-nuttx-eabihf"`.
Verified: both the `release` and `nros-fast-release` ELFs boot in QEMU and print
all six severities incl. `[FATAL] smoke: fatal payload` (so the image was never
the problem); after the fix + a `release` rebuild,
`logging_smoke_nuttx_qemu_arm_emits_every_severity` passes.
