# Audit findings — 2026-07-28 (deep, C + E)

- Depth: **deep** (Workflow orchestration; main loop located, agents only read and judged)
- Categories: **C** (API design & layering) and **E** (testing) only
- Target: `ef967d01e`; issue baseline = the same day's quick run
  (`audit-findings-2026-07-28.md`, which filed #321–#336)
- Lanes: api-shape (inherit/high) · axis-config · test-discipline · test-matrix ·
  test-isolation (sonnet/medium)
- Spend: 7 agents (5 finders + 2 refuters), 638k subagent tokens, 232 tool calls, ~17 min
- Verification: tiered single-refuter — **20 candidates → 12 P2 + 7 P3, zero P1**;
  2 findings met the refute trigger (low confidence or expensive fix), **1 survived,
  1 refuted**. The lead then independently re-verified the four findings filed as
  issues below (the report-only tier is unverified by contract).

## Why zero P1

Both P1-shaped candidates were downgraded on reading: the C7 `destroy_*`-returns-void
claim was **refuted** (see below), and the `native_api.rs` vacuous assertion turned out
to be shadowed by a real positive assertion two lines above it. That is the expected
shape after the quick run already harvested this surface's P1s (#321–#324).

## Refuted (1)

- **C7** · `rmw_vtable.h:68` · "four of five `destroy_*` slots return `void`, so backend
  cleanup failure is unreportable" — **refuted**: `rmw_vtable.h:41` documents the
  convention seven lines above the cited line ("`destroy_*`: void (best-effort
  cleanup)"), reinforced at :31-32. A documented deliberate exception, not drift.
  *(Exactly the failure mode the refuter was briefed on — the quick run lost a P1 to a
  guard 12 lines above the claim.)*

## Lead-verified and FILED (4 issues, 7 findings)

- **D** · `templates/service_nros.rs.jinja` + `action_nros.rs.jinja` · P2 · **new** ·
  RFC-0033 `heap`/`borrowed` is resolved for srv/action fields (`srv.rs:306,324`,
  `action.rs:401-441` all call `field_to_nros_field_with_mode`) but **only the message
  template branches on it** — verified by count: `message_nros.rs.jinja` has 12
  `is_heap`/`is_borrowed` branches, `service_nros.rs.jinja` and `action_nros.rs.jinja`
  have **zero**. A heap-configured srv field therefore gets a `heap::Vec<T>` struct field
  and a `heapless::Vec::new()` serde body — **generated code that cannot compile**.
  → **#343**
- **D** · `generator/common.rs:748` · P2 · **new** · The same `nros-codegen.toml` entry is
  accepted or rejected differently by the three emitters (Rust: any string or sequence;
  C: strings + primitive/string/bounded/nested sequences; C++: only primitive sequences,
  hard-errors on heap strings), so one config generates cleanly for Rust and C and fails
  codegen for C++. → **#343**
- **C6** · `nros-cpp/include/nros/executor.hpp:139` · P2 · **new** · `Executor::spin` means
  the **opposite** of its counterparts: bounded `spin(duration_ms, poll_ms)` with no no-arg
  overload, while C `nros_executor_spin(executor)` and Rust `Executor::spin(timeout) -> !`
  run forever, and the sibling free function `nros::spin()` blocks until `!ok()`.
  Upstream: `rclcpp::Executor::spin()` blocks until shutdown; the bounded verb is
  `spin_some(max_duration)`. → **#338**
- **C6** · `nros-c/include/nros/nros_generated.h:4168` · P2 · **new** · The C executor
  renames rclc's `rclc_executor_add_*` to `nros_executor_register_*` for seven entity
  kinds but leaves `nros_executor_add_client` on the rclc spelling — and every doc comment
  in the family still reads "Add a X to the executor". A rename of a standard concept that
  is not even internally consistent. → **#338**
- **C6** · `nros-cpp/include/nros/rclcpp_compat.hpp:477` · P2 · **new** ·
  `spin_until_future_complete`'s shim **ignores the future** on the explicit-timeout path:
  it calls the bounded `Executor::spin(timeout_ms)`, so it always burns the whole timeout
  even when the future is already ready, and returns `void` so the caller cannot tell
  SUCCESS from TIMEOUT. Upstream returns `rclcpp::FutureReturnCode` and returns as soon as
  the future is ready. → **#339**
- **C1** · `nros-cpp/include/nros/parameter.hpp:559` · P2 · **new (recurrence of #226)** ·
  `ParameterServer` still keeps array-parameter storage in a hand-rolled bump arena in the
  public header (`seq_pool_`, `align_up`, an out-of-band `uint64_t` capacity word) and
  recovers the capacity on `set` by reading `reinterpret_cast<const uint64_t*>(cur)[-1]`
  off a pointer returned by `nros_param_get_*_array` — an **undocumented pointer-identity
  contract** with the C server, living in the shim. #226 fixed the sequence-storage engine;
  the arena came back in a different shape. → **#340**
- **E5/E6** · `packages/testing/nros-tests/src/matrix.rs:90` and `:608` · P2 · **new** ·
  The matrix SSoT diverges from the supported axes in two ways: (a) the **uORB** backend is
  claimed supported in ARCHITECTURE §2 and has a real crate + example, but the `Rmw` enum
  defines only `Zenoh`/`Cyclonedds`/`Xrce`, so the cell **cannot be expressed at all**;
  (b) `cell(ZephyrNativeSim, Cpp, Cyclonedds, Qos, Interop, Runtime)` is declared, but the
  only test that could satisfy it (`qos_zephyr_ros2_interop_e2e.rs`) boots the **Rust**
  `ws-qos-rust` image over **zenoh-pico → rmw_zenoh_cpp** — verified by reading both. So a
  Cpp/Cyclonedds cell is asserted-covered by nothing, and the real Rust/Zenoh coverage is
  unmodelled. → **#341**

## Report-only P2 (unverified by contract, 3)

- **C4** · `packages/boards/nros-board-common/src/platform_config.rs:357` · RFC-0049's
  capability ladder is platform-only in code, though the module doc-comment and RFC-0049
  prose promise board-file `[capabilities]` deltas merge in the way `[knobs.*]` does — a
  declared-but-unimplemented resolution rung.
  · fix: implement the board-capabilities overlay (same shape as `resolve_tx`) or correct
  RFC-0049 + the doc-comment to say capabilities are platform-only.
- **C1** · `nros-cpp/include/nros/nros.hpp:109` · The global `nros::spin(duration_ms,
  poll_ms)` budgets by **iteration count** (`elapsed += timeout`) — the exact defect
  `Executor::spin` documents as fixed in Phase 118.C — so an early `nros_cpp_spin_once`
  return (signalled wake condvar) exits the loop long before `duration_ms` of wall time.
  Two copies of one budget, one carrying a known-fixed bug. **Appended to #329** rather
  than filed: #329's fix (one CFFI spin entry point) subsumes it.
- **E9** · `tests/orchestration_tiers_freertos.rs:68` · Hand-rolls
  `Command::new("timeout").args(["10","qemu-system-arm",…])` outside the `qemu.rs`
  interpreter with no sanctioned-bypass rationale — while the **next test in the same
  file** uses `QemuProcess::start_mps2_an385_networked`, proving the interpreter covers the
  board. Pairs with the E8 bare `start_slirp(7447)` in the same file (the only bare port
  literal among 14 call sites, and inside the allocator's own 7400–7799 window).
  → **#342**

## P3 (7, not filed)

- **C6** · `cmake/NanoRosVerbs.cmake:253` · **VERIFIED** · latent case-normalization defect
  in `nano_ros_auto_add_library`: `_nros_infer_lang` emits lowercase `c`/`cpp`, so
  `_lang STREQUAL "C"` at :253 is dead (no `LINKER_LANGUAGE C`) and :264 links
  `NanoRos::NanoRosCpp` even for pure-C libraries. The repo's own documented pitfall
  ("case-normalize enum-ish cmake args") — third recurrence.
- **C7** · `rmw_vtable.h:81` · Phase-301 aligned entity NOUNS with rmw.h but left the VERBS
  undecidable from their counterparts: `try_recv_raw` for `rmw_take`, `send_reply` for
  `rmw_send_response`, `try_recv_reply_raw` for `rmw_take_response`,
  `service_server_available` for `rmw_service_server_is_available`.
- **C7** · `rmw_ret.h:23` · The public ABI header documents a calling convention no slot
  has any more: "Pointer-returning calls (`open`, `create_publisher`, …) signal failure
  with `NULL`" — every `create_*` now takes an `out` struct and returns
  `rmw_ret_t`, and `open` was renamed `create_session`.
- **E3** · `nros-cli-core/src/cmd/ws.rs:3119` · `lookup_table_covers_w6_example_flip_extras`
  bakes a wave number into a test name.
- **E3** · `rosidl-codegen/src/config.rs:42` · `StorageMode::is_phase1_supported()` is a
  **public production API** whose name bakes a rollout-phase number; its sibling test
  `mode_phase1_gate` repeats it. (Also flagged in the quick run's P3 list.)
- **E4** · `tests/init_api.rs:154` · `nros_init_with_launch_auto_applies_xml_params` is a
  permanently-`#[ignore]`d, **empty-bodied** placeholder gated on a "Phase 212.L.5
  follow-up" that was never wired — distinct from #328's fixture-resolver set.
- **E7** · `tests/native_api.rs:896` · `assert!(!client_output.contains("[OK]"))` checks for
  a marker the C++ action client never prints (verified: zero occurrences in
  `examples/native/cpp/action-client/src/main.cpp`), so the negative assertion cannot fail.
  Downgraded from the finder's P2 because the **positive** assertion two lines above
  ("Goal was rejected by server") carries the test — this is decoration, not a hidden bug.
  Fix: assert via an `output.rs` constant so the coupling is checked.
- **E6** · `tests/c_riscv_nuttx_e2e.rs:30` · The `(NuttxRiscv, C, Zenoh, Pubsub, Example)`
  cell is proven by a single hand-written `#[test]` that imports `matrix::{Lang,
  PlatformId, Workload}` only to derive a port constant — a new instance of the tracked
  per-cell class (#327), listed for the migration's benefit, not filed.

## Coverage

- **api-shape** read all 5 ABI headers + 44 vtable slots, the C++ and C public header
  inventories (39 + 31 files), all 25 jinja templates, and the cmake verb definitions;
  did **not** read `nros-cpp/src/lib.rs` or `nros-c/src/*.rs` in depth (~15k lines of
  CFFI), so C1 violations that live in the Rust-side shim rather than the headers are
  uncovered.
- **axis-config** was told not to re-litigate the quick run's C5 triage and spent its
  budget on the RFC-0049 implementation; C5 outside the two known hit lists came back
  clean.
- **test-matrix** did the cell-by-cell cross-read the quick pass could not, which is where
  both #341 findings came from.
- **Not covered at all:** C3 (generated-vs-handwritten boundary) beyond the D lane's
  template reading; E2 across every `src/` helper's callers (the quick run sampled ~15);
  per-example `config.toml` handling for C4.
- **Recommended next:** the D storage-mode incoherence (#343) wants a codegen test that
  puts a heap field on a `.srv` — it is the kind of gap that only a fixture can hold shut.
