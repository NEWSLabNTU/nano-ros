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

**Status (audited 2026-05-31): planner-side already implemented;
in-tree regression gate added.** The audit ran `nros plan` against a
fresh fixture with `<arg enable_logger=false/>` + `<node if=$(var
enable_logger)>…</node>` and a `<group><push_ros_namespace
namespace="scoped"/>…</group>` wrap; both `enable_logger:=false`
(logger filtered out) and `:=true` (logger present at `/scoped/optional_logger`)
produced the correct plan shapes. The 211.D doc was written without
that audit — the gap was the missing in-tree gate, not a planner change.

- [x] **`if_condition` / `unless_condition` deploy-time eval** — already
      honored by the planner; entities whose condition resolves falsy
      do not land in `instances[*]`. Gated by
      `conditionals_disabled_omits_logger`.
- [x] **`<group>` + `<push_ros_namespace>` scoping** — child entities
      inherit the nested namespace prefix on both `namespace` and
      `launch_name`. Gated by
      `conditionals_enabled_keeps_logger_and_scopes_namespace`
      (asserts logger lands at `namespace="/scoped"` +
      `launch_name="/scoped/optional_logger"`).
- [x] **Fixture + e2e** —
      `packages/testing/nros-tests/fixtures/orchestration_conditionals/`
      with one launch file exercising both arg variants. Two pre-baked
      records (`record-false.json` + `record-true.json` — outputs of
      `play_launch_parser` with the corresponding `enable_logger:=<bool>`)
      decouple the test from the parser binary on PATH.
- [→] **`nros deploy` second-stage** — the original bullet said "two
      `nros deploy` runs". Deferred behind 211.A's deferred deploy
      stage: needs the demo_cond crate to actually compile + exported
      `nros_component_*` symbols. The plan-shape gate above already
      catches every planner-side regression of 211.D's listed behavior.
- **Files:**
  *(this tree)*
  `packages/testing/nros-tests/fixtures/orchestration_conditionals/*`,
  `packages/testing/nros-tests/tests/orchestration_conditionals.rs`.

### 211.E — `<set_remap>` / `<set_env>` / `<executable>`

**Status (audited 2026-05-31): mixed — `<set_remap>` already works
end-to-end, `<set_env>` parsed but not emitted by planner,
`<executable>` rejected at plan time.** A fresh audit ran the three
constructs through `nros plan` and inspected the resulting
`nros-plan.json` + `record.json`:

- [x] **`<set_remap>` propagation** — already implemented. Each child
      node's `instances[*].remaps` carries the scoped pair, and the
      subscriber's `resolved_name` reflects the remap target. Gated by
      `set_remap_propagates_to_group_children` (asserts both nodes carry
      `from=in → to=/scoped/in` AND their subscriber `resolved_name` is
      `/scoped/in`).
- [x] **`<set_env>` propagation** — resolved upstream in `nros-cli`
      planner `0b78ab8`. Parser already collected the entry
      (`record.node.env = [["DEMO_LEVEL", "verbose"]]`); planner now
      threads each pair onto the public schema as
      `instances[*].env: [{name, value}, …]` (new `EnvDecl` struct +
      `schema_env` reshape parallel to `schema_remaps`). Gated by
      `set_env_propagates_to_group_children`.
- [→] **`<executable>` planner-side gap** — parser records
      `<executable cmd="…">` as a `record.json` `node` with
      `package=None`; the planner then errors `missing-package: launch
      node has no package` (`planner.rs:102`) and refuses to emit the
      plan. The 211.E bullet calls for emitting `<executable>` as a
      non-rmw "spawn" plan entity (or refusing to deploy with a clear
      error when the deploy kind doesn't support it). Currently NOT
      exercised by the committed fixture — adding `<executable>` blocks
      the rest of the plan stage. Captured as
      `executable_emits_spawn_entity` (`#[ignore]` placeholder).
- [x] **Fixture + e2e** —
      `packages/testing/nros-tests/fixtures/orchestration_set_remap_env/`
      wraps two `<node>`s in a `<group>` carrying both `<set_remap>` and
      `<set_env>`. Pre-baked `record.json` decouples the test from the
      parser binary on PATH.
- **Files:**
  *(this tree)*
  `packages/testing/nros-tests/fixtures/orchestration_set_remap_env/*`,
  `packages/testing/nros-tests/tests/orchestration_set_remap_env.rs`;
  *(planner)*
  `nros-cli` `0b78ab8` —
  `packages/nros-cli-core/src/orchestration/{planner,plan,schema}.rs`
  (set_env propagation + `EnvDecl`); executable handling still
  pending in the same crate.

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

- [x] **In-tree bridge fixture binary:**
      `packages/testing/nros-tests/bins/bridge-zenoh-to-xrce-fwd/` —
      minimal sibling to the Phase 110.G `tt-zenoh-to-xrce` example
      (same dual-session topology: `Executor::open_with_rmw("zenoh", ...)`
      primary + `node_builder("egress").rmw("xrce").locator(...)` opens
      a second XRCE session). Carries the `std_msgs::msg::Int32` type
      name + hash so the keyexpr matches the standard `talker_binary` /
      `xrce_listener_binary` fixtures; no TT scheduling (irrelevant for
      the cross-RMW assertion).
- [x] **`test_zenoh_to_xrce_bridge_e2e`** in
      `packages/testing/nros-tests/tests/bridge_mixed_rmw.rs` —
      spawns zenohd → XRCE Agent → bridge → xrce listener → zenoh
      talker (in order; bridge before listener so the egress
      publisher is declared before the listener subscribes) and
      asserts ≥ 2 bridged samples reach the listener within a 10 s
      window. Verified 2026-05-31: 2 samples received.
- [→] **Stock cyclonedds variant** — the original "Autoware listener"
      framing replaces XRCE with stock `rmw_cyclonedds_cpp`. Needs the
      bridge to grow a cyclonedds egress (the in-tree fixture is
      zenoh+XRCE today; cyclonedds backend is C++/CMake-side and
      links differently). Deferred until a cyclonedds-enabled bridge
      fixture lands; the zenoh-↔-XRCE round-trip above is the
      foundation.
- [→] **Book documentation** — "cross-RMW gateway" recipe under
      `book/src/user-guide/`. Pairs naturally with the cyclonedds
      variant; deferred with that.
- **Files:**
  *(this tree)*
  `packages/testing/nros-tests/bins/bridge-zenoh-to-xrce-fwd/*`,
  `packages/testing/nros-tests/tests/bridge_mixed_rmw.rs`,
  `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
  (`build_bridge_zenoh_to_xrce_fwd`).

### 211.J — `<include>` recursion safety + depth cap

`<include>`-of-`<include>`-of-… works mechanically but has no cycle
detection or depth cap. A real Autoware launch tree includes 20+ files.

**Status (2026-05-31): RESOLVED.** Initial audit found both safety
guards as gaps; upstream landed both (`play_launch_parser` `098ccb4`
+ `nros-cli` planner `a2675aa`) and the in-tree `#[ignore]` gates
flipped on:

| Scenario | Behavior today | Gated by |
|----------|----------------|----------|
| `system → level_a → level_b → leaf` | leaf instance lands ✓ | `chain_3_levels_resolves_to_leaf` |
| `cycle_a → cycle_b → cycle_a → …` | parser raises `ParseError::CircularInclude`, `nros plan` exits non-zero with chain rendered as `a → b → a` in stderr | `cycle_rejected_with_clear_diagnostic` |
| 17-level chain w/ `NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH=16` | parser raises `MaxIncludeDepthExceeded`, `nros plan` exits non-zero | `depth_cap_rejects_over_16` |

- [x] **3-level chain plan-walk** — `chain_3_levels_resolves_to_leaf`.
- [x] **Cycle detection** — `play_launch_parser` `098ccb4` added the
      opt-in `ParseOptions::strict_includes` (default false →
      compat-preserving warn-and-skip stays the parser default) and
      the CLI flag `--strict-includes`. `nros-cli` planner `a2675aa`
      always passes `--strict-includes` so every `nros plan` surfaces
      the cycle as a hard error. Gated by
      `cycle_rejected_with_clear_diagnostic`.
- [x] **Depth-cap enforcement** — `MaxIncludeDepthExceeded` already
      existed (default 100); `play_launch_parser` `098ccb4` exposed
      the cap via `--max-include-depth <N>`. `nros-cli` planner
      `a2675aa` forwards `NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH` as
      `--max-include-depth`. Gated by `depth_cap_rejects_over_16`
      (sets the env var to 16 + writes a 17-level chain at runtime).
- [x] **Fixture + e2e** —
      `packages/testing/nros-tests/fixtures/orchestration_includes/`
      with three pre-baked records (`record-{chain,cycle,deep}.json` —
      parser output for each scenario) + `bake-records.sh`. The chain
      test uses `--record` for portability; the cycle + depth tests
      bypass `--record` and let the parser walk fresh launch files
      written into a tempdir at runtime (the only way to gate the
      parser's diagnostics end-to-end).
- **Files:**
  *(this tree)*
  `packages/testing/nros-tests/fixtures/orchestration_includes/*`,
  `packages/testing/nros-tests/tests/orchestration_includes.rs`;
  *(parser)*
  `play_launch_parser` `098ccb4` —
  `crates/play_launch_parser/src/{error,lib,main,traverser/{include,xml_include,ir_builder,ir_evaluator}}.rs`
  + `tests/include_safety.rs`;
  *(planner)*
  `nros-cli` `a2675aa` —
  `packages/nros-cli-core/src/orchestration/planner.rs`
  (`--strict-includes` + `NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH` forward).

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
