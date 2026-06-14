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

## Cause 2 — NOT a skip-gating bypass (re-diagnosed 2026-06-15)

**The original "195 tests bypass the skip path" diagnosis is wrong.** Reproduced
locally (`NROS_FIXTURES_OPTIONAL=1`, extras absent, `native_api` + `logging_smoke`
+ `integration_zephyr`): the fixture-absent tests DO route through
`require_prebuilt_binary` and `skip!` correctly — they panic
`[SKIPPED] fixture binary not prebuilt: …` from `mod.rs:280`. The
`native_api` callback/interop cases use
`build_*_callback().unwrap_or_else(skip_missing_fixture)`, but the inner
`require_prebuilt_binary` `skip!`-panics `[SKIPPED]` *before* returning, so
`skip_missing_fixture`'s non-`[SKIPPED]` `panic!` arm is never reached.

`cargo nextest` has no native skip, so every `[SKIPPED]` panic shows as a raw
`FAIL` — that is the **195** figure. But the lane's pass/fail is decided by the
`test-integration` recipe's `_rewrite-skipped-junit` + `_count-real-failures`,
which reclassify `[SKIPPED]` junit failures to `<skipped>`. Verified: a 47-test
`native_api`/`logging_smoke`/`integration_zephyr` slice showed 13 raw nextest
FAILs → `_rewrite-skipped-junit` rewrote **10** to `<skipped>` → `_count-real-failures`
returned **2**. **Skip-gating is functioning; the "195" was the raw nextest count,
not the recipe's real-failure count.**

The chronic red is therefore **Cause 1 (OOM)** corrupting/partially-writing
fixtures → the binary then *exists* (so `require_prebuilt_binary` returns Ok, no
skip) → it crashes/misbehaves at runtime → a genuine non-`[SKIPPED]` FAIL. Cap the
build jobs (Cause 1) and the lane stops manufacturing those.

### Residual: 2 genuine non-`[SKIPPED]` failures (stale fixtures, not lane-gating)
`test_cpp_rust_service_interop` (`native_api.rs:1241`, "0 responses from C++
server") and `test_action_callback_interop_c_client_cpp_server` (`:850`, no "Goal
accepted!") FAIL even in isolation — both **C++ server ↔ non-C++ client**.
Same-language and C-server↔rust-client pass. The local C/C++ fixtures are
**2026-06-12**, older than the phase-248 C5c nros-cpp "RMW/platform-agnostic"
churn (`dda517c0f` …) → likely **stale fixtures**, not a live regression. These
tests are NOT exercised by the light CI lane anyway (extras absent → `[SKIPPED]`).
Confirm via a fresh `build-fixture-extras` rebuild; if they still fail fresh, it's
a real C++-server cross-language regression to file separately.

## Impact

The lane cannot gate native-rust/C/C++ example changes (e.g. phase-244 D3's
talker/listener fork-unification can't be CI-validated here). Validation is being
done locally meanwhile.

## Direction

1. **Land the Cause-1 OOM cap (`fix-host-integration-oom`)** — this is the actual
   lane-red driver (corrupt fixtures → runtime FAILs that aren't `[SKIPPED]`).
2. ~~Audit skip-gating bypass~~ — **not needed** (re-diagnosed above): skip routing
   + `_rewrite-skipped-junit`/`_count-real-failures` already work; the "195" was the
   raw nextest count.
3. Confirm the 2 residual `native_api` C++-server interop failures are stale
   fixtures (fresh `build-fixture-extras` rebuild). If real, file a separate
   C++-server cross-language regression issue.
