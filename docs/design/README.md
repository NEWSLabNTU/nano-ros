# nano-ros design RFCs

This directory is the **design source of truth**. Each file is a numbered, living RFC:
a design decision record that is edited in place as the shape settles. Phase docs in
`docs/roadmap/` are *work breakdowns* — they implement RFCs, they do not own design
rationale. **New design rationale goes in an RFC, never only in a phase doc.**

- **Finalized whole-system view:** [ARCHITECTURE.md](ARCHITECTURE.md) — narrates the system
  end-to-end and links into the RFCs below.
- **New RFC:** copy [0000-template.md](0000-template.md) to `NNNN-slug.md`, next free number.
- **Status field** tells you stable vs evolving: `Draft` (moving) · `Stable` (settled) ·
  `Superseded` (retired by a hard reversal; see `archived/`).
- **Drift rule:** flipping an RFC to `Stable` requires updating the matching
  [ARCHITECTURE.md](ARCHITECTURE.md) section in the same commit.

Each RFC carries frontmatter: `rfc`, `title`, `status`, `since`, `last-reviewed`,
`implements-tracked-by` (phase slugs), `supersedes`, `superseded-by`.

## Index

### Foundations & architecture

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0001 | [architecture-overview](0001-architecture-overview.md) | Stable | layered crate stack + RMW + executor + board composition |
| 0002 | [rt-execution-model](0002-rt-execution-model.md) | Draft | real-time executor model (RTOS subsections pending) |
| 0003 | [rtos-integration-pattern](0003-rtos-integration-pattern.md) | Draft | universal embedded RTOS adapter pattern |
| 0004 | [configuration-and-transports](0004-configuration-and-transports.md) | Stable | unified config across single-node/workspace; `system.toml` + metadata; transports |

### RMW & data plane

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0005 | [rmw-layer-design](0005-rmw-layer-design.md) | Stable | middleware abstraction (delink zenoh-only) |
| 0006 | [portable-rmw-platform-interface](0006-portable-rmw-platform-interface.md) | Stable | Rust trait + C vtable dual API |
| 0007 | [service-qos](0007-service-qos.md) | Stable | service/action QoS design |
| 0008 | [service-qos-gap](0008-service-qos-gap.md) | Superseded | gap analysis → resolved by RFC-0007 (Phase 189.M3.3) |
| 0009 | [bridge-topic-forwarding](0009-bridge-topic-forwarding.md) | Stable | in-binary `[[bridge]]` raw-CDR topic relay |
| 0010 | [zero-copy-raw-api](0010-zero-copy-raw-api.md) | Stable | zero-copy raw publish/subscribe API |
| 0011 | [px4-rmw-uorb](0011-px4-rmw-uorb.md) | Stable | PX4 uORB RMW backend |
| 0031 | [rmw-selection-and-lowering](0031-rmw-selection-and-lowering.md) | Stable | RMW declared (system.toml/flag) + lowered per language; per-deploy |
| 0035 | [rmw-vtable-abi](0035-rmw-vtable-abi.md) | Draft | frozen 34-slot RMW vtable ABI + append-only rule + NULL contract + `abi_version` |
| 0058 | [multi-edition-ros-test-harness](0058-multi-edition-ros-test-harness.md) | Draft | **test-only** `RosEnv` provider so codegen + interop run against MULTIPLE ROS editions (a host holds only one). Two backends behind one trait: `HostRosEnv` (default edition, host `/opt/ros/<d>`, today's `ros2.rs`) + `DockerRosEnv` (extra editions, `nano-ros-ros:<distro>` image baking distro + pinned zenoh 1.7.2 overlay + `domain_bridge`, built locally on demand). Extra editions run in **opt-in** lanes covering two axes: **codegen** (`generate` in-container → `generated-editions/<E>/`, diff vs golden) + **testing** (host builds fixture against those defs → live interop vs the edition peer + `domain_bridge`). Default loop + user build untouched; motivated by issue 0267's manual jazzy verification. Work: phase-309 |
| 0059 | [launch-toolchain-split](0059-launch-toolchain-split.md) | Draft | Split the launch toolchain: keep only CPython-requiring stages in a **separately-built** tool (built on the user's machine against the user's Python, the way `just setup-cli` builds `nros`), and LINK the Python-free resolve pipeline into our own toolchain. The seam becomes a committable expanded-launch **IR artifact**, not a CLI verb — which is what removes the unbounded version skew of issue 0285 rather than managing it. Measured motivation: all **101** tracked `examples/**/*.launch.*` are XML, **zero** use `$(eval …)`, so the whole in-tree corpus needs no interpreter — yet is blocked today behind a tool that embeds one unconditionally. Real Python boundary is NOT "XML vs .py": `$(eval)` reaches CPython from XML and YAML equally. Open decisions: `$(eval)` policy, the fate of the duplicate in-tree `nros-launch-parser`, whether z3-bearing `check` follows |
| 0057 | [ament-parity-component-registration](0057-ament-parity-component-registration.md) | Draft | ament-parity registration surface: split `nano_ros_auto_add_library` + `nros_components_register_node(target PLUGIN … EXECUTABLE …)` (keyword parity with rclcpp_components — upstream CMakeLists ports by macro-name swap); **retire Phase 212.L.4** (class prefix == pkg) — cmake writes `pkg: ${PROJECT_NAME}` into nros-metadata.json, metadata.rs keys on it, nested upstream namespaces (`autoware::x::Class`) port verbatim (issue 0275); `nano_ros_auto_add_library` auto-wires generated interface deps, hiding the topo-last superset-archive footgun (issue 0277 UX; porting-notes 02/08/12). `SHAPE` defaults rclcpp; `HEADER` stays the one nano-specific optional keyword (static-linking price, convention-derived). Legacy macros remain as compat spellings |
| 0056 | [ros-edition-axis](0056-ros-edition-axis.md) | Draft | formalize **ROS edition** (target ROS 2 distro) as the third build axis (RMW × platform × **edition**) — a functional feature ordinary crates may branch on (unlike RMW/platform). One **per-edition interop profile** captures what varies on the wire across distros: **type hash** (Humble `TypeHashNotSupported` → Iron+ RIHS01, phase-41), **wire encoding/extensibility** (Humble XCDR1 → Jazzy+ XCDR2/appendable, RFC-0055/phase-303), and the **interface set** (`generated/<edition>/`). Extends the enum `humble`/`iron` → `+jazzy`/`+rolling`; selection via the `ros-<edition>` feature (as `nros-rmw-zenoh::keyexpr` already does). Turns "match the peer's distro" into one first-class selection rather than a per-call guess. Work: phase-41 (type hash) + phase-303 (encoding) |
| 0055 | [wire-encoding-xcdr2-extensibility](0055-wire-encoding-xcdr2-extensibility.md) | Draft | extend the wire encoding beyond XCDR1-only to **XCDR2 + explicit type extensibility** for modern ROS 2 (humble+) interop: `nros-serdes` gains an encoding-version-aware writer/reader that emits/parses the **DHEADER** for `@appendable` nested structs (encapsulation `0x0008/0x0009`), codegen declares extensibility explicitly (`nros-msg-to-idl` `@appendable`/`@final` → the Cyclone descriptor becomes correct for free; `rosidl-codegen` emits the matching Rust `EXTENSIBILITY` const), + `data_representation` QoS negotiation (advertise `[XCDR2, XCDR1]`, fall back for legacy peers). Roots issue #0267 (nano-ros CDR proven canonical; the gap is the missing DHEADER + implicit extensibility). Records the adjacent gaps: big-endian read, type-hash/RIHS. Non-goal: `@mutable`/optional (PL_CDR2/EMHEADER). Work: phase-303 |
| 0038 | [zero-copy-data-transport](0038-zero-copy-data-transport.md) | Stable | single-copy receive: size-class buffers + in-place dispatch; removes copy #1 + arena buffer (issue #8 / Phase 231) |
| 0039 | [px4-integration-architecture](0039-px4-integration-architecture.md) | Draft | PX4 umbrella: in-firmware uORB vs companion XRCE; 1.16 message versioning; px4-rs revision opportunities (RFC-0011 = uORB detail) |

### Platform, board & toolchain

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0012 | [board-bsp-integration-architecture](0012-board-bsp-integration-architecture.md) | Stable | vendor BSP × board × SDK-variant integration shape |
| 0013 | [custom-board-provisioning](0013-custom-board-provisioning.md) | Stable | out-of-tree boards self-describe deps to `nros setup` |
| 0014 | [nros-setup-toolchain-management](0014-nros-setup-toolchain-management.md) | Stable | `nros setup` as single toolchain entrypoint |
| 0015 | [rtos-orchestration](0015-rtos-orchestration.md) | Stable | launch tree + manifest codegen across RTOSes |
| 0016 | [rtos-scheduling-features](0016-rtos-scheduling-features.md) | Stable | per-RTOS scheduling feature survey |
| 0017 | [platform-timer](0017-platform-timer.md) | Stable | `PlatformTimer` + `AtomicSporadicState` |
| 0034 | [platform-layer-split](0034-platform-layer-split.md) | Draft | enforce `nros_platform_*` as sole system boundary; alloc-first unification; scalar vs opaque-struct ABI |
| 0042 | [platform-build-determinism](0042-platform-build-determinism.md) | Draft | one canonical `<nros/platform.h>`; capability-driven config SSoT in `nros-board.toml`; generated deterministic link manifest (one register path, no `--allow-multiple-definition`/`-u`); merge-time platform×lang gate |

### Language APIs

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0018 | [cpp-api-design](0018-cpp-api-design.md) | Stable | C++ surface mirroring rclcpp over typed FFI |
| 0019 | [nros-c-thin-wrapper-discipline](0019-nros-c-thin-wrapper-discipline.md) | Stable | nros-c delegates, never re-impls |
| 0020 | [thin-wrapper-audit](0020-thin-wrapper-audit.md) | Stable | nros-c / nros-cpp thin-wrapper compliance audit |
| 0021 | [blocking-api-rules](0021-blocking-api-rules.md) | Stable | every blocking helper takes an executor handle |
| 0022 | [entity-api-tiers](0022-entity-api-tiers.md) | Stable | convenient `fork` + customizable `clone` entity ctors |
| 0036 | [ros2-api-divergences](0036-ros2-api-divergences.md) | Draft | authoritative catalog of nano-ros vs rclrs/rclcpp/rclc divergences + rationale |
| 0037 | [rust-c-user-api-surface](0037-rust-c-user-api-surface.md) | Draft | records the Rust (`nros-node`) + C (`nros-c`) user surfaces; C++ is 0018 |

### Codegen, workspace & user workflow

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0023 | [codegen-workspace-discovery](0023-codegen-workspace-discovery.md) | Stable | unmodified ROS 2 msg pkg builds against nano-ros |
| 0024 | [multi-node-workspace-layout](0024-multi-node-workspace-layout.md) | Draft | overall multi-node workspace shape + open questions |
| 0025 | [workspace-layout-by-case](0025-workspace-layout-by-case.md) | Draft | concrete file trees per case (single/multi × rust/cpp) |
| 0026 | [example-directory-layout](0026-example-directory-layout.md) | Stable | canonical `examples/<plat>/<lang>/<example>/` shape |
| 0027 | [ros2-user-workflow](0027-ros2-user-workflow.md) | Stable | user-facing workflow + `nros new` scaffolding |
| 0030 | [sequence-of-nested](0030-sequence-of-nested.md) | Draft | nested-sequence message handling (Phase 212 spike) |
| 0032 | [entry-codegen-pipeline](0032-entry-codegen-pipeline.md) | Draft | how `main()` is emitted: proc-macro + CLI mirror, BoardEntry run/run_tiers, tier resolver |
| 0033 | [message-field-capacity-config](0033-message-field-capacity-config.md) | Stable | per-field, language-agnostic message capacity via `nros-codegen.toml`; owned/heap/borrowed modes |
| 0040 | [distribution-and-scaffolding-deps](0040-distribution-and-scaffolding-deps.md) | Draft | no-crates.io source-release model; `nros new` dep convention: in-tree + out-of-tree both use the ws-sync patch-block (git deps opt-in); CLI install line |
| 0041 | [unified-callback-receive-model](0041-unified-callback-receive-model.md) | Draft | service/action clients move from poll/`Promise`-single-buffer to callback-at-spin + QoS-depth `BufferStrategy` (triple/ring), unifying with subscriptions; dual-mode (Promise kept); ROS-aligned (KEEP_LAST 10, rclcpp callbacks); RT-safe + poll-backend (XRCE) compatible |
| 0043 | [entry-real-callback-binding](0043-entry-real-callback-binding.md) | Draft | resolves RFC-0032 §8a "callback bodies"; Entry path routes real user callbacks to the Rust executor (not the string-descriptor register), component = stateful object binding callbacks by identity (no naming, RFC-0019 thin wrapper), typed codegen entry; retires the synthesizing `EntryNodeRuntime`; NuttX executor-callback spike validated |
| 0045 | [unified-boot-config-resolution](0045-unified-boot-config-resolution.md) | Draft | one boot-config resolver (`node_name`/`locator`/`domain`) feeding the existing `RmwConfig`→`CffiRmw::open` sink; precedence A (hosted env > baked overlay > compiled default); `BootConfig` + `ExecutorConfig::resolve` in `nros-node`, three thin call-sites (Rust boards / C / C++) fixing #98 across all boards + the C PID-name / C++ `nros_cpp` defects; single `.nros_boot_config` bake site seeding a future config-patch tool + build-time plan image. Backs #101 |
| 0046 | [launch-authoritative-node-identity](0046-launch-authoritative-node-identity.md) | Draft | node name + namespace come from the launch `<node name= namespace=>` (SSoT, rclcpp-style override), `exec=` selects the entity (Cargo bin / CMake target), code provides only a fallback default; ONE precedence rule resolved at the one shared site both languages funnel through (`Executor::node_builder`), launch identity injected per-component (W4a param rail). Stops Rust hardcoding `create_node` names; unifies with C/C++ codegen. Naming half of #105 (graph half = per-node liveliness token) |
| 0047 | [unified-sched-context-binding](0047-unified-sched-context-binding.md) | Draft | one sched-context binding mechanism across Rust/C/C++ at ROS's granularity — the **callback group**: groups are code-declared (first-class, rclcpp/rclrs shape), group→tier owned by `system.toml` by-name (NOT the package manifest → portable, RFC-0026), executor holds a config-seeded `group → sched_context` table bound at registration. phase-272 shipped the per-node degenerate case (node-name table at `node_builder`, #124/#119); phase-273 generalizes to per-callback-group (sub-node tiering) + moves group→tier off the manifest. Concurrency type (MutuallyExclusive/Reentrant) deferred. Binding only — `run_tiers`/execution (RFC-0016) untouched |
| 0048 | [cmake-ament-consumption](0048-cmake-ament-consumption.md) | Draft | a nano-ros C/C++ package is written in the **ament_cmake convention** (`find_package(nano_ros)` + `find_package(<msgs>)` + an `add_*` verb + `ament_target_dependencies` + `install` + `ament_package`) and its `CMakeLists.txt` is **byte-identical across every platform** — the board/RMW delta lives in `package.xml <export><nano_ros …/>`. Resolution is **source-backed** (nano-ros is a source distribution, #171 D2): `find_package(nano_ros)` finds the pulled checkout via `nano_ros_ROOT` + an in-tree config, no install/crates.io. `find_package(<msg_pkg>)` triggers codegen per line via `CMAKE_FIND_PACKAGE_REDIRECTS_DIR`; two verbs (`nano_ros_add_executable` for standalone entries — exe, or `add_library`-into-`app` on Zephyr — vs `nano_ros_add_node` for workspace components); toolchain via `nros setup` CMakePresets + `nros init`. Implements #171 D5; tracked by phase-287 |
| 0049 | [hierarchical-platform-board-config](0049-hierarchical-platform-board-config.md) | Stable | one knob schema, `nros-platform.toml` + `nros-board.toml` per package, fixed 4-rung ladder, native lane front-ends (Kconfig only where the host framework requires it); retires `zenoh_platforms.toml`; resolves the phase-282 promotion as zephyr platform defaults |
| 0050 | [system-model](0050-system-model.md) | Draft | **SystemModel** — one resolved, checked, YAML artifact per concrete variant (early binding: args bound, conditions gone, names FQN), produced by `play_launch resolve` from launch tree + ros-launch-manifest contracts (phases 34–35) + integrator system config; three layers (structure / contracts / execution+deployment); types in a shared `model` crate in ros-launch-manifest (both projects vendor it); resolve refuses on checker Errors, embeds warnings; nano-ros build bakes each `mcu:*` node's slice (tiers→RFC-0015/0047, budgets/QoS→endpoints, domain→RFC-0045 baked rung) and the executor gains on-target contract monitors; play_launch runtime consumes the `linux` slice — same numbers both sides of a cross-machine E2E budget. Producer-side sibling: play_launch `docs/design/system-model.md` |
| 0051 | [test-matrix-architecture](0051-test-matrix-architecture.md) | Draft | ONE declared test matrix (platform × lang × RMW × workload × {example,workspace,interop}) generating fixture rows + test lanes + deterministic port/domain assignments (injective allocator); one standard-node output checker (stock-ROS-demo behavior contract); launch via framework runner metadata over prebuilt artifacts; micro-test budget. Work: phase-295 |
| 0052 | [system-model-rtos-mapper](0052-system-model-rtos-mapper.md) | Draft | the nano-ros half of RFC-0050: one mapping table from the model's execution layer to RTOS primitives per platform (priority/sched_class, stack_bytes — fixes the FreeRTOS emit_cpp drop, core pinning, ThreadX preempt_threshold; platform-inapplicable knob in the selected target = bake-time ERROR), `class` → executor modes (real_time deadline monitor, time_triggered = existing TT window, sporadic budget fed from tier tables; `interrupt` rejected v1), contract layer → generated const monitor tables (codegen stamp-offset `max_age_ms` at take, rate/jitter per spin tick, node-path latency; play_launch rule-id vocabulary through nros-diagnostic-updater; zero flash cost when uncontracted). Ingestion = `nros codegen-system --model system_model.yaml`; `nano_ros_add_executable(... MODEL …)`; orchestration-ir TierDef converts `From` the shared sched crate (round-trip drift guard). Closes RFC-0050's deploy question: `[deploy]` lives in system.toml, play_launch resolve consumes it. Tracked by phase-296 |
| 0053 | [threadx-multi-tier-execution](0053-threadx-multi-tier-execution.md) | Draft | ThreadX gains multi-tier execution (one Executor per tier over one shared session) like freertos/zephyr/nuttx, via **codegen-baked static per-tier stacks** (Option A, over a `TX_BYTE_POOL` — chosen for exact RAM / link-time placement / no runtime-alloc failure / consistency with the other boards; the cross-RTOS norm) + a C `nros_threadx_create_task` shim, with `preempt_threshold` applied through ThreadX's **native** `tx_thread_preemption_change` — the one platform where the six-dim `non_preempt_scope` is a kernel primitive, not emulated. Each per-tier executor calls the portable `apply_tier_sched_policy` (phase-296 W5.4). Migration ladder v0 (single-executor tier policy) → A. Work: phase-297 |
| 0054 | [c-header-abi-ssot](0054-c-header-abi-ssot.md) | Stable | The hand-written C headers become the SINGLE source of truth for the RMW backend contract + platform port ABI: a new pure-header `nros-rmw-abi` package (+ `nros-platform-api` staying the platform home), with Rust consuming **committed bindgen output** (regen-diff gated; libclang maintainer/CI-only, embedded builds see plain checked-in Rust) and the hand mirrors + #238/#239 drift gates retiring. Closes the #131/#160/#238/#239 stale-mirror class by construction, incl. per-slot fn signatures. Work: phase-299 |

### Domain & safety

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0028 | [e2e-safety-protocol-integration](0028-e2e-safety-protocol-integration.md) | Stable | safety-critical platform integration analysis |
| 0029 | [zonal-vehicle-architecture](0029-zonal-vehicle-architecture.md) | Stable | zonal E/E architecture + nano-ros fit |

## Superseded

Retired RFCs live in [archived/](archived/) with `status: Superseded` and a forward pointer.

## How a decision moves: RFC → roadmap → code

1. **Draft RFC** captures the option space + open questions.
2. Discussion (PR/issue/phase doc) resolves the open questions.
3. A **phase doc** in `docs/roadmap/` carries the work items + acceptance tests and names the
   RFC in its `Implements:` header.
4. Implementation lands; the RFC flips to **Stable** and ARCHITECTURE.md is updated in the
   same commit.
