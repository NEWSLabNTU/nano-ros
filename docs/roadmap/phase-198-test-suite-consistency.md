# Phase 198 — Test-suite consistency hardening (remaining items)

**Goal.** Finish the test-suite consistency pass started in the AXIS review
(landed 2026-05-29, commit `13bd04ca1`): every E2E test should (a) report
`[SKIPPED]` — never PASS — on an unmet precondition, (b) parse talker/listener
output through the one canonical helper against one canonical fixture format,
and (c) keep debug output centralized. AXIS 1 (precondition `skip!`s), the
freertos `Loopback received:` → `Received:` rename, the 6 simplest hand-rolled
`contains("Published:"/"Received:")` assertions, and AXIS 3 (debug logs, already
clean) are **done**. This phase tracks the deferred remainder.

**Status.** In progress (2026-05-29). **198.1 + 198.3 DONE** (commits
`4a404a6e3`, `d75f5b689`): runtime-failure greenwash removed + value-extraction
loops routed through the canonical parser. **198.2 (Zephyr fixture output)
handed off to another agent** — it is the last open item and is build-gated
(needs a Zephyr build to verify); see its handoff detail below.

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

- [ ] **198.2 — Normalize Zephyr fixture output to the canonical format.**
      *(Owned by another agent — handoff 2026-05-29.)* The Zephyr talker/listener
      fixtures emit alt formats — `data=…` (instead of `Published: <n>`) and
      `Received[<i>]: <v>` (instead of `Received: <n>`), forcing the
      format-tolerant checks scattered through `nros-tests/tests/zephyr.rs`. This
      is the **last open 198 item** (198.1 + 198.3 landed). Handoff detail:
  - **Fixtures to normalize** (`examples/zephyr/`): the `Received[{}]: {}` print
    is `rust/listener/src/lib.rs:101`; mirror prints live in `rust/talker`,
    `cpp/talker`, `cpp/listener`, `cpp/cyclonedds/talker-aemv8r` (grep
    `examples/zephyr` for `data=` / `Received[`). Change them to the canonical
    `Published: <n>` / `Received: <n>` (drop the `[index]` + `data=` forms).
  - **Test sites to simplify afterward** (`nros-tests/tests/zephyr.rs`): the
    `||`-tolerant checks at `:39`, `:156`, `:164`, `:1223`, `:1228`, `:1320`,
    `:1940`, `:2245`, `:2337` + the `count_pattern(.., "Received[")` /
    `count_zephyr_received` helpers (`:1241` and the comments at `:392`/`:546`).
    Once the fixtures emit the canonical format, replace these with
    `output::assert_talker`/`assert_listener` / `parse_listener(..).values` and
    retire `count_zephyr_received`.
  - **Build-gated:** verify on the Zephyr path (native_sim / FVP) before landing
    — the example build needs the Zephyr SDK + the `nros codegen` toolchain
    (couldn't be built in the normalization session). This is why it was deferred,
    not skipped.

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
