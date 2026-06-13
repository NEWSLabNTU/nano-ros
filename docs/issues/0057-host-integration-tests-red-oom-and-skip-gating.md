---
id: 57
title: host-integration-tests chronically red — fixture-build OOM + light-tier skip-gating regression
status: open
type: bug
area: testing
related: [phase-244]
---

`host-integration-tests.yml` (the native `nros-tests` integration lane on
`ubuntu-22.04`, 2 vCPU / 7 GB) is effectively red — green only flakily. Two
independent causes, found 2026-06-13 while trying to validate phase-244 D3.

## Cause 1 — fixture-build OOM (fix ready)

The fixture builder fans out `NROS_BUILD_JOBS` (default **8**) cargo frontends,
each spawning its own rustc jobs. Compiling the heavy codegen deps
(`toml` / `regex-automata` / `nom` / `mime_guess` / `indexmap`) across the native
example fixtures exceeds 7 GB and the kernel OOM-kills rustc mid-compile —
surfacing as `SIGSEGV` / `SIGILL` / `SIGABRT` (reproduced locally with the same
crashes). The two `Build … fixtures` steps are best-effort (`|| … [SKIPPED]`), so
the crash silently leaves fixtures unbuilt or partially written; the run then
fails downstream on missing/corrupt binaries. Flaky: green when the runner has
headroom, red when not (matches the green-07:52 → red-12:03 flapping).

**Fix (branch `fix-host-integration-oom`, not yet merged):** cap the two
fixture-build steps + the nextest compile to `NROS_BUILD_JOBS=2` ×
`CARGO_BUILD_JOBS=2` (≤4 concurrent rustc). Verified: the capped build now runs
to completion (~45 min, no OOM-skip) instead of fast-failing. This is the
CLAUDE.md "parallel-build memory pressure" mitigation.

## Cause 2 — light-tier skip-gating regression (~195 tests, NOT yet fixed)

With the build fixed, `just test-integration` still fails: **195 / 384 tests
genuinely FAIL** (not skip) — overwhelmingly `native_api` C/C++/cyclonedds,
`logging_smoke`, and `integration_{esp_idf,platformio,px4,zephyr}`. The light
lane deliberately does NOT build the C/C++/Cyclone "extras" fixtures
(`build-fixture-extras`, skipped here for disk/ENOSPC reasons — see the workflow
comment), and those tests are *supposed* to `skip!` via `NROS_FIXTURES_OPTIONAL`.

The shared skip path `require_prebuilt_binary` (`nros-tests/src/fixtures/binaries/mod.rs:257`)
honors `NROS_FIXTURES_OPTIONAL` correctly — tests routing through it (e.g.
`action-server`, `cmake_node_register_metadata`) DID `[SKIPPED]` cleanly. The 195
failing tests do NOT route through that path, so an absent extras-fixture is a
hard failure instead of a skip. Likely introduced by the recent #41/#34
build-fixture conversions + the phase-246 cmake codegen refactors (heavy churn on
2026-06-13). Root-cause + the exact regressing commit not yet pinned.

## Impact

The lane cannot gate native-rust/C/C++ example changes (e.g. phase-244 D3's
talker/listener fork-unification can't be CI-validated here). Validation is being
done locally meanwhile.

## Direction

1. Land the Cause-1 OOM cap (`fix-host-integration-oom`).
2. Audit which `native_api` / `logging_smoke` / `integration_*` tests bypass
   `require_prebuilt_binary` and route their fixture-absence through the same
   `NROS_FIXTURES_OPTIONAL` `skip!` (so the light tier skips, the full tier still
   hard-fails). Bisect the #41/#34/246.x churn for the regressing commit.
