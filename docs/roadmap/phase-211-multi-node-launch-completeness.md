# Phase 211 — Multi-node + launch-file completeness against real ROS production

**Goal.** Close the gap between nano-ros's orchestration surface (`nros plan` /
`nros deploy` / `nros run-target` + Autoware-level launch parsing via the
vendored `play_launch_parser`) and what a real ROS 2 production system
exercises (composable containers, intra-process zero-copy, `ros2` CLI
interop, lifecycle transitions, conditional groups, dynamic loads,
multi-host, mixed-RMW). The parser layer is strong (260 tests, Autoware
100 %); the **planner consumption** + the **in-tree fixture coverage** +
the **runtime-interop story** are where the gaps sit.

**Status.** Proposed (2026-05-31). Survey done — see the gap list below;
priorities are concrete (each item maps to a missing fixture or a missing
planner field).

**Priority.** P2 — orchestration is the user-facing entry point to "this
nano-ros workspace behaves like a ROS 2 workspace"; the gaps below are
what would surface the first time a real Autoware-shape user `nros
deploy`s their launch file.

**Depends on.** Phase 172 (orchestration, archived), Phase 172.A
(`[lifecycle]` block), Phase 126.B / 126.C (component metadata API +
launch-manifest planner), Phase 195/197 (`nros setup` + `just`→`nros`
migration), the `nros-cli` repo's `play_launch_parser` +
`ros-launch-manifest` vendored submodules.

## Overview

Today's orchestration pipeline:

```
*.launch.{xml,py,yaml}
       │
       │  play_launch_parser (Autoware-level)
       ▼
   record.json   ─── pkg/exec/name/namespace/parameters/remappings/nodes/includes/launch_arguments
       │
       │  nros-cli-core::orchestration::planner
       ▼
   nros-plan.json   ─── + nros.toml overlay (rmw, domain_id, lifecycle{autostart}, params)
       │
       │  nros-cli-core::orchestration::generate
       ▼
   generated entry crate   ─── compile / west build / vendor-module emit
       │
       ▼
   `nros deploy <name>` → runs the binary / sim / hardware target
```

The pipeline carries the **happy path** (one or more nodes per package,
parameters + remappings, lifecycle autostart, deploy-target switch). Real
ROS 2 production stresses it on five orthogonal axes the planner doesn't
fully cover yet:

1. **Composable / intra-process** — `<composable_node>` / `<node_container>`,
   the same container hosting multiple nodes for zero-copy.
2. **Conditionals + scoping** — `<group>`, `<if>` / `<unless>`,
   `<set_remap>`, `<set_env>`, `<executable>` siblings.
3. **`ros2` CLI host interop** — `ros2 param`, `ros2 lifecycle`,
   `ros2 topic`, `ros2 service` reaching a deployed nano-ros node from a
   plain ROS install.
4. **Multi-host / mixed-RMW** — `machine="…"` attr; nano-ros (XRCE / zenoh)
   ↔ stock (cyclonedds / fastdds) discovery.
5. **Test infra** — `launch_testing` equivalent (assert "topic ≥ N Hz for
   ≥ T s", "node enters Active in ≤ T s").

Each work item below is a focused fixture + planner enhancement + e2e
assertion.

## Architecture

```
                                 ┌──────────────────────┐
launch-file features ────────────┤  play_launch_parser  │  (Autoware-level coverage today)
                                 └──────────┬───────────┘
                                            │ record.json
                                            ▼
                                 ┌──────────────────────┐
                                 │  nros-cli planner    │  ← 211.B/D/E/F/G/I land here
                                 └──────────┬───────────┘
                                            │ nros-plan.json
                                            ▼
                                 ┌──────────────────────┐
                                 │  generated entry lib │
                                 └──────────┬───────────┘
                                            │ rmw vtable (zenoh / xrce / cyclonedds)
                                            ▼
                                ┌────────────────────────┐
                                │  nano-ros executor +   │
                                │  param/lifecycle svcs  │  ← 211.C/J/H land here
                                └────────────────────────┘
                                            │
                                            ▼
                                 ┌──────────────────────┐
                                 │  `ros2` CLI (host)   │
                                 └──────────────────────┘
```

Most items split into three layers: **fixture** (a workspace in
`nros-cli/testing_workspaces/`), **planner change** (in nros-cli, requires
a release + pin bump), **runtime side** (in nano-ros).

## Work Items

### 211.A — In-tree orchestration e2e test (foundation)

The orchestration tests live in `nros-cli`'s `testing_workspaces/`; nano-ros's own
test suite has nothing under `nros-tests/tests/{orchestrat,launch,plan,deploy}*`.
A user editing nano-ros runtime code wouldn't catch an orchestration
regression until the next nros-cli release picks it up.

- [x] **Vendor a stable fixture** — `packages/testing/nros-tests/fixtures/orchestration_e2e/`
      mirroring the nros-cli `testing_workspaces/orchestration_e2e` shape
      (root `nros.toml`, `src/demo_pkg/{launch,component_nros.toml,package.xml,metadata/talker.json}`).
      One single-talker case. `record.json` is committed (pre-collected
      `play_launch_parser` output) so the test runs without the parser binary.
- [x] **`orchestration_plan_emits_expected_entities`** in `packages/testing/nros-tests/tests/orchestration_e2e.rs`:
      drives `nros plan demo_pkg src/demo_pkg/launch/system.launch.xml --record record.json`
      and asserts `nros-plan.json` carries the `demo_pkg::talker` component,
      a `demo_pkg.talker.*` instance, the `cb_timer` callback binding, and the
      `build.rmw = zenoh` / `build.target = x86_64-unknown-linux-gnu` pin.
- [x] **Skip cleanly** when the `nros` CLI isn't on PATH (`require_nros_cli`
      helper added to `nros-tests::lib`, mirrors `require_xrce_agent`).
- [ ] **`nros deploy native` second-stage** — start the resulting binary →
      assert "Published:" in stdout. Deferred: requires the demo_pkg crate
      to actually compile + an FFI exported `nros_component_talker` symbol;
      the plan-only stage is enough to gate planner regressions, the build
      stage rolls into 211.B's executor-driven container e2e.
- **Files:** `packages/testing/nros-tests/fixtures/orchestration_e2e/*`,
  `packages/testing/nros-tests/tests/orchestration_e2e.rs`,
  `packages/testing/nros-tests/src/lib.rs` (`nros_cli_bin_path`, `require_nros_cli`),
  `packages/testing/nros-tests/Cargo.toml` (`serde_json` dep + test target).

### 211.B — Composable-node planner handling (biggest production gap)

`play_launch_parser` reads `<node_container>` / `<composable_node>` /
`<load_composable_node>`; the nros-cli planner currently emits each as a
separate plan entity. Production ROS (Nav2, Autoware, MoveIt) **relies**
on a single container hosting many nodes for intra-process zero-copy.

- [→] **Planner change (nros-cli):** group composable-children under the
      parent container in `nros-plan.json` — new `entities[*].container_id`
      field + `entities[*].kind = "container"|"composable_node"|"node"`.
      Preserve per-child parameters/remappings. **Lives in `nros-cli`
      repo** (`packages/codegen` submodule retired); this nano-ros tree
      holds only the regression fixture + plan-shape gate. The in-tree
      test is structured so the post-fix shape is a single
      `assert_eq!(child["container_id"], container["id"])` flip.
- [ ] **Runtime: one-process-many-nodes** — `Executor::open` +
      `executor.create_node(name)` already supports N nodes per process
      (Phase 172 W.5). Add an executor unit + per-RMW build fixture
      that exercises 2 composable libraries linked into one container
      binary and asserts both publish from the same PID. Deferred until
      the planner-side `kind`/`container_id` lands (so the build script
      knows which entities to whole-archive).
- [x] **Fixture:** `packages/testing/nros-tests/fixtures/orchestration_composable/`
      mirrors the multi-component layout (`nros/components/{talker,listener}.toml`)
      with a `<node_container>` + 2 `<composable_node>` children sharing a
      remapped `/chatter_a` topic. Pre-baked `record.json` so the test runs
      without `play_launch_parser` on PATH.
- [x] **`composable_container_plan_shape`** (in
      `packages/testing/nros-tests/tests/orchestration_composable.rs`):
      runs `nros plan` against the fixture and asserts (a) two flat
      composable instances surface today, (b) neither carries a
      `container_id` field (gates current planner gap), (c) per-composable
      `<param>` override propagates (`rate_hz = 20`), (d) `<remap>` resolves
      both endpoints to `/chatter_a`, (e) `components` lists both
      `Talker` + `Listener`. Carries a TODO block for the post-fix
      assertions when 211.B's planner change lands upstream.
- **Files:** *(planner, lives in nros-cli)*
  `packages/nros-cli-core/src/orchestration/planner.rs` (composable handling),
  `packages/nros-cli-core/src/orchestration/generate.rs` (multi-node entry-lib);
  *(this tree)* `packages/testing/nros-tests/fixtures/orchestration_composable/*`,
  `packages/testing/nros-tests/tests/orchestration_composable.rs`.

### 211.C — `ros2` CLI host-interop fixtures

`nros-params` + `nros-node`/`lifecycle-services` exist but **no fixture
proves host `ros2` CLI round-trips against a deployed nano-ros node**.
This is what a ROS-2-fluent user expects to "just work".

**Status (2026-05-31): largely satisfied by existing tests; the only
real gap (host CLI observes the publisher's rate) closed by a new
`test_ros2_topic_rate_via_echo_interop`.** The phase-doc audit found
the suite already covered all three sub-bullets — the doc was written
without surveying existing coverage.

- [x] **`ros2 param` round-trip** — `packages/testing/nros-tests/tests/params.rs`
      (`test_ros2_param_list` + `test_ros2_param_get` + `test_ros2_param_set`).
      Deploys the param-services talker, drives `ros2 param list/get/set`,
      asserts the round-trip. C++ variant in `cpp_parameters.rs`.
- [x] **`ros2 lifecycle` full cycle** —
      `packages/testing/nros-tests/tests/ros2_lifecycle_interop.rs::ros2_lifecycle_full_cycle`.
      Drives `ros2 lifecycle list/set` through configure → activate against
      `lifecycle_node_binary`, asserts the node only publishes after
      `activate`.
- [x] **`ros2 topic list / topic echo / topic rate` interop** — list +
      echo were already in `tests/rmw_interop.rs`
      (`test_discovery_topic_visible` + `test_nano_to_ros2`); the
      missing **rate-observation** path closed by the new
      `test_ros2_topic_rate_via_echo_interop` (counts `ros2 topic echo`
      samples over an 8 s window, asserts the measured rate is within a
      0.3..3.0 Hz band of the talker's 1 Hz publish loop).
- [x] **`ros2_topic_hz` helper landed** in
      `packages/testing/nros-tests/src/ros2.rs` for future use; the
      hz-based assertion itself sits behind `#[ignore]`
      (`test_ros2_topic_hz_interop`) because `ros2 topic hz` against
      `rmw_zenoh_cpp` errors with
      `failed to initialize wait set: the given context is not valid …`
      before any "average rate" line emits (the rclpy wait-set is
      polled after rmw_zenoh's shutdown handler runs). Re-enable once
      the upstream interaction is fixed.
- [x] **Skip-clean pattern** — every test above already gates on
      `require_ros2()` / `require_rmw_zenoh()` / `require_lifecycle_node_binary`;
      no new helper needed.
- **Files:** `packages/testing/nros-tests/tests/rmw_interop.rs`
  (`test_ros2_topic_rate_via_echo_interop` + ignored
  `test_ros2_topic_hz_interop`),
  `packages/testing/nros-tests/src/ros2.rs` (`ros2_topic_hz` helper +
  doc-comments explaining the `--spin-time` / `stdbuf` rationale).
  Existing coverage cited above is unmodified.

### 211.D — Conditionals + scoping deploy-time eval

`<arg name="enable_logger" default="false"/>` + `<node if="$(var enable_logger)">…</node>`
— the schema has `if_condition`/`unless_condition` fields but no fixture
exercises **deploy-time** evaluation (the parser does the `$(var …)`
substitution; the planner has to honour the boolean).

- [ ] **Planner change:** emit only entities whose `if_condition` resolves
      truthy + `unless_condition` resolves falsy after launch-arg
      substitution.
- [ ] **`<group>` scoping** — preserve nested `<group>` namespace prefixes
      on child entities (today the planner flattens).
- [ ] **Fixture + e2e** — a launch with `enable_logger` arg, deploy with
      `--launch-arg enable_logger:=true` (logger node present) vs
      `:=false` (logger absent). Same workspace, two plans, two
      `nros deploy` runs.
- **Files:** `nros-cli/packages/nros-cli-core/src/orchestration/planner.rs`,
  `packages/testing/nros-tests/tests/orchestration_conditionals.rs`.

### 211.E — `<set_remap>` / `<set_env>` / `<executable>`

Parser reads them; planner ignores. Production launches use
`<set_remap>` for global remaps inside a `<group>`, `<set_env>` to
parameterise downstream `<executable>` calls.

- [ ] **Planner:** thread `set_remap` scope down into child entities;
      thread `set_env` into the plan's per-entity env block.
- [ ] **`<executable>`** — emit as a non-rmw "spawn" plan entity that the
      generated entry lib runs alongside (or refuses to deploy with a
      clear error if the deploy kind doesn't support it).
- [ ] **Fixture + e2e** — one launch that wraps two `<node>`s in a
      `<group>` with a `<set_remap from="/in" to="/scoped/in"/>`; verify
      both nodes resolve `/scoped/in`.
- **Files:** `nros-cli` planner + new test under `packages/testing/nros-tests/tests/`.

### 211.F — Multi-host launch + the `machine=` attr

ROS 2 launches with `<node machine="robot">` route the node to a remote
host. nano-ros today plans one host at a time. A real production launch
(simulator on workstation + autopilot on Jetson) needs this.

- [ ] **Schema** — extend `nros-plan.json` `entities[*]` with an optional
      `host_id`; new `nros.toml` `[host.<id>]` blocks (ssh target,
      deploy kind override).
- [ ] **`nros deploy --all-hosts`** — run the per-host deploy in
      parallel (or serial — see e2e); each host gets only its own
      entities + the bridge config.
- [ ] **Fixture + e2e** — single-machine *simulated* multi-host using
      two domain-isolated processes (no real ssh needed for CI).
- **Files:** `nros-cli` planner + cmd + generate; nano-ros side: nothing
  new — already supports cross-process via rmw.

### 211.G — `launch_testing` equivalent assertion harness

ROS 2 `launch_testing` lets you assert "topic X publishes ≥ N Hz for ≥ T s",
"node enters Active in ≤ T s". nano-ros has no equivalent — every e2e
hand-rolls the assertion.

- [ ] **`nros test <plan>`** subcommand — accepts a `.test.yaml` next to
      the plan with assertion entries (topic_rate, lifecycle_state,
      service_response, log_match). Starts the deploy, waits, asserts.
- [ ] **Fixture:** `nros-cli/testing_workspaces/launch_testing_e2e/`
      with a `system.test.yaml`.
- [ ] **Reuse the existing `wait_for_output_pattern` + `count_pattern`
      machinery** in nros-tests; expose them as a Rust crate the
      `nros test` runner links.
- **Files:** `nros-cli/packages/nros-cli-core/src/cmd/test.rs` + the
  matching Rust assertion crate, mirror fixture.

### 211.H — DDS `qos_overrides` from launch arg

ros2 launch supports `<param name="qos_overrides./topic.publisher.reliability" value="reliable"/>`
and an argument `qos_overrides_file`. The nano-ros planner doesn't
surface these to the runtime — a user porting an existing launch can't
override QoS per-topic.

- [ ] **Planner:** parse `qos_overrides.<topic>.<role>.<setting>`
      parameter prefixes into a `qos_overrides` per-entity block.
- [ ] **Runtime:** `nros-node` honours the override when constructing the
      publisher / subscriber (today QoS is the Rust-API default).
- [ ] **Fixture + e2e** — deploy with default QoS (best-effort) +
      qos_overrides file (reliable) → assert reliable counters increment
      in the rmw layer.
- **Files:** `nros-cli` planner + `packages/core/nros-node/src/qos.rs`
  (or wherever the qos override slot already lives — Phase 193).

### 211.I — Mixed-RMW discovery + bridge fixture

Phase 128/129 landed `nros-bridge` for in-process cross-rmw forwarding.
**No fixture exercises** "nano-ros XRCE node + stock cyclonedds Autoware
node discover each other" via the bridge. This is the headline use case
the bridge was built for.

- [ ] **Fixture:** `nros-cli/testing_workspaces/mixed_rmw_bridge_e2e/`
      with one nano-ros XRCE talker + one stock cyclonedds listener
      (or vice versa) + a bridge node.
- [ ] **In-tree e2e** — deploy both, assert the listener receives.
- [ ] **Document the bridge config** in the book (a real "cross-RMW
      gateway" recipe).
- **Files:** mirror fixture + `nros-tests/tests/mixed_rmw_bridge.rs` +
  book pages under `book/src/user-guide/`.

### 211.J — `<include>` recursion safety + depth cap

`<include>`-of-`<include>`-of-… works mechanically but has no cycle
detection or depth cap. A real Autoware launch tree includes 20+ files.

- [ ] **Planner:** depth-cap (default 16) + cycle detection (visited-set).
- [ ] **Fixture + e2e** — three-level include chain + a cyclic include
      that the planner rejects with a clean error.
- **Files:** `nros-cli` planner.

## Acceptance

- [ ] In-tree `nros-tests` exercises the full plan→deploy→assert pipeline
      without depending on `nros-cli`'s own test suite (211.A).
- [ ] Composable-node container deploys as ONE process hosting N node
      handles (211.B).
- [ ] At least one `ros2 <subcommand>` host-CLI round-trip is e2e-proven
      against a nano-ros deploy (211.C — param OR lifecycle OR topic).
- [ ] Conditional-node deploy works both branches (211.D).
- [ ] A real ROS 2 workspace fixture (rclcpp samples or a vendored
      `demo_nodes_cpp` package) — NOT a synthetic `demo_pkg` —
      `nros plan`s + `nros deploy`s + publishes. This is the "real ROS
      production" claim, end-to-end. Lives behind the same skip-on-missing
      pattern as the other interop tests.

## Notes

- Most items split across nano-ros + nros-cli. nros-cli changes need a
  release + the `scripts/install-nros.sh` pin bump in nano-ros (Phase 207
  exercised that flow twice).
- The vendored `play_launch_parser` already supports far more than the
  planner consumes — this phase is mostly "consume what's already
  parsed" + "add the in-tree fixture coverage to prove it works".
- The bigger items (211.B composable, 211.G `nros test`) deserve their
  own sub-phases if scope balloons.
- `launch_testing`-style assertions (211.G) overlap with Phase 196 (CI
  bring-up) — if 211.G's harness lands, the CI workflows in 196 can
  reuse it.
