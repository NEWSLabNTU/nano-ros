# Phase 263 — Complete the workspace examples (feature demos + tests)

Status: **Planned (2026-06-19)** · Informs RFC-0024 (multi-node layout), RFC-0026
(examples), RFC-0027 (ROS 2 user workflow) · Book: `getting-started/workspace-*.md`.

> **Goal.** Turn the four product-shaped workspaces (`examples/workspaces/{rust,c,
> cpp,mixed}`) from pub/sub-only connectivity baselines into (a) polished **starters**
> that demonstrate the everyday ROS feature set in all four languages, and (b) a set of
> separate **advanced** workspaces that each demonstrate one nano-ros differentiator —
> with runtime tests asserting every feature actually works.

## Why (the 2026-06-19 review)

Grounded review of `examples/workspaces/*` + the `multi_pkg_workspace_*` fixtures:

- **All four workspaces demonstrate only pub/sub + timer + multi-node.** Rust is the
  flagship (9 entry pkgs across native/freertos/nuttx/esp32/threadx/zephyr, a
  `multihost.launch.xml`, 5 deploy targets, 2 e2e tests). **C / C++ / mixed are
  native-only, single-entry, single-launch, and have NO runtime test** (fixture-built
  but never asserted).
- **Every advanced feature is absent from every workspace** — services, actions,
  parameters (+ param services), QoS overrides, lifecycle, scheduling tiers, E2E
  safety/CRC, cross-RMW bridges, logging, custom `.msg`, composable nodes, advanced
  launch (conditionals / includes / remaps / params / env). Each exists ONLY in a
  single-node `examples/native/*` app or a stub test fixture. A user wanting to see a
  feature *in a real multi-package system* has no example.
- The `multi_pkg_workspace_*` test fixtures (esp_idf / zephyr / nuttx / platformio /
  px4) are mostly **stubs** (codegen-shape checks; px4 `#[ignore]`d; pre-§212.L shape)
  — a different role (toolchain-integration smoke), not feature demos. Out of scope here.
- Housekeeping: the `build*/` `target*/` dirs are **already gitignored + untracked** —
  no cleanup needed.

## Decisions (locked 2026-06-19)

1. **Feature scope: ALL** — the ROS quartet (services, actions, parameters, lifecycle),
   the nano-ros differentiators (scheduling tiers, E2E safety, cross-RMW bridge, QoS),
   advanced launch, and logging + an in-workspace custom-msg package.
2. **Structure: starters + separate advanced workspaces.** Extend the existing four
   workspaces into **good starters** carrying the everyday ROS feature set, kept
   approachable (a minimal default launch + a `showcase` launch). The **differentiators
   and advanced features go to new, separate, single-purpose workspaces** so the starter
   stays a clean onboarding path.
3. **Language parity: all four.** `rust`, `c`, `cpp`, `mixed` reach the same starter
   feature set + tests (not native-only).

## Coverage target (feature → where it lands)

| Feature | Starter (rust/c/cpp/mixed) | Advanced workspace |
| --- | --- | --- |
| pub/sub, timer | ✓ (have) | — |
| services (server + client) | ✓ add | — |
| actions (server + client) | ✓ add | — |
| parameters (declare/get/set + param services) | ✓ add | — |
| lifecycle (managed node) | ✓ add | — |
| logging | ✓ add | — |
| scheduling tiers (RFC-0015) | — | `ws-realtime-<lang>` |
| E2E safety / CRC | — | `ws-safety-<lang>` |
| cross-RMW bridge | — | `ws-bridge` |
| QoS overrides | — | `ws-qos-<lang>` |
| advanced launch (cond/include/remap/env) | minimal in showcase | `ws-launch` (full) |
| custom `.msg`/`.srv` in-workspace | — | `ws-custom-msg-<lang>` |
| multi-platform entries + multihost | ✓ (rust has; add to c/cpp/mixed) | — |

## Tracks & waves

### Track A — Starter workspaces (extend the existing four)
Per language, add the everyday-ROS feature node-pkgs and a `showcase` launch that
composes them; keep `system.launch.xml` (talker+listener) as the untouched minimal
default. Sequence so each wave is shippable on its own.

- **A1 — services. RUST DONE (2026-06-19).** Added `service_server_pkg`
  (`AddServer` — declarative `create_service_server_for_name::<AddTwoInts>` +
  `ctx.message`/`ctx.reply` in `on_callback`) + `service_client_pkg` (`AddClient` —
  `create_service_client_for_name` + a 1 Hz timer that arms a flag in `on_callback`,
  with the blocking `ctx.call_for_name` + `/sum` publish in the per-spin `tick(TickCtx)`
  hook). Added `showcase.launch.xml` (talker+listener+add_server+add_client) +
  `native_showcase_entry` (boots it); the minimal `system.launch.xml`/`native_entry`
  stay the quickstart. system.toml gains the two components; root `Cargo.toml` the
  members. **First workspace example to exercise the declarative service server AND
  client** — `cargo build -p native_showcase_entry` links clean (the macro emits all
  four `register` calls from the showcase launch). **Finding:** the client path needs
  the `tick(TickCtx)` surface (calls/publish), distinct from `on_callback(CallbackCtx)`
  — undocumented + unexercised before this; worth a book note (tracked for A-docs).
  **Runtime e2e DONE (2026-06-23, Track D)** — but cross-process, not in the combined
  `native_showcase_entry`. Running the never-before-run showcase surfaced two bugs:
  (1) the 4-node topology declares 5 callback entries, over the default
  `NROS_EXECUTOR_MAX_CBS = 4`, and the overflow registers as an **opaque**
  `NodeRegister("service_client_pkg")` (**issue 0095**); (2) more fundamentally, an
  **in-process (same-executor) service server+client do not talk** — `add_server` never
  receives `add_client`'s locally-issued query (bisected: `/chatter`✓, direct `/sum`
  publish✓, `/srvhit` server-receipt✗) (**issue 0096**). So the service round-trip e2e
  runs the server + client as **two processes** (the supported topology, mirroring the
  imperative `native_api.rs::test_native_service_communication`): new
  `native_service_server_entry` + `native_service_client_entry` (one-node
  `service_server.launch.xml` / `service_client.launch.xml`), fixtures
  `workspace-rust-native-service-{server,client}`, and
  `tests/service_roundtrip_xprocess_e2e.rs` asserts a `/sum` subscriber sees the
  server-computed sums `1,2,3` (PASS). The native listener gained an `NROS_SUB_TOPIC`
  env override (default `/chatter`) so it can subscribe `/sum`. The combined
  `native_showcase_entry` is left as-is (its in-process service nodes are non-functional
  per 0096; documented, no fixture/test).
  **Remaining:** project to C / C++ / mixed.
  Port from `examples/native/{rust,c,cpp}/service-*`.
- **A2 — parameters.** `param_pkg` (declare/get/set; enable `[param_services]` /
  `features=["param_services"]` so external get/set works). Port from
  `examples/native/cpp/parameters`.
- **A3 — lifecycle. RUST DONE (2026-06-20, via phase-264 W2).** Was gated (the macro
  didn't wire `[lifecycle]`); phase-264 W2 fixed that, so the new
  `examples/workspaces/ws-lifecycle-rust` (a managed system: `[lifecycle] autostart =
  "active"` + `nros/lifecycle-services`) builds via plain-cargo `nros::main!` — the
  macro emits `apply_lifecycle` → the runtime registers the 5 REP-2002 services + drives
  Configure→Activate. `cargo build -p native_entry` links clean. (Transition-callback
  hooks on the declarative node are still a separate gap; this is the managed-node demo.)
- **A4 — actions. RUST DONE (2026-06-24, Track D) — cross-process.** New
  `action_server_pkg` (declarative Fibonacci server on `/fibonacci`: accepts the goal in
  `on_callback`, drives feedback + `complete_goal` in `tick`, mirroring the orchestration
  test's `fib_server`) + `action_client_pkg` (declarative client via
  `create_action_client_with_callbacks_for_name` — sends one goal `order=10` in `tick`,
  and on the auto-delivered result `on_callback("cb_fib_result")` republishes the result's
  last sequence element — 55 — on `/fib_result`). First WORKSPACE exercise of the
  declarative action server AND client (the orchestration test used the imperative client
  against a declarative server). Runs cross-process (issue 0096): new
  `native_action_server_entry` + `native_action_client_entry` (one-node
  `action_server.launch.xml` / `action_client.launch.xml`), fixtures
  `workspace-rust-native-action-{server,client}`, and
  `tests/action_roundtrip_xprocess_e2e.rs` asserts a `/fib_result` subscriber sees `55`
  (PASS). Uses the workspace's generated `example_interfaces::action::Fibonacci`.
  Remaining: project to C / C++ / mixed.
- **A5 — logging. RUST DONE (2026-06-24, Track D).** Was gated on the board not
  initing a sink; **phase-264 W3 fixed that** (`nros-board-posix` calls
  `nros_log::init(sinks::default())` at boot). So the ws-rust `talker_pkg` now logs
  `"talker publishing chatter seq=<n>"` each tick via `nros_log::nros_info!(
  &DEFAULT_LOGGER, …)` (added an `nros-log` dep), and booting the `native_entry`
  shows the line on the entry's own stdout (`[INFO] nros: talker publishing …`) —
  the chain board boot-time `init` → global sink → `DEFAULT_LOGGER.dispatch` → host
  stdout. `tests/logging_workspace_e2e.rs` asserts ≥3 log lines on the entry stdout
  (PASS). Logging is process-local (no subscriber, unlike pub/sub delivery — issue
  0096). Remaining: project to C / C++ / mixed.

### Track B — Advanced workspaces (new, single-purpose, separate dirs)
Each is a minimal product-shaped workspace demonstrating ONE differentiator end-to-end.

- **B1 — `ws-safety-<lang>`. RUST DONE (2026-06-20).** New `examples/workspaces/
  ws-safety-rust`: `talker_pkg` (publishes /chatter — CRC attached by the backend) +
  `safe_listener_pkg` (declares `create_subscription_for_callback_name_with_safety`,
  reads `CallbackCtx::integrity()` under `#[cfg(feature = "safety-e2e")]`). `system.toml`
  declares `features = ["safety"]`; the plain-cargo `native_entry` wires the
  `safety-e2e` features explicitly (`nros-board-native/safety-e2e` → backend CRC;
  `safe_listener_pkg/safety-e2e` → `nros/safety-e2e`, cargo-unified). `cargo build -p
  native_entry` links clean (38.7s). **First WORKSPACE demo of the E2E-safety
  differentiator.** **Runtime e2e DONE (2026-06-23, Track D)** — but cross-process: the
  combined `native_entry` (talker + safe_listener in one process) can't deliver in-process
  (issue 0096 — a same-session subscriber never receives the same-process publisher), so
  the demo splits into `native_safety_talker_entry` + `native_safety_listener_entry`
  (one-node `safety_talker.launch.xml` / `safety_listener.launch.xml`, both baking
  `safety-e2e`). `safe_listener` republishes the running count of CRC-**valid** messages
  on `/safe_ok`; fixtures `workspace-rust-native-safety-{talker,listener}` +
  `tests/safety_workspace_e2e.rs` assert a `/safe_ok` subscriber sees the count climb —
  proving the E2E CRC attach→validate→`integrity().is_valid()`→republish path (PASS).
  Remaining: project to C/C++ (the `NANO_ROS_SAFETY_E2E` knob is wired by phase-261 W5).
  Note: a bake build derives the `safety-e2e` features from `system.toml` automatically
  (phase-261 W3); the hand-cargo entries set them explicitly.
- **B2 — `ws-realtime-<lang>`. RUST DONE (2026-06-20).** New `examples/workspaces/
  ws-realtime-rust`: a 10 ms control node on tier `high` + a 100 ms telemetry node on
  tier `low`. Each Node pkg declares `callback_groups = [{ id, tier }]` in Cargo
  metadata + `node.callback_group(id)` at runtime; `system.toml [tiers.high|low.posix]`
  gives the priorities. **`nros::main!` reads both, resolves the 2-tier table, and emits
  the multi-tier `run_tiers` entry** (RFC-0032 §5) — confirmed by `cargo build -p
  native_entry` (14.5s). Unlike lifecycle, the macro DOES wire tiers
  (`main_macro.rs` imports `resolve_tiers`). First WORKSPACE demo of deployment-time
  real-time scheduling. **Runtime e2e DONE (2026-06-23, Track D).** The two nodes were
  pure timers ticking into a no-op `declarative_component!` default — nothing observable.
  Extended each to PUBLISH a monotonic counter (control → `/ctrl` @10 ms, telem →
  `/telem` @100 ms; added `std_msgs` `<depend>` + a real `ExecutableNode`, dropping the
  empty `declarative_component!`). Fixture `workspace-rust-native-realtime` +
  `tests/realtime_tiers_e2e.rs`: two `/ctrl`+`/telem` subscribers, anchor on the slow
  tier (telem≥5), assert the high tier published ≥3× the low tier — proving `run_tiers`
  scheduled **both** tiers at their declared cadences (PASS). (Tier *priority* preemption
  is advisory on native; the rate assertion proves both tiers run.) Remaining: project to
  an RTOS deploy (freertos/threadx) where priorities are real tasks.
- **B3 — `ws-bridge`:** cross-RMW gateway (zenoh ↔ xrce/cyclonedds), from
  `examples/bridges/*`, but as a workspace bringup (`[[bridge]]` in system.toml).
- **B4 — `ws-qos-<lang>`:** QoS overrides (reliability / durability / deadline) +
  status events (deadline-missed / liveliness), from the book's documented surface.
- **B5 — `ws-launch`:** advanced launch — conditionals, includes, remaps, params, env
  in the bringup XML; exercises the planner end-to-end.
- **B6 — `ws-custom-msg-<lang>`:** an in-workspace `.msg`/`.srv` interface package
  (`nros generate-rust` / `nros_generate_interfaces`), from `examples/native/rust/custom-msg`.

### Track C — Platform parity (C / C++ / mixed)
Give the starter C/C++/mixed workspaces the embedded entries Rust already has
(freertos / nuttx / zephyr / esp32 / threadx) + a `multihost.launch.xml` + robot1/2
deploy targets. Reuses the Rust workspace's per-platform Entry pattern.

### Track D — Tests (close the C/C++/mixed gap)
A runtime e2e test per workspace + per feature, asserting behaviour (not just a build):
- starter: service call returns, action goal completes, param get/set round-trips,
  lifecycle transitions, log line appears.
- advanced: CRC catches a corrupted frame, tiers schedule by priority, bridge forwards
  across RMWs, QoS deadline-missed fires.
- Each as a build-stage fixture (`examples/fixtures.toml`) + a `nros-tests` consumer
  (no compile-in-test — prebuilt fixture, per AGENTS.md). C/C++/mixed currently have
  zero runtime tests; this is the biggest correctness win.

## RE-SEQUENCE (2026-06-19) — declarative-API gaps found during A1/A2

Implementing A1 (services) + starting A2 (parameters) surfaced that the **declarative
Node-pkg API does not yet support several features** the plan assumed (issue **0089**):

- **A2 (parameters): BLOCKED.** No runtime parameter-value read on
  `CallbackCtx`/`TickCtx` — a Node-pkg can `declare_parameter` but not read its value.
- **A1 for C/C++/mixed: degraded.** C/C++ service-in-component is raw-CDR only (no
  typed `bind_service<C,&C::method>`); a faithful demo needs an API add.
- **A1 service CLIENT (Rust): shipped but via the undocumented `tick(TickCtx)` surface**
  (calls can't run in `on_callback`). A1 is the first to exercise it — needs a book note.

These are real API-maturity gaps, not example bugs. Per the plan's guardrail ("don't
fake the demo"), **re-sequence to the features the declarative API FULLY supports**,
and gate the rest behind issue 0089:

- **Do next (fully supported in the declarative shape):** **B1 safety — DONE** (no
  new node-API needed; `ws-safety-rust` ships). Then **B2 tiers** (system.toml
  `[tiers]` + `callback_groups`, no runtime API) and **A5 logging** (the `nros-log`
  facade). **A3 lifecycle is also GATED** (0089 #3 — the macro doesn't wire
  `[lifecycle]` for the cargo shape).
- **Gated on 0089 (mature the API first):** A2 parameters; A1 C/C++/mixed (typed
  service bind); A4 actions client side (same `tick` surface as services-client).
- **A1 Rust services: DONE** (server + client both build; see A1 above).

## Sequencing (original)

A1 → A2 → A3 → A4 → A5 (starter, rust first as the reference, then c/cpp/mixed per
wave), interleaving Track D tests as each feature lands. Track B advanced workspaces
are independent — pick up after the starter quartet (B1 safety first; it's already
wired by phase-261). Track C parity last (most build-infra, least feature-novel).

Each wave: implement for Rust (reference), then project to c/cpp/mixed; add the fixture
+ runtime test; update the book `getting-started/workspace-*.md` + the workspace README
coverage matrix; `just ci`.

## Acceptance

- Each starter workspace boots its `showcase.launch.xml` with services + actions +
  parameters + lifecycle + logging working, in all four languages, with a runtime test
  per feature.
- Each advanced workspace demonstrates its one differentiator end-to-end with a test.
- The README in `examples/workspaces/` carries a feature × language × workspace matrix
  so a user can find "feature X in a real workspace" in one place.
- No feature remains demonstrated ONLY in a single-node example or a stub fixture.

## Risks / notes

- **Scale.** This is large (≈6 starter feature-pkgs × 4 languages + 6 advanced
  workspaces + tests). Waves are sized to ship independently; the starter quartet (A1–A4
  for Rust) is the highest-value first slice.
- **C/C++ feature maturity.** Some features are richer in Rust today (lifecycle,
  actions). Where the C/C++ API lags, the wave surfaces the gap as a tracked issue
  rather than faking the demo (tests must fail-loud on an unimplemented path).
- **Don't bloat the starter.** The minimal `system.launch.xml` stays talker+listener;
  features live behind `showcase.launch.xml` so the onboarding path is still small.
- **Reuse, don't duplicate.** Port node logic from the existing `examples/native/*`
  single-node apps into reusable node-pkgs; the workspace is the composition, not new
  node code.
