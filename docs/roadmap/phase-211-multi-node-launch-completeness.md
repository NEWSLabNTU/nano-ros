# Phase 211 — Multi-node + launch-file completeness against real ROS production

**Goal.** Close the gap between nano-ros's orchestration surface (`nros plan` /
`nros deploy` / `nros run-target` + Autoware-level launch parsing via the
vendored `play_launch_parser`) and what a real ROS 2 production system
exercises (composable containers, intra-process zero-copy, `ros2` CLI
interop, lifecycle transitions, conditional groups, dynamic loads,
multi-host, mixed-RMW). The parser layer is strong (260 tests, Autoware
100 %); the **planner consumption** + the **in-tree fixture coverage** +
the **runtime-interop story** are where the gaps sit.

**Status.** Re-scoped 2026-06-13 (was Proposed 2026-05-31). The
parser/planner spine + most fixtures LANDED (211.A core, B-planner, C, D, E,
I, J — all `[x]`). The remaining work was re-scoped against the current design
(RFC-0043 typed entry + the board/`[deploy.<name>]` model) — see **Design drift
& re-scope** below. Net: the deploy-verb-centric bullets are superseded; three
genuine production gaps (multi-host, `nros test`, qos_overrides) + the
real-ROS-workspace acceptance remain.

## Design drift & re-scope (2026-06-13)

This phase was written (2026-05-31) against an orchestration tail that assumed a
runtime **`nros deploy <name>`** verb generating an entry crate whose components
were resolved by the type-erased `__nros_component_<pkg>_register` interpreter.
Two design moves since then change the tail (the parser→planner→`nros-plan.json`
spine is unchanged + still current):

1. **No `nros deploy` runtime verb.** Deploy is now a `nros.toml`
   `[deploy.<name>]` target table (SSOT; `nros new --deploy` scaffolds it,
   `nros check` validates it) + the board/platform build that produces and runs
   the binary. The CLI verbs are `nros plan` / `check` / `explain` /
   `codegen[-system]` / `metadata` — there is no `deploy` command. Every
   "`nros deploy` second-stage" bullet below (211.A, 211.D, parts of 211.F) is
   therefore **superseded** — re-targeted onto the entry-pkg + board build.
2. **RFC-0043 typed entry replaced the register-symbol interpreter.** A planned
   system bakes to a typed entry (`emit_typed` → `Board::run_components`
   constructing each component + `configure(node)`), NOT a `register`-symbol
   interpreter. The **211.B runtime "one-process-many-nodes"** item is realized
   by the typed multi-node entry (`cpp_multi_node_entry_typed` —
   `multi_node_workspace_cpp_typed_pubsub_e2e`: 2 nodes, 1 PID) — **superseded /
   done elsewhere** (phase-240/242).
3. **`nros` is never a build/test/flash verb (RFC-0024 §4).** "nros =
   provisioner + codegen + metadata. Idf.py-shaped, not colcon-shaped." So the
   earlier **211.G `nros test`** subcommand is out of scope — the
   `launch_testing`-equivalent harness lives in the `just` + `nros-tests`
   runtime/test layer; the CLI contributes only codegen (it may *emit* a test
   scaffold, never *run* one). Re-scoped below.

> **Post-Phase-218**: References to `scripts/install-nros.sh` pin
> bumps below predate the Phase 218 monorepo merge. The CLI now lives
> in-tree at `packages/cli/` (build via `just setup-cli`); pin bumps
> are no longer relevant.

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
       │  emit_typed (RFC-0043) — one component class + configure() per node
       ▼
   typed entry (Board::run_components)   ─── compile / west build / vendor-module emit
       │
       │  nros.toml [deploy.<name>] target (SSOT) + board/platform build
       ▼
   binary / sim / hardware target runs the components
```

> NOTE (2026-06-13): the old tail read "generated entry crate → `nros deploy
> <name>` runs the binary". There is no `nros deploy` verb; the entry is a TYPED
> entry (RFC-0043) and the run is driven by the `[deploy.<name>]` target + the
> board build. See **Design drift & re-scope** above.

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
                                 │ typed entry (RFC-43) │  emit_typed → Board::run_components
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
- [x] **Deploy second-stage — LANDED (2026-06-13).** Was deferred ("once
      demo_pkg compiles + exports `nros_component_talker`"); both premises are
      retired (no `nros deploy` verb; register-symbol → RFC-0043 typed entry).
      Proven end-to-end by `deployed_native_system_e2e` (new): the planned
      `examples/workspaces/rust` deploy (`native_entry`,
      `nros::main!(launch = "demo_bringup:…")`) builds → boots → spins →
      **publishes `std_msgs/Int32` to the ROS graph**, and a separate `listener`
      process receives it. Two mechanisms made this work without code changes:
      (a) the macro's env-gated hosted spin `NROS_ENTRY_SPIN_MS` /
      `NROS_ENTRY_SPIN_STEP_MS` (`nros-macros::main_macro`) RUNS the system for a
      bounded test window; (b) the e2e is **cross-process** because zenoh-pico
      (the `nros-rmw-zenoh` backend) has a documented "write filter" limitation
      — **in-process pub/sub doesn't deliver**, regardless of shared session or
      distinct executors in one OS process (`trigger_conditions.rs` header,
      `component_runtime.rs` note). So the deployed system's own in-process
      listener sees nothing; delivery is observed from the separate subscriber,
      the canonical out-of-process topology every nano-ros pubsub e2e uses. The
      plan-only `orchestration_e2e` gate stays the planner-regression guard.
- **Files:** `packages/testing/nros-tests/fixtures/orchestration_e2e/*`,
  `packages/testing/nros-tests/tests/orchestration_e2e.rs`,
  `packages/testing/nros-tests/src/lib.rs` (`nros_cli_bin_path`, `require_nros_cli`),
  `packages/testing/nros-tests/Cargo.toml` (`serde_json` dep + test target).

### 211.B — Composable-node planner handling (biggest production gap)

`play_launch_parser` reads `<node_container>` / `<composable_node>` /
`<load_composable_node>`; the nros-cli planner currently emits each as a
separate plan entity. Production ROS (Nav2, Autoware, MoveIt) **relies**
on a single container hosting many nodes for intra-process zero-copy.

- [x] **Planner-side grouping** — resolved upstream in `nros-cli`
      `706023c`. `PlanInstance` gains additive `kind` (`"node"` /
      `"container"` / `"composable_node"`, default `"node"`) and
      `container_id: Option<String>` (skip-when-None). Planner reads
      `record.container`, mints a container `PlanInstance` for each,
      builds a `launch_name → instance.id` map, then resolves every
      `record.load_node`'s `target_container_name` (handling FQN /
      leading-slash-stripped / trailing-segment forms) onto the child's
      `container_id`. `<node_container>` no longer trips the
      `missing-source-metadata` diagnostic (stock containers like
      `rclcpp_components::component_container` aren't nros
      components). Per-child parameters / remaps unchanged. Gated by
      `composable_container_plan_shape`.
- [x] **Runtime: one-process-many-nodes** — **DONE via RFC-0043 typed entry
      (2026-06-13).** Was framed as "2 composable libraries in one container
      binary, both publish from the same PID". The typed multi-node entry
      realizes exactly this: `emit_typed` bakes N component classes into one
      entry, `Board::run_components` constructs + `configure`s each and pumps
      them on a single `spin_once` loop. Gated by
      `multi_node_workspace_cpp_typed_pubsub_e2e` (two robot_entry components,
      one process, both publish — ≥1 Received asserted) +
      `cpp_multi_node_entry_typed`. The `<composable_node>`-specific intra-process
      zero-copy optimization is a separate future item (not a launch-completeness
      gap; tracked wherever intra-process transport lands).
- [x] **Fixture:** `packages/testing/nros-tests/fixtures/orchestration_composable/`
      mirrors the multi-component layout (`nros/components/{talker,listener}.toml`)
      with a `<node_container>` + 2 `<composable_node>` children sharing a
      remapped `/chatter_a` topic. Pre-baked `record.json` so the test runs
      without `play_launch_parser` on PATH.
- [x] **`composable_container_plan_shape`** (in
      `packages/testing/nros-tests/tests/orchestration_composable.rs`):
      asserts 3 instances (container + 2 composables), the container
      carries `kind = "container"` and NO `container_id`, both
      composables carry `kind = "composable_node"` + `container_id`
      matching the parent's `instance.id`, per-composable `<param>`
      override propagates (`rate_hz = 20`), `<remap>` resolves both
      endpoints to `/chatter_a`, and `components` lists both class
      entries. Active gate, no `#[ignore]`.
- **Files:**
  *(planner)* `nros-cli` `706023c` —
  `packages/nros-cli-core/src/orchestration/{planner,plan}.rs`
  (`PlanInstance.kind` / `container_id`; container loop +
  composable container_id resolution);
  *(this tree)*
  `packages/testing/nros-tests/fixtures/orchestration_composable/*`,
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
- [→] **`nros deploy` second-stage** — **SUPERSEDED (2026-06-13)** with 211.A's
      (no `nros deploy` verb; register-symbol retired → typed entry). The
      plan-shape gate above already catches every planner-side regression of
      211.D's listed behavior; build-and-run is the typed-entry e2e's job.
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
- [x] **`<executable>` as a non-rmw spawn entity** — resolved upstream
      in `nros-cli` planner `4ad1ae8`. Parser still writes
      `<executable cmd="…">` as a `record.node` with `package=None`;
      planner now mints a dedicated `PlanExecutable {id, name,
      namespace, cmd, args, env, trace}` for each (new top-level
      `executables` collection on `NrosPlan`, additive +
      `skip_serializing_if = "Vec::is_empty"` so pre-211.E plans
      round-trip byte-identical). No more `missing-package`
      diagnostic for launches that carry rviz / rosbag / similar
      raw commands. Gated by `executable_emits_spawn_entity`.
- [x] **Fixture + e2e** —
      `packages/testing/nros-tests/fixtures/orchestration_set_remap_env/`
      wraps two `<node>`s in a `<group>` carrying both `<set_remap>` and
      `<set_env>`, AND carries a sibling `<executable cmd="/bin/echo"
      name="greeter">` with two `<arg>` children. Pre-baked
      `record.json` decouples the test from the parser binary on PATH;
      the same fixture exercises all three sub-bullets in one plan run.
- **Files:**
  *(this tree)*
  `packages/testing/nros-tests/fixtures/orchestration_set_remap_env/*`,
  `packages/testing/nros-tests/tests/orchestration_set_remap_env.rs`;
  *(planner)*
  `nros-cli` `0b78ab8` (set_env propagation + `EnvDecl`) +
  `4ad1ae8` (`PlanExecutable` + non-rmw spawn-entity emit) —
  `packages/nros-cli-core/src/orchestration/{planner,plan,schema}.rs`.

### 211.F — Multi-host launch + the `machine=` attr

ROS 2 launches with `<node machine="robot">` route the node to a remote
host. nano-ros today plans one host at a time. A real production launch
(simulator on workstation + autopilot on Jetson) needs this.

**STILL RELEVANT — re-scoped (2026-06-13); BLOCKED on a parser gap.** No
`nros deploy` verb, so the runtime split is per-host `[deploy.<id>]` targets +
the board build, not a `deploy --all-hosts` flag.

**Parser + planner LANDED (2026-06-13).** The earlier draft's blocker (the
parser didn't record `machine=`) is cleared: `play_launch_parser` now records
`<node machine="…">` (jerry73204/play_launch_parser `4144b8c`, bumped into the
superproject), and the planner lowers it into `host_id`.

- [x] **Parser (prerequisite):** `play_launch_parser` records `<node machine>`
      on its node record — `NodeRecord.machine` (additive, omitted-when-absent),
      threaded through the XML + IR node paths (`test_node_machine_attr_recorded`).
- [x] **Schema + lowering:** `PlanInstance.host_id` (additive,
      `skip_serializing_if`); `build_node_instance` forwards the record's
      `machine`, `schema_instance` lowers it onto `host_id`
      (`plan_system_lowers_machine_to_host_id`). Single-host plans byte-compat.
- [ ] **`nros.toml` host targets** — model each host as a `[deploy.<id>]` target
      (the existing SSOT table — kind `self`/vendor, board, ssh/target override)
      rather than a new `[host.<id>]` block; a multi-host system maps its
      `host_id` partitions onto these deploy targets. Reuse `scaffold_deploy`.
- [x] **Per-host bake — LANDED (`e7e9cbfff`).** The entry codegen partitions by
      host: `launch_parser::NodeSpec.machine` → `entry::PlanNode.host`;
      `Plan::for_host(id)` keeps host `id`'s nodes + all unhosted (shared) nodes
      (`Plan::hosts()` = the `machine=` set); `nros codegen entry --host <id>`
      bakes the per-host entry (errors if a host names no nodes + no shared
      exist). Single-host launches unaffected (all nodes unhosted → kept). Unit:
      `plan_for_host_partitions_by_machine`. In-tree codegen path only — the
      `nros::main!()` macro (git-pinned nros-build) gets `--host` parity when
      that distribution lands.
- [ ] **Fixture + e2e** — single-machine *simulated* multi-host: bake two
      per-host entries from one 2-`machine` launch, run as two processes (the
      proven `deployed_native_system_e2e` cross-process pattern ×2), assert
      cross-host delivery. The mechanism (above) + cross-process delivery
      (211.A) are both proven; this is the two-entry runtime demonstration.
- [ ] **`nros.toml` host targets (optional)** — model each host as a
      `[deploy.<id>]` target so a multi-host system maps `--host` bakes onto
      deploy targets via `scaffold_deploy`. Convenience over the bare
      `--host` codegen.
- **Files (landed):** `nros-cli-core/{launch_parser,codegen/entry/{mod,emit_*},
  cmd/codegen}.rs`. nano-ros runtime side: nothing new — cross-process works.

### 211.G — `launch_testing` equivalent assertion harness

ROS 2 `launch_testing` lets you assert "topic X publishes ≥ N Hz for ≥ T s",
"node enters Active in ≤ T s". nano-ros has no equivalent — every e2e
hand-rolls the assertion.

**`nros test` is RULED OUT (definitive) + the harness already EXISTS — so this
is OPTIONAL convenience, not a gap (2026-06-13).**

`nros test` cannot happen: RFC-0024 §4 ("nros never a build verb. No `nros
build` / `nros test` / `nros flash`. nros = provisioner + codegen + metadata"),
reaffirmed by RFC-0027 (the Phase-222 note: those verbs were *removed*) and
RFC-0040 ("provisioner + codegen + metadata; no build/run"). The CLI surface is
`plan` / `check` / `explain` / `codegen[-system]` / `metadata` — testing is not
and will not be a verb.

And the launch_testing-equivalent capability **already exists** in the
`nros-tests` runtime layer: `ManagedProcess::spawn_command`,
`wait_for_output_pattern`, `wait_for_output_count`, `count_pattern` +
`zenohd_unique`. `deployed_native_system_e2e` (Phase 211.A) uses exactly these
to deploy a planned system + assert cross-process delivery — i.e. an e2e
assertion over a deployed launch topology is **writable today**, no new
machinery. So a `launch_testing`-style `.test.yaml` is pure **data-driven
convenience** over the existing primitives (read a YAML of
topic_rate/lifecycle_state/log_match assertions → drive the same
`ManagedProcess`/`wait_for_*` calls), valuable for non-Rust authors but not a
missing capability.

- [→] **DOWNGRADED to optional.** If built: a `.test.yaml` schema + a
      `just <plat> test-system` recipe (or an `nros-tests` data-driven runner)
      over the existing primitives. NOT a CLI verb; NO `cmd/test.rs`. Until
      then, e2e tests are hand-written in `nros-tests` (the
      `deployed_native_system_e2e` pattern), which is the de-facto harness.

### 211.H — DDS `qos_overrides` from launch arg

ros2 launch supports `<param name="qos_overrides./topic.publisher.reliability" value="reliable"/>`
and an argument `qos_overrides_file`. The nano-ros planner doesn't
surface these to the runtime — a user porting an existing launch can't
override QoS per-topic.

**LANDED (2026-06-13) for the native path; rclcpp-faithful + RT-safe.** Design:
plan = authority, applied transparently — the user's `create_publisher(topic)`
is unchanged (matches rclcpp/rclrs/rclc); the override SOURCE is the deploy plan,
baked by codegen / consulted from an immutable `&'static` table — no runtime
param search, no alloc (RT-safe, setup-time). Two honoring paths because
generated entities and component-created entities take different create paths:

- [x] **Planner (wave1, `5f0f5eaff`):** `schema_qos_overrides` lowers
      `qos_overrides.<topic>.<role>.<policy>` params into a typed
      `PlanInstance.qos_overrides` block (split from `parameters`, dotted name
      decomposed via `rsplitn(3,'.')` so the `/`-bearing topic survives, sorted).
- [x] **Runtime (wave2, `abc6760cf`):** `nros-rmw` `QosOverride` typed value +
      `QosSettings::apply_overrides`; `NodeHandle` carries
      `qos_overrides: &'static [QosOverride]` + `set_qos_overrides`, merged in
      `create_{publisher,subscription}_with_qos` BEFORE `validate_against` (no
      silent downgrade). Serves COMPONENT-created entities (typed entry calls
      `set_qos_overrides` before `configure`).
- [x] **Codegen-bake (wave3a, `36082e8c8`):** generated subscriptions go through
      the executor `register_subscription_*` path (not `NodeHandle`), so
      `render_sub_qos_expr` bakes the merged QoS literal at GENERATION time
      onto all three generated-sub emit sites.
- [x] **Plan→plan e2e (wave4, `1476d53fc`):** `plan_system_lowers_qos_overrides`
      proves the real launch-param→`nros-plan.json` path + full schema round-trip.
- [→] **Typed C++ entry honoring (wave3b) — DEFERRED.** Emitting the static
      `QosOverride[]` table + a `nros_cpp_node_set_qos_overrides` FFI + the
      `set_qos_overrides` call in `emit_cpp` is the embedded/C++ extension. It
      touches `emit_cpp.rs` + `component_node.hpp` — the maintainer's hot
      phase-242/244 emit territory (collision risk) — and the only thing that
      exercises it (runtime delivery counters) rides on the deferred deploy
      second-stage. Sequence it after the 242/244 emit work settles.
- [→] **Runtime delivery e2e — DEFERRED with 211.A's deploy second-stage**
      (deploy a binary, count reliable vs best-effort samples in the rmw layer;
      needs a compilable native system + run). The native chain above is fully
      unit/integration-tested short of the live deploy.
- **Files (landed):** `nros-cli-core/orchestration/{plan,planner,generate}.rs`,
  `nros-rmw/src/traits.rs`, `nros-node/src/executor/node.rs`, `nros/src/lib.rs`.

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

- [x] In-tree `nros-tests` exercises the plan→assert pipeline without depending
      on `nros-cli`'s own test suite (211.A — `orchestration_e2e` + the
      per-feature `orchestration_*` gates).
- [x] One process hosts N node handles (211.B) — realized by the RFC-0043 typed
      multi-node entry (`multi_node_workspace_cpp_typed_pubsub_e2e`); the
      `<composable_node>` intra-process zero-copy optimization is separate.
- [x] At least one `ros2 <subcommand>` host-CLI round-trip is e2e-proven against
      a nano-ros node (211.C — param + lifecycle + topic-rate all covered).
- [x] Conditional-node plan works both branches (211.D — plan-shape gates).

**Remaining (re-scoped onto the current entry/board model):**

- [ ] Multi-host: a system with `machine="…"` partitions onto per-host
      `[deploy.<id>]` targets, each baking its own typed entry (211.F).
- [~] `launch_testing`-equivalent harness (211.G) — **DOWNGRADED to optional.**
      `nros test` is ruled out (RFC-0024 §4 / 0027 / 0040), and the runtime
      primitives already exist (`ManagedProcess` + `wait_for_*` + `count_pattern`,
      used by `deployed_native_system_e2e`). Hand-written `nros-tests` e2e is the
      de-facto harness today; a `.test.yaml` data-driven runner is convenience,
      not a gap.
- [ ] Per-topic `qos_overrides` from launch honored at runtime (211.H).
- [~] The plan→deploy→publish PIPELINE is proven end-to-end
      (`deployed_native_system_e2e`): a planned native workspace deploy builds,
      boots, spins, and publishes to the ROS graph; a cross-process subscriber
      receives. Remaining for the full "real ROS production" claim: run a
      **vendored real ROS 2 workspace** (e.g. `demo_nodes_cpp`) — NOT the
      synthetic `demo_bringup` — through the same pipeline, behind the usual
      skip-on-missing-`ros2` gate. The pipeline is no longer the gap; the real
      workspace fixture is.

## Notes

- Post-Phase-218 the CLI is in-tree at `packages/cli/` (build via
  `just setup-cli`); planner/`nros test` changes land there directly — no
  release + pin-bump dance. The pre-218 `scripts/install-nros.sh` flow the
  earlier draft assumed is retired.
- The vendored `play_launch_parser` already supports far more than the
  planner consumes — this phase is mostly "consume what's already
  parsed" + "add the in-tree fixture coverage to prove it works".
- The bigger items (211.B composable, 211.G `nros test`) deserve their
  own sub-phases if scope balloons.
- `launch_testing`-style assertions (211.G) overlap with Phase 196 (CI
  bring-up) — if 211.G's harness lands, the CI workflows in 196 can
  reuse it.
