# Phase 198 — Test-suite consistency hardening (remaining items)

**Goal.** Finish the test-suite consistency pass started in the AXIS review
(landed 2026-05-29, commit `13bd04ca1`): every E2E test should (a) report
`[SKIPPED]` — never PASS — on an unmet precondition, (b) parse talker/listener
output through the one canonical helper against one canonical fixture format,
and (c) keep debug output centralized. AXIS 1 (precondition `skip!`s), the
freertos `Loopback received:` → `Received:` rename, the 6 simplest hand-rolled
`contains("Published:"/"Received:")` assertions, and AXIS 3 (debug logs, already
clean) are **done**. This phase tracks the deferred remainder.

**Status.** All items landed (2026-05-29). **198.1 + 198.3 DONE** (commits
`4a404a6e3`, `d75f5b689`): runtime-failure greenwash removed + value-extraction
loops routed through the canonical parser. **198.2 DONE**: the rust Zephyr
listener (the one non-canonical fixture) emits canonical `Received: <n>` and the
`zephyr.rs` tolerances are dropped; native_sim E2E parse rides on the
`zephyr-dual-line` CI.

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

- [x] **198.1 — Runtime-failure swallow (greenwash on mid-test failure). DONE**
      (2026-05-29). Classified per-site by *who* failed:
      - **ROS 2 helper process fails to spawn** → `skip!` (optional ROS 2
        demo-nodes / `example_interfaces` / tooling not installed; the test gated
        on `require_ros2()` upfront, so core ROS 2 is present — a missing helper is
        an unmet capability, not a product failure). Converted in
        `rmw_interop.rs` (listener ×2, publisher, service-call, service-server,
        action-client, action-server, subscriber ×3) and `xrce_ros2_interop.rs`
        (DDS listener/publisher/service-call/action-client/fibonacci-server/
        add_two_ints-server, 6 sites).
      - **The nros side under test exits early** → `panic!` (fail loudly):
        `rmw_interop.rs` `native-rs-action-server` + `native-rs-service-server`
        "exited early" `[FAIL]` checks.
      - **Unmet-capability runtime exit** → `skip!`: `nano2nano.rs` peer-mode
        listener/talker "exited early — peer mode may not be supported".
      Existing `.kill()` cleanup preserved before each `skip!`. Verified
      `cargo test -p nros-tests --no-run` clean; a grep for `Failed to start ROS 2`
      / `[FAIL]` / `exited early` followed by a bare `return;` is now empty.

- [x] **198.2 — Normalize Zephyr fixture output to the canonical format.**
      Audit found only **one** non-canonical fixture: `examples/zephyr/rust/listener`
      emitted `Received[<i>]: <v>`; all talkers (c/cpp/rust) already print
      `Published: <n>` and the c/cpp listeners `Received: <n>` (the `data=`
      tolerance was dead — no Zephyr fixture emits it). Fixes:
      - `examples/zephyr/rust/listener/src/lib.rs` → canonical `Received: <v>`
        (dropped the unused loop counter).
      - `nros-tests/tests/zephyr.rs`: `count_zephyr_received` drops the
        `|| Received[` arm; removed the dead `|| data=` talker/listener
        tolerances (`:156`/`:164`/`:1223`), the `|| Received[` listener tolerance
        (`:1228`), the `count_pattern(…,"Received[")` add (`:1238`), and the
        cpp/xrce `|| data=` (`:1937`); refreshed the stale `Received[N]` comments.
      Acceptance met: `grep 'data=|Received\['` in `zephyr.rs` returns only a
      comment; `cargo test -p nros-tests --no-run` clean. **E2E note:** the
      native_sim boot→parse path is exercised by the `zephyr-dual-line` CI (builds
      the rust listener) + the zephyr test lane — not re-run locally (workspace
      setup is a ~20-min west update; the change is a trivial log-string rename).
      The talker/listener gate checks now read the canonical parser
      (`output::parse_talker(..).published_count > 0` / `count_zephyr_received`)
      instead of ad-hoc `contains` — keeping the multi-condition diagnostic logic
      (session-error / sub-created / error-attribution) that `assert_*`'s
      panic-on-miss would have destroyed. The only remaining `contains("Received:")`
      is inside `count_zephyr_received`, the canonical Zephyr listener counter.

- [x] **198.3 — Route value-extraction loops through `parse_*` (non-Zephyr). DONE**
      (2026-05-29). `executor.rs` ordering test now uses
      `output::parse_listener(&out).values` + `output::assert_monotonic(..)`
      (dropping the hand-rolled `split("Received:")` loop + manual windows check).
      The duplicated local `received_values()` helpers in `qos.rs` + `multi_node.rs`
      now delegate to `parse_listener` (a thin `i64`→`i32` adapter, callers
      unchanged) — one `Received: <n>` extractor for the suite. `cargo test -p
      nros-tests --no-run` clean; no `split("Received:"/"Published:")` outside
      `zephyr.rs`.
      *Remaining (folds into 198.2):* the `zephyr.rs:39` filter tolerates the
      Zephyr alt format (`Received[`), so it converts only after the Zephyr
      fixtures emit the canonical format.

## Acceptance

- [x] No test path reports PASS on a runtime failure (198.1) — failures either
      `skip!` (unmet capability) or `panic!` (real failure).
- [x] Zephyr fixtures print `Published: <n>` / `Received: <n>`; the zephyr tests
      gate on the canonical parser (`output::parse_talker(..).published_count` +
      `count_zephyr_received`), not ad-hoc `contains`. native_sim E2E parse rides
      on the `zephyr-dual-line` CI (builds the rust listener).
- [x] `grep` for `contains("Published:")` / `contains("Received:")` in
      `nros-tests/tests/` returns only the canonical `count_zephyr_received`
      counter helper (no hand-rolled assertions or value loops).
- [x] `cargo test -p nros-tests --no-run` clean.

## Notes

- The first pass (commit `13bd04ca1`) intentionally did **not** touch the
  bool-returning `require_*` helpers — their `#[test]` callers already `skip!` on
  `false` (the canonical pattern), so those paths report `[SKIPPED]` correctly.
- AXIS 3 (debug logs) needed no work: fixture logs are already centralized +
  opt-in under `test-logs/` (gated on `NROS_TEST_LOGS` / `ZENOHD_LOG`), no `dbg!`
  / `/tmp` writes, and the `RUST_LOG=debug` sites are documented + intentional.
