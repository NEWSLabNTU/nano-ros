---
id: 222
title: "freertos/nuttx/threadx fixture resolvers use existence-only require_prebuilt_binary — museum binaries pass silently (the #215 trap, still open on 4 platforms)"
status: open
type: bug
severity: medium
area: testing
related: [issue-0215, issue-0196]
---

## Findings (deep audit 2026-07-17, E1)

- `packages/testing/nros-tests/src/fixtures/binaries/{freertos,nuttx,
  threadx_linux,threadx_riscv64}.rs` resolve with bare
  `require_prebuilt_binary` (existence check) instead of
  `require_prebuilt_binary_fresh` — a stale binary passes every sweep until
  it silently rots (exactly how #215's threadx-linux cyclone breakage hid
  since 2026-07-08, and the #196/#164 class before it).
- `packages/testing/nros-tests/tests/zpico_drift_gate.rs:66` runs a real
  `cargo build -p zpico-sys` TWICE per invocation with no doc-comment
  sanctioning it as a compile-check cell (E1 requires the carve-out to be
  explicit).

## Fix sketch

Switch the four resolvers to the freshness-checking variant (same input-set
rule as the build-side probe); either move zpico_drift_gate's builds to the
fixture stage or document the sanctioned exception in-file.
