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
- **A2 — parameters. RUST DONE (2026-06-20/24, via phase-264 W4).** Was BLOCKED (no
  runtime parameter-VALUE read on `CallbackCtx`); phase-264 W4 added
  `CallbackCtx::parameter::<T>(name)` / `TickCtx::parameter` (the live read, gated on
  `param-services`) + the launch-baked initial via `ctx.param`. The param demo landed
  as the single-purpose workspace **`examples/workspaces/ws-params-rust`** (same
  separate-`ws-<cap>` shape as A3 lifecycle / B1 safety — keeps the minimal starter
  free of system-wide `[param_services]`): `param_talker_pkg` declares
  `publish_period_ms` via the launch `<param>`, reads the LIVE value each tick with
  `ctx.parameter::<i64>`, and publishes it; `system.toml` declares `[param_services]`,
  the `native_entry` enables `nros/param-services`. **Tests:**
  `tests/param_live_read_e2e.rs` (nros↔nros — a subscriber observes the baked initial
  `250` on the wire, proving the W4c live-read chain) + `tests/params.rs` (the `ros2
  param set` reconfig round-trip). Verified: the params entry builds clean (declare +
  live-read node compiles + links); `param_live_read_e2e` compiles + is fixture-wired
  (runtime green is CI-side, via the prebuilt+stamped workspace fixture). **Remaining:**
  project to C / C++ / mixed (Track C/D).
- **A3 — lifecycle. RUST DONE (2026-06-20, via phase-264 W2).** Was gated (the macro
  didn't wire `[lifecycle]`); phase-264 W2 fixed that, so the new
  `examples/workspaces/ws-lifecycle-rust` (a managed system: `[lifecycle] autostart =
  "active"` + `nros/lifecycle-services`) builds via plain-cargo `nros::main!` — the
  macro emits `apply_lifecycle` → the runtime registers the 5 REP-2002 services + drives
  Configure→Activate. `cargo build -p native_entry` links clean. (Transition-callback
  hooks on the declarative node are still a separate gap; this is the managed-node demo.)
  **Runtime e2e DONE (2026-06-24, Track D).** Fixture `workspace-rust-native-lifecycle`
  + `tests/lifecycle_workspace_e2e.rs` (ROS 2 interop lane): boots the entry, discovers
  the managed node via `ros2 lifecycle nodes`, and asserts `ros2 lifecycle get` reports
  **`active`** — proving autostart drove Configure→Activate at boot with **no manual
  transition** (the workspace's distinguishing behaviour vs the standalone
  `ros2_lifecycle_interop` test). **Verified locally** (the `build/rmw_zenoh_ws` overlay
  is present); skips (per the ROS 2 contract) where `rmw_zenoh_cpp` is absent.
  Remaining: project to C / C++ / mixed.
- **A4 — actions. RUST DONE (2026-06-24).** Added `action_server_pkg` (declarative
  `FibonacciServer`: `create_action_server_for_name_with_callbacks::<Fibonacci>`, goal/cancel
  decisions in `on_callback`, feedback + `complete_goal` driven from `tick` via
  `for_each_active_goal_for_name`) + `action_client_pkg` (declarative `FibonacciClient`:
  `create_action_client_with_callbacks_for_name::<Fibonacci>`, one-shot `send_goal_for_name`
  gated by a `sent` flag in `tick`, result/feedback in `on_callback`). Both ported from the
  declarative `examples/qemu-arm-baremetal/rust/action_server_rtic_pkg` + `examples/zephyr/
  rust/action-client` references. Wired into the showcase (workspace members, `system.toml`
  components, `showcase.launch.xml` nodes, `native_showcase_entry` deps). Result is
  republished on `/fib_result` (Int32) so it is observable on the wire — the workspace shape
  inits no log sink yet (A5). `cargo build -p native_showcase_entry` links all six pkgs clean
  (13.6s); both new pkgs clippy-clean. Known limitation (shared with the rtic reference): the
  app-node shape does not surface the goal payload at tick time, so the server emits a
  fixed-`ORDER = 10` sequence matching the client's goal rather than the per-goal requested
  order. **Runtime e2e DONE (2026-06-24, Track D) — cross-process.** The combined showcase
  boots the pair in one entry, but in-process node-to-node delivery does not happen (issue
  0096), so the round-trip runs as two processes: new `native_action_server_entry` +
  `native_action_client_entry` (one-node `action_server.launch.xml` /
  `action_client.launch.xml`), fixtures `workspace-rust-native-action-{server,client}`, and
  `tests/action_roundtrip_xprocess_e2e.rs` asserts a `/fib_result` subscriber sees the
  result's last element `55` (PASS). Remaining: project to C/C++ + embedded entries (Track C).
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
- **B3 — `ws-bridge`. SCOPED — heavier bake-path wave (2026-06-25).** A
  cross-RMW gateway (zenoh ↔ xrce/cyclonedds) declared via `[[bridge]]` in
  system.toml. Declarative `[[bridge]]` **is implemented** — but in the CLI
  **bake** path (`nros-cli-core/src/orchestration/`: the `bridge` Vec in
  `cargo_metadata_schema.rs`, `build_executor_bridge` / `register_bridges` /
  `validate_bridges` + bridge-origin encode/parse in `generate.rs`), **not** the
  plain-cargo `nros::main!` macro that B1/B2/B4/B5/B6 all use (the macro emits no
  bridge registration). So B3 cannot follow the `native_entry` + `nros::main!`
  pattern of its Track-B siblings; it needs the bake shape (`nros bake` /
  `nros codegen entry`) AND two RMW backends linked in one process — including the
  **vendored C++ CycloneDDS** (`nros-rmw-cyclonedds-sys`, whose `build.rs`
  compiles CycloneDDS 0.10.5) — plus the #53 multi-RMW egress-domain handling
  (egress extra-session defaults to domain 0; must set `.domain_id()`). The
  working imperative reference is `examples/bridges/tt-zenoh-to-{xrce,cyclonedds}`
  (manual TT-scheduled ingress/egress, type-descriptor staging). Deferred as a
  dedicated wave because of the bake-path + two-backend (C++) build infra, which
  is out of scope for the macro-shaped batch.
- **B4 — `ws-qos-rust`. RUST DONE (2026-06-25).** New `examples/workspaces/
  ws-qos-rust`: `reliable_talker_pkg` publishes `std_msgs/Int32` on `/qos_chatter`
  via `create_publisher_for_topic_with_qos` with an explicit profile
  (`reliable() + transient_local() + depth(10)`); `qos_listener_pkg` subscribes
  via `create_subscription_for_topic_with_qos` with the SAME profile (re-uses
  `reliable_talker_pkg::qos_profile()`) and republishes the receive count on
  `/qos_ok`. QoS is a per-entity code-level contract via the declarative
  `*_with_qos` API — no system.toml QoS section (the planner's baked
  `qos_overrides` `apply_overrides` table is a separate, more advanced path).
  `cargo build -p native_entry` links both pkgs clean; both clippy-clean.
  Remaining: status events (deadline-missed / liveliness) are not yet on the
  declarative `CallbackCtx`; runtime e2e (Track D); project to C/C++/mixed.
- **B5 — `ws-launch-rust`. RUST DONE (2026-06-25).** New `examples/workspaces/
  ws-launch-rust`: the topology lives in launch XML. `system.launch.xml`
  exercises the launch v1 surface — `<arg>` defaults, `$(var …)` substitution,
  `<group ns=…>`, a `<node>` with child `<param>` + `<remap>`, and `<include>` of
  `sensors.launch.xml` with `<arg value=>` pass-through. `nros::main!` resolves
  the whole tree at build time and emits one register per resolved node; `cargo
  build -p native_entry` links both (talker_pkg + listener_pkg, relative topic
  names so ns/remap apply); both clippy-clean. `nros plan` writes record.json +
  nros-plan.json (the launch record resolves `robot_ns=alpha` + the remap +
  param). Scope: `if=`/`unless=` conditionals + `$(env …)` intentionally unused
  (v1 has no conditionals; unset `$(env)` is a compile-time error). Lowering
  `<group ns=…>` into the per-node namespace of the orchestration IR is not yet
  wired (IR normalizes namespace to `/`) — a planner-maturity item; the workspace
  demonstrates advanced-launch parse+resolve+build. Remaining: runtime e2e
  (Track D); project to C/C++/mixed.
- **B6 — `ws-custom-msg-rust`. RUST DONE (2026-06-25).** New `examples/workspaces/
  ws-custom-msg-rust`: an in-workspace interface package `src/custom_msgs/`
  (`package.xml` + CMakeLists + `msg/Reading.msg`: float64 temperature/humidity +
  int32 sequence; no Cargo.toml). `nros ws sync` walks its `package.xml`, runs the
  nano-ros codegen pipeline, and emits `generated/custom_msgs`. `reading_talker_pkg`
  publishes `custom_msgs/Reading` on `/reading` via the typed
  `create_publisher_for_topic::<Reading>` path; `reading_listener_pkg` decodes it
  and echoes the sequence on `/reading_seq`. The generated `Reading` implements
  `RosMessage`, so it flows through the ordinary typed path — only the schema is
  yours. `cargo build -p native_entry` links both pkgs + the generated crate
  clean; both clippy-clean. Mirrors `examples/templates/local-msg-package`'s
  in-workspace interface shape. Remaining: runtime e2e (Track D); project to
  C/C++/mixed (the `.msg` is already colcon-buildable).

### Track C — Platform parity (C / C++ / mixed)
Give the starter C/C++/mixed workspaces the embedded entries Rust already has
(freertos / nuttx / zephyr / esp32 / threadx) + a `multihost.launch.xml` + robot1/2
deploy targets. Reuses the Rust workspace's per-platform Entry pattern.

- **C1 — multihost (the cheap, native-verifiable half). C / C++ / mixed DONE (2026-06-25).**
  **Finding:** the C/C++ CMake path had **no host-partition support** — `nano_ros_entry`
  (and `NROS_MAIN_C`) took no host arg, so a C entry baked the *whole* launch, never a
  per-host subset. But `nros codegen entry --host <id>` already exists and is
  **lang-agnostic** (filters `<node machine="…">`, emits C/C++/Rust identically), so the
  gap was only a CMake passthrough. Added a `HOST` keyword to `nano_ros_entry` →
  `_nros_entry_invoke_codegen` → `nros codegen entry --host` (`cmake/NanoRosEntry.cmake`).
  Then authored the C workspace's `multihost.launch.xml` (talker `machine="robot1"`,
  listener `machine="robot2"`) + `native_entry_robot1` (`HOST robot1`) +
  `native_entry_robot2` (`HOST robot2`) + `[deploy.robot1/2]`. Verified the partition
  (robot1 TU registers talker only, robot2 listener only) and runtime: fixtures
  `workspace-c-native-robot{1,2}` + `tests/c_multihost_e2e.rs` boot both as two processes
  and assert robot2 (listener-only host entry) receives robot1's `/chatter` (PASS). Also
  line-buffered the C listener's stdout (`setvbuf _IOLBF`) so piped output is observable
  live. **C++ + mixed followed the same passthrough:** `cpp_multihost_e2e.rs` (talker
  robot1 → listener robot2) and `mixed_multihost_e2e.rs` (C talker + Rust heartbeat on
  robot1, C++ listener on robot2 — a genuinely mixed-language two-host topology; partition
  verified: robot1 TU = c_talker + rust_heartbeat, robot2 = cpp_listener) both PASS, with
  fixtures `workspace-{cpp,mixed}-native-robot{1,2}`. So all three CMake workspaces now
  reach Rust's multihost parity from the single `HOST` passthrough.
  Remaining: embedded entries (C2 — the harder, uncharted CMake half).

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

- **A2 (parameters): ~~BLOCKED~~ RESOLVED (phase-264 W4).** The missing runtime
  parameter-value read on `CallbackCtx`/`TickCtx` landed as `ctx.parameter::<T>(name)`
  (live, gated on `param-services`); the Rust param demo (`ws-params-rust`) + e2e are
  done — see A2 above.
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
