# Phase 96 — Phase 95 cross-process E2E follow-ups

**Goal**: Close the three known issues that remained after Phase 95
shipped its 51 example crates. Each issue blocks one or more
cross-process E2E tests that are currently `#[ignore]`d. None of
them are example-coverage gaps — the example crates themselves all
build clean and reach readiness — but each blocks an interop test
that proves end-to-end behaviour.

**Status**: All three closed.

**Priority**: Medium. Phase 95's matrix is complete; these are
quality-of-life follow-ups that turn `#[ignore]`d tests back on so
the regression suite catches future breakage in service / action
paths and in the cpp-xrce session demux.

**Depends on**: Nothing — each item is independent and small.
Coordinate with Phase 71.28 / 71.29 for the dust-dds half (those are
listed there because they're dust-dds-internal); this doc tracks the
two non-dust-dds items + the cross-link.

## Work Items

- [x] 96.1 — Root cause was a stack of three independent bugs,
      not a `try_recv()` demux bug:
      1. **XRCE session-key collision.** The cpp `nros::init()`
         wrapper hardcoded the session key to `hash("nros_cpp")`
         for every cpp process — two cpp participants on the
         same agent collided as one client, so topic publishes
         weren't cross-routed. Added
         `init(locator, domain_id, session_name)` overload +
         `Executor::create(... session_name)`; updated all 6
         zephyr cpp/xrce examples to pass distinct names.
      2. **Action result reply offset off-by-3.** `arena.rs`
         `result_offset = 5` skipped only the CDR header and
         status byte but missed the `align(4)` pad that
         `try_handle_get_result_raw` inserts before the payload
         (correct offset = 8). The leaked pad bytes blew up
         `ffi_deserialize`. Same fix in
         `nros_cpp_action_client_poll` (poll path) and
         `nros_cpp_action_client_try_recv_result` (Future path).
      3. **Action feedback offset off-by-4 + missing stash.**
         `arena.rs` `offset = CDR_HEADER_LEN + GoalId::UUID_LEN`
         (= 20) missed the `write_u32(16)` length-prefix that
         `write_goal_id` emits before the UUID bytes (correct
         offset = 24). Plus the polling path needed a
         `FEEDBACK_STASH` filled by the trampoline so
         `try_recv_feedback()` wouldn't return empty after the
         arena's auto-dispatch consumed the message.

      Re-enabled tests:
      `test_zephyr_xrce_cpp_talker_listener` (24 s),
      `test_zephyr_xrce_cpp_service_e2e` (37 s),
      `test_zephyr_xrce_cpp_action_e2e` (69 s).
      Full cpp/xrce nextest suite: 9/9 pass.
- [x] 96.2 — `test_talker_param_declaration` waits directly for
      the assertion target (`"Counter start value"`, 15 s window)
      instead of `"Publishing"` + 2 s grace. Eliminates the
      `just test-all`-load timing dependency.
- [x] 96.3 — Cross-link to Phase 71.28 / 71.29: **closed**.
      71.28 (slice-offset bug in `ServiceServerTrait::handle_request`),
      71.29 (cooperative-runtime starvation in DDS Cortex-A9
      example clients), and the 71.29 async follow-up
      (embassy-time driver + manual Promise poll) all landed.
      Re-enabled tests:
      `test_dds_service_server_client_e2e`,
      `test_dds_action_server_client_e2e`,
      `test_zephyr_dds_rust_service_a9_e2e`,
      `test_zephyr_dds_rust_action_a9_e2e`,
      `test_zephyr_dds_rust_async_service_a9_e2e`. Remaining
      open: `nros-rmw-dds` doesn't yet implement
      `ServiceClientTrait::register_waker` /
      `Subscriber::register_waker`, so the async example uses a
      manual `try_recv` poll loop instead of `.await`. Tracked
      separately as a 71.x follow-up.

### 96.1 — cpp/xrce subscription demux on shared agent

**Symptom**: Two cpp/xrce participants on the same XRCE Agent
(`MicroXRCEAgent` on port 2018) — one publishes, the other
subscribes. Talker prints `Published: 1..10` cleanly; listener
stays in `nros::spin_once(100) + sub.try_recv(msg)` loop without
ever logging `Received: …`.

**Why it's specific to cpp**: The matching rust/xrce and c/xrce
talker↔listener pairs work fine on the same agent under the same
test harness (Phase 95.A `test_zephyr_xrce_rust_talker_listener` and
the existing `test_zephyr_xrce_c_talker_listener`). The bug is on
the cpp-API session shape — most likely in
`nros::Subscription::try_recv()` or the cpp → nros-c FFI demux that
splits incoming agent messages by topic.

**Tests this would re-enable** (all currently `#[ignore]`d in
`packages/testing/nros-tests/tests/zephyr.rs`):

* `test_zephyr_xrce_cpp_talker_listener`
* `test_zephyr_xrce_cpp_service_e2e`
* `test_zephyr_xrce_cpp_action_e2e`

**Hypothesis to verify first**: instrument `nros_cpp::Subscription`
with a counter that bumps on every `try_recv` call that returns
`Ok(Some(_))`, and a separate counter for the underlying nros-c
`nros_subscription_take` call. If the C-level call returns data but
the cpp-level wrapper drops it, the bug is in the cpp wrapper. If
the C-level call never returns data, walk the agent → nros-c demux
path.

**Files**: `packages/core/nros-cpp/src/subscription.rs`,
`packages/core/nros-cpp/include/nros/subscription.hpp`,
`packages/core/nros-c/src/subscription.rs`.

### 96.2 — `test_talker_param_declaration` flake fix

**Symptom**: `test_talker_param_declaration` in
`packages/testing/nros-tests/tests/params.rs:121` panics with
"Should log counter start value. Output: …" when run as part of
`just test-all` under load. Passes deterministically when run
solo (`cargo nextest run … test_talker_param_declaration`).

**Cause (educated guess)**: timing-sensitive assertion that scans
the talker's stdout for a counter-start line. When `test-all` runs
~50 tests in parallel and the host is loaded, the talker boots
slowly enough that the harness's read window misses the line.
Native zenoh talker, no involvement of the recently-added Phase 95
work.

**Fix shape**: replace fixed-window scan with `wait_for_pattern`
(or extend the timeout). Pattern: see `wait_for_pattern` calls in
`tests/zephyr.rs` for the right shape — `Duration::from_secs(15)`
is the standard upper bound for native zenoh boot under load.

**Files**: `packages/testing/nros-tests/tests/params.rs:121`.

### 96.3 — Cross-link to Phase 71.28 / 71.29

The dust-dds service request/reply SEDP issue and the Cortex-A9 GEM
RX queue tuning are tracked under Phase 71 (dust-dds platform-
agnostic) because they're dust-dds backend bugs, not Phase 95
work. Once 71.28 / 71.29 land, flip the matching `#[ignore]`d
tests back on:

* In `packages/testing/nros-tests/tests/zephyr.rs`:
  `test_zephyr_dds_rust_service_a9_e2e`,
  `test_zephyr_dds_rust_action_a9_e2e`,
  `test_zephyr_dds_rust_async_service_a9_e2e`.
* In `packages/testing/nros-tests/tests/dds_api.rs`:
  `test_dds_service_server_client_e2e`,
  `test_dds_action_server_client_e2e`.

Each test has a `// **#[ignore]d**: …` comment that quotes the
underlying bug and the re-enable condition; the diff is just
removing the `#[ignore]` line once 71.28 closes.

## Acceptance Criteria

- [ ] 96.1 — cpp/xrce talker→listener interop on a shared agent:
      listener logs `Received: <data>` for at least 5 distinct
      messages within 10 s. The 3 cpp/xrce E2E tests in
      `tests/zephyr.rs` flip from `#[ignore]` to passing in
      `just zephyr test`.
- [ ] 96.2 — `test_talker_param_declaration` passes in
      `just test-all` under typical CI load (no flakes in
      ≥ 10 consecutive runs).
- [ ] 96.3 — When Phase 71.28 / 71.29 land, the matching 5 dds
      E2E `#[ignore]`s in this phase's "tests this would re-enable"
      lists are removed and the tests pass.
- [ ] `just ci` passes.

## Notes

* **Bug-fix phase, not feature.** No new APIs or example crates.
  Each item is a localised fix to an existing code path.
* **Phase 95 stays complete.** This phase doesn't reopen 95's
  acceptance criteria — those were met when the 51 example crates
  landed. The `#[ignore]`d cross-process E2E tests are tracked here
  so they're not lost; they aren't a Phase 95 deliverable.
* **No deadline.** Pick up when the matching backend (dust-dds /
  nros-cpp xrce / param test) is being touched for another reason.
