---
id: 222
title: "freertos/nuttx/threadx fixture resolvers use existence-only require_prebuilt_binary — museum binaries pass silently (the #215 trap, still open on 4 platforms)"
status: resolved
resolved_in: "2026-07-17 — all 9 rtos resolver sites switched to the freshness variants (cargo dep-info / ninja-deps); regenerated-in-place cbindgen headers excluded from the comparison (cross-family false-stale); zpico_drift_gate compile-in-test exception documented in-file"
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

## RESOLVED (2026-07-17)

- All 9 `require_prebuilt_binary` sites in the four rtos resolvers switched
  to the matching freshness variant: cargo-built entries →
  `require_prebuilt_binary_fresh` (`.d` dep-info), cmake/ninja-built →
  `require_prebuilt_binary_fresh_cmake` (`ninja -t deps`). Negative-path
  proven: touching `threadx-linux/c/talker/src/main.c` fails the lane with
  "fixture is STALE … newer: …/src/main.c".
- The first full sweep exposed a REAL flaw in naive dep-graph freshness:
  the committed-but-regenerated cbindgen headers (`nros_generated.h`,
  `nros_cpp_ffi.h`, `zpico.h`) are rewritten in place by every family
  build — feature-variant builds ping-pong the content, so the mtime is a
  build side-effect, not an edit event, and any family built before
  another family's build gets a false STALE forever (an unfixable
  treadmill: rebuilding family A stales family B and vice versa). Fixed by
  excluding exactly those three paths from both probes
  (`REGENERATED_INPLACE_HEADERS`); no coverage lost — a semantic change to
  these headers implies an edited `.rs` source, which IS in the dep graph.
- `zpico_drift_gate`'s two in-test `cargo build`s documented in-file as
  the sanctioned compile-in-test exception (its subject IS build-script
  behavior; the corrupted run must fail to build by design).

Validation: fresh fixture rebuild of all four families, then the full
33-test `rtos_e2e` sweep green (18 pass-on-retry = the known
QEMU-under-load flake class, AGENTS.md Test Pitfalls).
