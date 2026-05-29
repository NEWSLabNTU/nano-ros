# Phase 198 — Test-suite consistency hardening (remaining items)

**Goal.** Finish the test-suite consistency pass started in the AXIS review
(landed 2026-05-29, commit `13bd04ca1`): every E2E test should (a) report
`[SKIPPED]` — never PASS — on an unmet precondition, (b) parse talker/listener
output through the one canonical helper against one canonical fixture format,
and (c) keep debug output centralized. AXIS 1 (precondition `skip!`s), the
freertos `Loopback received:` → `Received:` rename, the 6 simplest hand-rolled
`contains("Published:"/"Received:")` assertions, and AXIS 3 (debug logs, already
clean) are **done**. This phase tracks the deferred remainder.

**Status.** Proposed (2026-05-29). Captured while normalizing the test suite;
the items below were left out of the first pass because each is either
build-gated (can't verify here) or a judgment-call semantic change.

**Priority.** P3 — test hygiene / drift prevention; no product capability
depends on it. The remaining greenwash surface (198.1) is the highest-value bit
(a real runtime failure can currently report PASS).

**Depends on.** None. 198.2 (zephyr fixture output) wants a working Zephyr build
to verify (the FVP / native_sim path); 198.1/198.3 are host-verifiable.

## Overview

The canonical contracts the suite is converging on:
- **Preconditions:** `nros_tests::skip!(...)` (panics `[SKIPPED]`); never
  `eprintln!` + `return` (reports PASS). `assert!`/`bail!` for hard requirements.
- **Output format:** every talker prints `Published: <n>`, every listener
  `Received: <n>`; tests parse via `nros_tests::output::{parse,assert}_{talker,
  listener}` (count + values), not ad-hoc `String::contains`.
- **Debug/logs:** centralized + opt-in under `test-logs/` (done; nothing here).

Three gaps remain.

## Work items

- [ ] **198.1 — Runtime-failure swallow (greenwash on mid-test failure).** A
      distinct class from the AXIS-1 preconditions: after prerequisites pass, a
      *runtime* failure is caught and turned into an early `return` that reports
      PASS. Found in `nros-tests/tests/rmw_interop.rs` (e.g. `:100`/`:172`/`:411`
      `"Failed to start ROS 2 listener: {e}"` → `return`; `:463`/`:814` `[FAIL]
      ... exited early` → `return`) and `nano2nano.rs` `:173`/`:189` (`[INFO]
      ... exited early — peer mode may not be supported` → `return`). Decide
      per-site: a genuine unmet-capability (e.g. peer mode unsupported) → `skip!`;
      a real failure (ROS 2 available but the process crashed) → `panic!`/`assert!`
      (fail loudly). Do **not** blanket-convert — some tolerate known-flaky ROS 2
      startup; each needs a judgment call. Acceptance: no `[FAIL]`/`Failed to
      start` path silently returns PASS.

- [ ] **198.2 — Normalize Zephyr fixture output to the canonical format.** The
      Zephyr talker/listener fixtures emit alt formats — `data=...` (instead of
      `Published: <n>`) and `Received[<i>]: ...` (instead of `Received: <n>`),
      forcing the format-tolerant checks left in `nros-tests/tests/zephyr.rs`
      (`:39`, `:156`, `:1223`, `:1228`, `:1320`, `:2245`, `:2337`). Rename the
      fixture prints to the canonical `Published: <n>` / `Received: <n>`, then
      replace those tolerant `contains(...||...)` checks with
      `output::assert_talker`/`assert_listener`. **Build-gated:** verify on the
      Zephyr path (native_sim / FVP) before landing — couldn't be built in the
      normalization session (the example build needs the Zephyr SDK + the
      `nros codegen` toolchain). Locate the fixtures under
      `examples/zephyr/**` / the Zephyr fixture sources the tests boot.

- [ ] **198.3 — Route value-extraction loops through `parse_*`.** A few tests
      hand-roll a line loop to pull message values instead of using
      `output::parse_talker(out).values` / `parse_listener(out).values` (which
      already return `Vec<i64>` and pair with `assert_monotonic`):
      `nros-tests/tests/executor.rs:157` (`if line.contains("Received:")` value
      loop) and the `zephyr.rs:39` filter. Convert where the fixture prints the
      canonical format (after 198.2 for the Zephyr ones). Low risk, host-verifiable
      for the non-Zephyr sites.

## Acceptance

- [ ] No test path reports PASS on a runtime failure (198.1) — failures either
      `skip!` (unmet capability) or `panic!` (real failure).
- [ ] Zephyr fixtures print `Published: <n>` / `Received: <n>`; the zephyr tests
      use `assert_talker`/`assert_listener` (198.2), verified on a Zephyr build.
- [ ] `grep` for `contains("Published:")` / `contains("Received:")` in
      `nros-tests/tests/` returns only `wait_for_output_pattern` waiters (no
      hand-rolled assertions or value loops).
- [ ] `cargo test -p nros-tests --no-run` clean.

## Notes

- The first pass (commit `13bd04ca1`) intentionally did **not** touch the
  bool-returning `require_*` helpers — their `#[test]` callers already `skip!` on
  `false` (the canonical pattern), so those paths report `[SKIPPED]` correctly.
- AXIS 3 (debug logs) needed no work: fixture logs are already centralized +
  opt-in under `test-logs/` (gated on `NROS_TEST_LOGS` / `ZENOHD_LOG`), no `dbg!`
  / `/tmp` writes, and the `RUST_LOG=debug` sites are documented + intentional.
