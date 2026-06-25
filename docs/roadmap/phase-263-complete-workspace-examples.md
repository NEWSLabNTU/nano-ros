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
- **B3 — `ws-bridge-rust`. PARTIAL (2026-06-25) — engine landed, bake-flow cascade
  remains.** A cross-RMW gateway **zenoh ↔ cyclonedds** declared via `[[bridge]]`
  in system.toml. **Landed this session:** (Step 1) the planner transform
  (`system.{bridges,domains}` → `build.transports` + `plan.bridges`, issue #99
  step 0) — `nros plan` on `ws-bridge-rust` emits a correct bridge plan; (cyclone
  codegen) native Rust entries now link + register `nros-rmw-cyclonedds-sys`
  (board-gated, no C/cpp regression, test
  `cyclone_backend_dep_gated_on_native_board`); (workspace skeleton) `talker_pkg`
  + bridge `system.toml` authored (talker builds). **Remaining (issue #99
  cascade) — B3 is phase-sized:** (1) `cmd/codegen_system.rs::render_plan_json` is
  a SECOND, thin plan emitter that doesn't carry the transform; (2) topic
  resolution needs component metadata (a launch-only plan leaves `interfaces=[]`
  → `topics=[]`); (3) no existing lane builds a pure-cargo BAKED Rust entry (Rust
  workspaces build the `nros::main!` macro entry, which emits no bridge relay); (4)
  per-type cyclone descriptor staging for non-baked types (`std_msgs/Int32` is
  baked, so the demo type needs none). Each is its own gap; see issue #99. A
  3-explorer deep-dive (bake codegen + imperative ref/runtime + build infra)
  reframed B3: the build is **not** the blocker (plain `cargo build` links both
  backends; vendored C++ CycloneDDS is ~90s first build, sccache'd after; no
  runtime agent vs xrce's Micro-XRCE Agent), and the declarative path is
  ~90% wired (schema `SystemBridgeEntry`, `PlanBridge`/`PlanTransport` IR,
  `validate_bridges` + `render_register_bridges_fn` relay codegen, `nros-bridge`
  echo codec, `open_multi` runtime — all code-complete + unit-tested per
  RFC-0009). **The one real gap is in the planner** (issue **0099**): it emits
  only `bridged_rmws` (the RMW name union) and leaves `plan.build.transports`
  empty + `plan.bridges = []`, so `is_bridge()` is false and the relay codegen
  (gated on `!plan.bridges.is_empty()`) never fires. **Decisions (locked):** close
  the planner gap (make the declarative bridge real, not a build-only stub) +
  zenoh↔cyclonedds (no-agent runtime, clean stock-ROS2 interop).
  - **Step 1 (engine, issue 0099):** planner transform `system.{bridges,domains}`
    → consistent `plan.build.transports` (one `PlanTransport{rmw,domain,locator}`
    per endpoint, so `is_bridge()` + `SESSION_SPECS`/`open_multi` light up) +
    `plan.bridges` (one `PlanBridge{name,connect:[from,to],topics}`, endpoints
    byte-matching the transports for `bridge_endpoint_session_idx`; `topics` =
    all declared interface topics, RFC-0009 resolve-from-interfaces) + planner
    unit tests. Honor #53 (egress domain threaded) + #67 (multi-RMW uses raw +
    `register_type_descriptor`, no `nros/rmw-cyclonedds` marker).
  - **Step 2 (workspace):** `examples/workspaces/ws-bridge-rust` — a nano-ros
    talker publishes `/chatter` on the zenoh session; `[[bridge]] gw
    from="zenoh:…" to="cyclonedds:…"` forwards it; a stock `rmw_cyclonedds_cpp`
    peer (`ros2 topic echo /chatter`) receives → proves cross-RMW forward + ROS2
    interop. **Bake-shaped Entry** (`nros codegen system` → `nros generate-rust`),
    NOT `nros::main!` — the one structural difference from the macro-shaped
    Track-B siblings. Deps: `nros-rmw-zenoh` + `nros-rmw-cyclonedds-sys` +
    `nros/bridge` + `platform-posix`.
  - **Step 3 (runtime e2e, GATED):** zenohd + the bake'd entry + a live cyclone /
    ROS2 subscriber; assert receipt. Gated on a live DDS peer — same gate as the
    existing `bridge-zenoh-to-cyclonedds-fwd` fixture.
  Working imperative reference: `examples/bridges/tt-zenoh-to-{xrce,cyclonedds}`.
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
- **C2 — embedded entries (freertos / nuttx / zephyr / threadx; esp32 skipped). DESIGN
  LOCKED (2026-06-25). DONE + runtime-verified (2026-06-25): C2-pre, C2a (threadx-linux C),
  C2b-freertos (QEMU C), C2c-cpp (threadx-linux + freertos C++), C2c-mixed (threadx-linux
  C+C++/Rust), C2d-zephyr C (native_sim, west lane) — 7 GREEN e2e tests. C2b-nuttx BUILD blocker
  RESOLVED (2026-06-26, per-`NROS_PKG_NAME` wall — per-source cc-rs; multi-node ELF builds with
  seams resolved), runtime QEMU-console pending. BLOCKED: C2c-mixed-freertos (std Rust node vs
  no_std target). See below.** Two-agent framework exploration found the
  embedded build is **far smaller than it looked** — not a single-platform-model rewrite,
  just two gaps + wiring:
  - **The embedded carrier mechanism already exists** in `nano_ros_node_register`
    (`cmake/NanoRosNodeRegister.cmake` ThreadX 471–552 / FreeRTOS 572–640 / NuttX 374–458):
    synthesize a carrier `add_executable`, link `NanoRos::NanoRosCpp`, then call
    `nros_platform_link_app(target)` — which pulls the board's startup source, app_define,
    linker script, kernel+netstack umbrella, and RMW stub. Platform/board overlays
    (`cmake/{platform,board}/*.cmake`) + toolchain files (`cmake/toolchain/*`) all exist.
  - **Gap 1 (the core change): `nano_ros_entry` has NO embedded link path.** Its
    LAUNCH-codegen executable only calls `nros_platform_link_app` when
    `NANO_ROS_PLATFORM == posix` (`NanoRosEntry.cmake:201–206`); embedded falls through
    unlinked. Fix: call `nros_platform_link_app(${target})` for embedded platforms when a
    board is loaded, mirroring `node_register`. ~10 lines.
  - **Gap 2: the workspace root hardcodes `PLATFORM posix`.** Fix: accept
    `-DNANO_ROS_PLATFORM` / `-DNANO_ROS_BOARD` cache overrides (default posix). The model
    stays **one platform per configure** (a build dir per board) — exactly how the
    standalone examples and the Rust per-fixture-row approach already work. **No
    `nano_ros_use_board` (it's a phantom — referenced only in comments, never defined),
    no per-entry multi-platform.**
  - The rest is wiring (no R&D): embedded fixture rows (`platform` + `build_subdir` +
    `cmake_defs` with toolchain + `NANO_ROS_PLATFORM`/`NANO_ROS_BOARD`; the cmake lane
    already forwards `cmake_defs`), codegen `--board zephyr` (the unified embedded adapter,
    RFC-0032 §8a), and `just/<platform>.just` hookup.
  - **C2a SPIKE (2026-06-25) — found a THIRD gap; the build works but the runtime needs
    codegen.** Implemented gaps 1+2 and threadx-linux C end-to-end: the entry **built and
    linked** (the embedded link pass pulled in ThreadX startup + board TU + kernel/netstack;
    the `_NRC_DEPLOY` node gate kept the workspace nodes component-only). Gap-2 turned out to
    be a real bug vs the **documented contract** (`NanoRosNodeRegister.cmake:142–148` says the
    embedded carrier branches gate on `<rtos> IN_LIST _NRC_DEPLOY` — but the threadx + freertos
    branches were missing that gate; only nuttx had it). BUT the entry **segfaults at runtime**:
    - **Gap 3 (the true blocker): `nros codegen entry` for C/C++ is native-only.** Its
      `--help` says "Defaults to `native` — the only Entry-pkg target the C/C++ surface
      supports today (Phase 212.L.2)". Even with `--board zephyr`, the generated TU emits
      `int main()` → `nros_board_native_run_components` (the **native** runner), which clashes
      with ThreadX's `startup.c` main + `tx_kernel_enter` / `app_main` contract → SIGSEGV. The
      single-node `node_register` carrier emits the board-correct shape via a per-platform
      TEMPLATE (`cmake/templates/threadx_entry_main_c_typed.cpp.in`), but the multi-node LAUNCH
      codegen has **no embedded board runners** — it always emits the native runner.
    - **Conclusion: C2 is gated on a CODEGEN feature** — teach the C/C++ `nros codegen entry`
      emitter to produce per-board embedded runners (the app_main / board-run shape) for the
      LAUNCH (multi-node) case, mirroring the node_register templates. This is Rust-CLI work,
      not cmake. The two cmake fixes (entry embedded link + the node_register DEPLOY gate) are
      correct and ready to reapply once the codegen lands. Filed as **issue 0097**.
    - **Revised plan + work items (mapped 2026-06-25 — emitter is inline string builders, no
      template files; the N-node setup body, board run-classes, app_main macro, and the
      single-node templates all already exist, so the fix is the OUTER wrapper only):**

      **C2-pre — codegen + cmake (issue 0097). W1–W4 DONE + runtime-verified (2026-06-25).**
      The ws-c ThreadX entry BUILDS + LINKS + BOOTS the kernel, dispatches to the generated
      `app_main` → `ThreadxBoard::run_components`, brings the nros runtime online over a baked
      loopback locator, AND **delivers** `/chatter` cross-process to a native listener — the C2a
      runtime e2e (`tests/c_threadx_entry_e2e.rs`) is GREEN on a single host with **no veth
      bridge / no root**. Both prior "follow-ups" turned out to be two MORE pieces of carrier
      wiring the LAUNCH path was missing (the LAUNCH path builds the exe itself, so the
      `nano_ros_node_register` carrier's wiring never ran for it), now fixed in `nano_ros_entry`:
      (1) **locator bake** — define `NROS_ENTRY_LOCATOR` on the embedded entry target
      (`-DNROS_ENTRY_LOCATOR` cache > `LOCATOR` arg > per-board default; threadx-linux → loopback,
      QEMU → slirp `10.0.2.2`); the locator-less `run_components` overload reads it, so `nros::init`
      reaches a router (the nsos-netx POSIX-`connect()` shim dials loopback with no bridge). (2)
      **sizes-header ordering** — the same `add_dependencies` + file-level `OBJECT_DEPENDS` on the
      Corrosion mirror header the carrier uses, so a fresh embedded build dir compiles clean (no
      manual copy). Fixture `workspace-c-threadx-linux` (own `build-subdir`, baked
      `tcp/127.0.0.1:17553`) + the `just threadx-linux build-examples` C lane wire it in.
      - **W1 — C++ emitter embedded shape. DONE.** `emit_cpp.rs` (~383) already board-aware
        (`board_cpp_path()` → `ThreadxBoard`/…) but emits `int main(){ <Board>::run_components(
        &setup); }` for ALL boards → **double-mains** with the RTOS `startup.c`. For non-native:
        emit `#include <nros/app_main.h>` + `extern "C" int nros_app_main(...){ return
        <Board>::run_components(&__nros_entry_setup); }` + `NROS_APP_MAIN_REGISTER_VOID();` (the
        locator-less overload reads the `NROS_ENTRY_LOCATOR` macro — no codegen-side locator).
        Native keeps `int main`.
      - **W2 — C emitter embedded shape.** `emit_c.rs` (~128) is native-only (hardcodes
        `nros_board_native_run_components`, ignores `plan.board`). The board runners are C++ only,
        so for non-native emit a **`.cpp`** TU with the W1 shape, invoking each node via its
        existing `extern "C" __nros_c_component_<pkg>_create/configure` seam (exactly what
        `cmake/templates/threadx_entry_main_c_typed.cpp.in` does). Native keeps pure-`.c`.
      - **W3 — cmake reapply + board key.** `nano_ros_entry`: pass the **real** board key to
        `--board` (not the `zephyr` auto-derive — boards differ in spin/init); add the embedded
        `nros_platform_link_app` link pass; link `NanoRosCpp` for an embedded C entry (its TU is
        C++). `node_register`: add the documented `AND _NRC_DEPLOY` gate to the threadx + freertos
        carrier branches (real bug — only nuttx had it). Workspace root: accept
        `-DNANO_ROS_PLATFORM`/`-DNANO_ROS_BOARD` overrides (one board per configure).
      - **W4 — verify on threadx-linux C. DONE.** `native_threadx_entry` builds + links + boots +
        **delivers**: `tests/c_threadx_entry_e2e.rs` asserts cross-process `/chatter` from the
        embedded entry's talker to a separate native listener (≥3 received) — the first
        runtime-verified C embedded workspace entry. Backed by the locator-bake + header-ordering
        wiring in `nano_ros_entry` (the two ex-follow-ups) + the `workspace-c-threadx-linux`
        fixture row.

      **C2a — threadx-linux** C (host sim, run-verifiable) **DONE 2026-06-25** —
      `tests/c_threadx_entry_e2e.rs` GREEN; C++/mixed on threadx-linux follow the same path
      (reuse the locator-bake + header-ordering wiring). **C2b — freertos** C (QEMU build + run)
      **DONE 2026-06-25** — `tests/c_freertos_entry_e2e.rs` GREEN: the first QEMU-cross embedded
      workspace entry boots FreeRTOS on MPS2-AN385 + lwIP and delivers `/chatter` cross-process to
      a native listener. Three new pieces beyond the C2a wiring: (i) `board_cpp_path` gained a
      FreeRTOS arm (it fell through to `NativeBoard` → the codegen emitted the native runner, not
      `FreertosBoard::run_components`); (ii) the FreeRTOS `NROS_APP_CONFIG` TU (startup.c's
      network/scheduling) is generated in `nano_ros_entry` — ThreadX's `nros_platform_link_app`
      bakes its own, FreeRTOS's does not (the carrier did it, the LAUNCH path now mirrors that);
      (iii) the ws-c root maps the board → arm-none-eabi toolchain BEFORE `project()` (cross
      boards need `CMAKE_TOOLCHAIN_FILE` at the first compiler probe). The firmware's static
      `192.0.3.x` lwIP config drives a board-matching QEMU slirp net (`host=192.0.3.1`, a new
      `QemuProcess::start_mps2_an385_freertos_slirp`) + a `tcp/192.0.3.1:<port>` baked locator — no
      TAP/bridge/root. **C2b — nuttx: BUILD BLOCKER RESOLVED (2026-06-26, the per-`NROS_PKG_NAME`
      wall) — runtime QEMU-console pending.** The earlier "block": the NuttX image links the entry
      INTO the kernel via the cargo `nros-nuttx-ffi` build, which compiled `APP_EXTRA_SOURCES` in
      ONE `cc-rs` invocation with a SINGLE `APP_COMPILE_DEFS`; `NROS_C_COMPONENT` names each seam
      `__nros_c_component_<NROS_PKG_NAME>_*` via `-DNROS_PKG_NAME=<pkg>`, so a multi-node entry
      needs Talker.c with `=c_talker_pkg` AND Listener.c with `=c_listener_pkg` — two defines in one
      compile. **Solution (Option A — chosen over cross-building the libs): per-source `cc::Build`.**
      cc-rs supports many `cc::Build` instances with different defines (the code already made two,
      one C + one C++); now each mapped component source compiles in its OWN `cc::Build` with its
      pkg's `NROS_PKG_NAME` → its own archive. Three layers: (0) annotate the component lib with an
      `NROS_COMPONENT_PKG_SYM` property; (1) `nuttx_ffi_build.rs` parses `APP_EXTRA_SOURCE_PKGS`
      (`<abs-src>=<pkg>`) and compiles each solo — CRUCIALLY ordered AFTER the shared `app_cpp`
      entry archive so the entry's seam references pull the component objects (static-link order);
      (2) the board cmake extracts each component lib's SOURCES + pkg + includes from
      `LINK_INTERFACES` and hands them to the cc-rs build instead of linking the wrong-arch host
      `.a`. Plus the en-route fixes (NUTTX_DIR/APPS_DIR CACHE-promotion for the sibling subdir
      scope). **Verified: the multi-node ws-c NuttX entry now BUILDS a bootable `armv7a-nuttx-eabihf`
      ELF with BOTH `__nros_c_component_c_{talker,listener}_pkg_*` seams resolved** (`nm` confirms;
      the original blocker symptom is gone). This is the NuttX analog of how Zephyr (C2d) compiles
      each component as a separate static lib. Back-compat: empty `APP_EXTRA_SOURCE_PKGS` → the
      original single-archive behavior (the single-node carrier compiles its node as a direct
      source, no component lib in `LINK_LIBRARIES`, so the new extraction never fires). RUNTIME
      pending: the QEMU image boots (virtio-net activity) but emits no console output even at 55s —
      a nuttx-qemu console/boot matter separate from the per-pkg wall (no working standalone
      baseline locally to bisect); so no fixture/test yet (scaffold lands as WIP). **C2c — C++ DONE (2026-06-25)** — `tests/cpp_threadx_entry_e2e.rs` +
      `tests/cpp_freertos_entry_e2e.rs` GREEN: the C++ workspace's threadx-linux + FreeRTOS-QEMU
      entries deliver `/chatter` cross-process, reusing the C2a/C2b wiring VERBATIM through the C++
      emitter (only the cpp workspace root needed the same toolchain-map + conditional-subdir
      restructure the C root got). **mixed (C+C++/Rust) on threadx-linux DONE (2026-06-25)** —
      `tests/mixed_threadx_entry_e2e.rs` GREEN: the Rust heartbeat node links via the
      `nros_ws_runtime` umbrella, which on threadx-linux targets the host x86_64 triple (ThreadX
      sim = pthreads), so the Rust node compiles host-side like the native mixed entry — bootable
      C+C++/Rust image, cross-process `/chatter` delivery. **mixed on FreeRTOS: BLOCKED
      (2026-06-25) — std Rust node vs no_std target.** `rust_heartbeat_pkg` depends on `nros` with
      the `std` feature (no `#![no_std]`); FreeRTOS is `thumbv7m-none-eabi` (no_std), where a std
      crate cannot compile. threadx-linux worked only because its cargo triple is the host
      `x86_64`. Unblock needs the workspace's Rust node made `no_std` (nros `alloc`-only) AND the
      `nros_ws_runtime` umbrella selecting no_std features + the cross triple per active platform
      (it currently pins one feature set workspace-wide) — a standalone subproject; the same wall
      hits FreeRTOS/NuttX *Rust* workspaces generally. **C2d — zephyr C: build + native_sim
      runtime delivery PROVEN (2026-06-26), approach A.** The Zephyr build model is
      fundamentally different (west lane, `find_package(Zephyr)` → monolithic `app` target, not
      add_executable + nros_platform_link_app), so `nano_ros_entry` gained a zephyr branch — the
      C/C++ analog of zephyr-lang-rust's `rust_cargo_application()`: it puts the generated entry
      TU (an `int main(void)` driving `ZephyrBoard::run_components` — a THIRD codegen shape, since
      Zephyr's kernel calls `main` directly, NOT the startup.c `nros_app_main` shape) into `app`
      (whole-archived → strong `main`), and links the node component libs in via a placeholder
      static lib the sidecar targets. The locator threads in via `CONFIG_NROS_ZENOH_LOCATOR`
      Kconfig (no bake). The entry CMakeLists is itself a Zephyr app (`find_package(Zephyr)` +
      `nano_ros_entry(BOARD zephyr …)`); it includes NanoRosEntry (NOT NanoRosWorkspace) to avoid
      the corrosion nros build clashing with the Zephyr module. **Crucially the per-`NROS_PKG_NAME`
      wall that blocked NuttX does NOT apply** — Zephyr compiles each node component as a separate
      cmake static lib (its own define), then links them. Verified on native_sim/native/64: the
      C workspace entry (talker + listener) boots `zephyr.exe` and delivers `/chatter`
      cross-process to a native listener over NSOS host sockets (5 sent / 5 received in 20s, no
      bridge/root). The automated test landed: `tests/zephyr_entry_e2e.rs` (GREEN, 12.9s — the
      first Zephyr *workspace-entry* runtime test; the Rust one was build-coverage only) +
      `build_zephyr_workspace_c_entry()` resolver + a C workspace-entry record in
      `zephyr-fixture-leaves.sh` under `--include-workspace-entry` (built by `just zephyr
      build-fixtures`, distinct port 17831). **C2d-zephyr C: DONE.** C++/mixed zephyr reuse the
      same wiring (C++ direct; mixed inherits the C2c-mixed-freertos no_std-Rust caveat on a real
      board, but native_sim's host triple sidesteps it like threadx-linux did). Design + gaps
      captured here + in 0097.
      Toolchains present locally: freertos (arm-none-eabi + qemu), nuttx (arm-none-eabi/riscv),
      threadx-linux (host), zephyr (west); esp32 (idf.py) absent → skipped.

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
