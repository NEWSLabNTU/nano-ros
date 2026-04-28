# RTOS Orchestration via Launch Tree + Manifest Codegen

**Status:** Draft
**Companion roadmap:** [Phase 94](../roadmap/phase-94-rtos-orchestration.md)
**Related repos:** `~/repos/nano-ros`, `~/repos/play_launch`, `~/repos/play_launch/src/ros-launch-manifest`, `~/repos/autoware-nano-ros`

---

## 1. Problem

ROS 2 launch files target Linux: each node = process, kernel scheduler arbitrates, runtime evaluates Python (Turing-complete) at launch time. RTOS targets work the opposite way:

- **One binary** per device — all packages compile-linked together.
- **Limited tasks** — RAM-bounded; cannot afford `O(nodes)` heavy threads.
- **No runtime Python** — codegen must resolve graph at build time.
- **Shared executor model** — `spin()` works differently than per-process Linux executor.

Today users of nano-ros on RTOS hand-write a `main.rs` that wires every publisher/subscriber/service/timer (autoware-nano-ros sentinel = **1472 lines**, 11 algorithm crates, 51 publishers, 22 services, 7 subscriptions, 1×30 Hz timer). This loses the launch-file mental model and is brittle.

**Goal:** allow users to organize RTOS code as a colcon-style workspace with `package.xml` + ROS 2 launch tree + sidecar manifest, build-time codegen the orchestration `main()`. Keep ROS 2 mental model, accept RTOS constraints.

---

## 2. Decisions (locked)

### 2.1 Launch semantics — build-time freeze

Run `play_launch_parser` w/ user-supplied launch arguments at build time. Freeze resulting graph. Conditions / `LaunchConfiguration` / `OpaqueFunction` resolved once during build. Graph-shaping arg changes = rebuild. Value-shaping arg changes = runtime override (see §10).

Rationale: pure-declarative variant rejects too much real-world launch-file content; runtime-arg-table variant inflates code size for marginal benefit on resource-constrained MCUs.

### 2.2 Execution model — priority-tier (Apex.OS / `ara::exec` style)

Codegen emits **one RTOS task per declared priority tier**, each owning one `nros::Executor`. Manifest assigns each node (or each callback group of each node) to a tier.

Degenerate cases (no rewrite of codegen needed):

- All nodes default tier → 1 task = single shared executor (matches nano-ros today and autoware sentinel).
- Each node distinct tier → N tasks = mirrors Linux per-node-process model.
- Mixed → matches Apex.OS / AUTOSAR `ara::exec` style.

Picks up REP-2014 (callback-group priority) data model statically at build time.

### 2.3 Zenoh-pico session — one shared session per binary

- Platforms with `Z_FEATURE_MULTI_THREAD=1` (POSIX, Zephyr, ESP-IDF): rely on zenoh-pico internal mutexes (`_z_session_t::_mutex_inner`, per-transport TX mutex, cancellation mutex).
- Platforms with `Z_FEATURE_MULTI_THREAD=0` (bare-metal, FreeRTOS, NuttX, ThreadX): codegen enables `ffi-sync` feature on `nros-rmw-zenoh`, wraps FFI in `critical_section::with()`. Bounded ~µs ISR-disabled window per call (one decomposed `spin_once(0)`).

Per-tier session model rejected:

- N× RAM cost (~2 KB batch buffer + declaration tables × N).
- Loopback A→B traffic must transit zenohd (no intra-process shortcut in zenoh-pico).
- No precedent in upstream rmw_zenoh, full Zenoh, or zenoh-pico (all use 1 session per process).

### 2.4 Project structure — extend, no new repo

Codegen + runtime hooks land in existing `nano-ros` and `play_launch` workspaces. Manifest schema crate goes next to `ros-launch-manifest-types` in play_launch. No third repo. Rationale: codegen is tightly coupled to both producer (parser) and consumer (Executor/Lifecycle/Platform API) — a third repo adds atomic-refactor cost without benefit.

---

## 3. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│ User repo (colcon-style workspace)                           │
│  ├── packages/my_node_a/                                     │
│  │   ├── package.xml                                         │
│  │   ├── Cargo.toml + src/                                   │
│  │   └── nros.toml          ← node manifest (logical groups) │
│  ├── packages/my_node_b/                                     │
│  │   └── nros.toml                                           │
│  └── packages/my_system/                                     │
│      ├── launch/system.launch.py  ← stock ROS 2 launch        │
│      └── nros.toml          ← system manifest (RTOS knobs)   │
└──────────────────────────────────────────────────────────────┘
                           │
                           ▼ build-time
┌──────────────────────────────────────────────────────────────┐
│ play_launch_parser                                            │
│  ├── evaluate launch tree w/ user args                       │
│  ├── resolve includes, conditions, substitutions             │
│  └── emit record.json (frozen ExecutionPlan)                 │
└──────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│ cargo-nano-ros codegen orchestrator                          │
│  ├── discover per-package nros.toml                          │
│  ├── merge record.json + per-package manifests               │
│  ├── verify: every node has crate + entry fn                 │
│  ├── verify: tier assignments cover all callbacks            │
│  ├── apply remaps + namespaces to topic/service strings      │
│  ├── emit shared_context C ABI + Rust + C++ headers          │
│  ├── emit per-tier task entry fns                            │
│  ├── emit toplevel main() (platform-specific)                │
│  └── emit Cargo workspace deps glue                          │
└──────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────┐
│ Generated orchestration binary                               │
│  main() {                                                    │
│    open shared zenoh session (ffi-sync if MT=0)              │
│    spawn task_tier_high (priority 90, stack 8K)              │
│    spawn task_tier_normal (priority 50, stack 4K)            │
│    spawn task_tier_low (priority 10, stack 4K)               │
│    join / WFI                                                │
│  }                                                            │
│  task_tier_X() {                                             │
│    let mut exec = Executor::open_with_session(shared);       │
│    register_node_a_to_exec(&mut exec);                       │
│    register_node_b_to_exec(&mut exec);                       │
│    loop { exec.spin_once(N); platform_yield(); }             │
│  }                                                            │
└──────────────────────────────────────────────────────────────┘
```

---

## 4. Manifest schema (extension to ros-launch-manifest)

Per-package TOML at `<pkg>/nros.toml` (or referenced from `package.xml` `<export>`). Codegen discovers + merges all manifests in workspace.

**Hard split between two manifest kinds — driven by who owns the knob:**

| Manifest kind       | Owner                  | What it declares                                                                                                                        | Why here                                                                                                       |
| ------------------- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **Node manifest**   | Package author         | Logical structure: crate, entry fn, callback groups (id + type + symbolic tier name), entity bindings                                   | Author knows the node's own concurrency requirements; doesn't know deployment target                           |
| **System manifest** | Deployer (board owner) | RTOS-specific physical: `tiers.<name>.<rtos>` priority/stack/scheduler tables, startup_order, launch_args, shared_state, node_overrides | Same node package can deploy to FreeRTOS / Zephyr / NuttX with different RTOS pri numbers; only deployer knows |

This mirrors ROS 2 launch-file philosophy: package = reusable component; launch file = deployment-specific composition.

Codegen requires exactly **one system manifest** per build target. Node manifests are picked up from every workspace package whose nodes appear in `record.json`.

### 4.1 Node manifest example

```toml
# packages/my_robot_control/nros.toml
schema = "nano-ros/orchestration/v1"
kind = "node"

[node]
name = "control_node"            # matches launch file Node(name=...)
crate = "my_robot_control"
entry = "register_control_node"  # fn(executor: &mut Executor) -> Result<()>

# Symbolic tiers only — physical mapping in system manifest.
[[node.callback_groups]]
id = "ctrl_loop"
type = "MutuallyExclusive"
tier = "high"                    # symbolic name; system manifest binds to RTOS

[[node.callback_groups]]
id = "telemetry"
type = "Reentrant"
tier = "low"

[[node.bindings.timers]]
name = "control_tick"
group = "ctrl_loop"

[[node.bindings.subscriptions]]
topic = "/imu"                   # post-remap
group = "ctrl_loop"

[[node.bindings.subscriptions]]
topic = "/diagnostics_req"
group = "telemetry"

[[node.bindings.services]]
name = "~/set_mode"
group = "telemetry"
```

### 4.2 System manifest example

```toml
# packages/my_system/nros.toml
schema = "nano-ros/orchestration/v1"
kind = "system"
target_rtos = "freertos"         # selects which [tiers.<X>.<rtos>] sub-table is consumed

# Symbolic tier table. Each tier MUST have a sub-table for the active target_rtos.
[tiers.high]
spin_period_us = 1000
[tiers.high.freertos]
priority = 5                     # FreeRTOS: higher = higher
stack_bytes = 8192
[tiers.high.threadx]
priority = 5                     # ThreadX: lower = higher (note inversion)
preempt_threshold = 5
stack_bytes = 8192
[tiers.high.nuttx]
priority = 200
sched_class = "SCHED_FIFO"
stack_bytes = 8192
[tiers.high.zephyr]
priority = 0                     # K_PRIO_PREEMPT(0)
stack_bytes = 8192
[tiers.high.posix]
priority = 80
sched_class = "SCHED_FIFO"

[tiers.normal]
spin_period_us = 10000
[tiers.normal.freertos]
priority = 3
stack_bytes = 4096
# ... (other RTOS sub-tables analogous)

[tiers.low]
spin_period_us = 100000
[tiers.low.freertos]
priority = 1
stack_bytes = 4096

# Optional node-level overrides — system can override the symbolic tier
# of a specific callback group declared in a node manifest.
[[node_overrides]]
name = "control_node"
[[node_overrides.callback_groups]]
id = "telemetry"
tier = "low"

# Cross-node startup ordering (lifecycle activation).
startup_order = ["sensing_node", "control_node"]

# Shared state (see §9).
[[shared_state]]
name = "safety_island"
schema = "SafetyIsland"
storage = "static"
sync = "tier_aware"
fields = [
    { name = "current_velocity", type = "f32" },
    { name = "mrm_state", type = "u8" },
    { name = "has_external_control", type = "bool" },
    { name = "last_external_control_ms", type = "u64" },
]
read = ["control_node", "validator_node"]
write = ["sensing_node", "mrm_node"]

# Launch arg classes (see §10).
[[launch_args]]
name = "robot_namespace"
class = "graph"
[[launch_args]]
name = "ctrl_gain"
class = "parameter"
target_param = "control_node.gain"
```

### 4.3 Invariants enforced by codegen

- Every node in launch tree (`record.json`) must have a discoverable node manifest, OR an implicit default applied (`tier = "normal"`, single MutuallyExclusive group covering all entities).
- Every callback group's symbolic `tier` must resolve in system manifest's `[tiers.*]` table.
- For the active `target_rtos`, every `[tiers.<X>]` must have a `[tiers.<X>.<target_rtos>]` sub-table.
- Tier task `spin_period_us` must be ≤ tightest timer period in its group set.
- `startup_order` entries must exist in launch tree.
- Binding entity names (`timers.name`, `subscriptions.topic`, `services.name`) reconciled with launch tree at build time; mismatches reported as build errors.
- Per-RTOS priority bounds enforced (FreeRTOS 0..configMAX-1, ThreadX 0..31, NuttX 1..255, Zephyr per CONFIG*NUM*\*, POSIX 1..99).

---

## 5. Callback groups — concept & application

### 5.1 What they are

rclcpp construct that bundles entities (subs, timers, services, clients) for **concurrency control by an executor**. Two flavors:

- **MutuallyExclusive** (default): executor guarantees no two callbacks of this group run concurrently. Safe default for state mutation.
- **Reentrant**: executor may dispatch multiple callbacks of this group in parallel on different threads. Required for callbacks that block on a sibling (service that calls `client->async_send_request().get()`, action server execute loop) — without Reentrant they deadlock.

Not a priority mechanism (REP-2014 draft proposes adding that). Not a deadline mechanism. Constrains _concurrency_; executor + scheduler decide _when_.

Source: `rclcpp/include/rclcpp/callback_group.hpp`. Default group per node = MutuallyExclusive. `Executor::add_callback_group(group, node_base)` allows splitting one node across multiple executors / threads.

### 5.2 Industrial usage

| System                            | Pattern                                                                                                                                                                                          |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Apex.OS / cbg_executor**        | One executor per priority tier, each bound to thread w/ SCHED_FIFO. Nodes register groups w/ specific tier executor. https://github.com/ros2/examples/tree/rolling/rclcpp/executors/cbg_executor |
| **ros-realtime/reference-system** | Single-threaded executor per "node bucket" + separate sensor input thread. https://github.com/ros-realtime/reference-system                                                                      |
| **Autoware.Universe**             | Dedicated MutuallyExclusive group per timer vs subscription set; keeps planning loops out of TF callback contention.                                                                             |
| **Nav2**                          | Dedicated group per action server (long execute callback); separate group for parameter callbacks.                                                                                               |
| **AUTOSAR `ara::exec`**           | Manifest declares Executable → Process → Thread w/ priority. Build-time binding.                                                                                                                 |

### 5.3 Mapping to launch + manifest

Stock launch files (`launch_ros.actions.Node`) carry `parameters`, `remappings`, `arguments` — **no callback-group field**. Groups created imperatively in C++ today. We add via manifest sidecar (§4).

Codegen consumes manifest, emits per group:

- One RTOS task = one priority tier (groups sharing tier coexist in one task).
- One `Executor` per task; group's callbacks pre-registered.
- Group type (Mutex vs Reentrant) → codegen picks executor semantics. nano-ros today is single-threaded; Reentrant within one task = serial anyway. Cross-task Reentrant = needs MultiThreadedExecutor support (future work, not v1).

**v1 scope:** all groups effectively MutuallyExclusive within their tier-task. Reentrant becomes meaningful only when we ship a multi-worker executor (post-v1).

---

## 6. Loss / gain analysis vs hand-written `main.rs`

Reference: autoware-nano-ros sentinel (1472 lines).

### What hand-written gets right today

- **Deterministic single-thread control:** 30 Hz timer + sequenced callbacks. No race conditions.
- **Staleness guard inline:** 2000 ms `external_control` threshold checked in timer body.
- **Conditional internal vs external control:** boolean flag `has_external_control`.
- **Static memory:** all 11 algorithms + 51 pubs + 22 services in static `RefCell<SafetyIsland>`.
- **NaN-safe defaults:** velocity parsing treats NaN as "stopped".

### What hand-written cannot express

| Feature                                                 | Codegen supplies                                        |
| ------------------------------------------------------- | ------------------------------------------------------- |
| Conditional node inclusion (`IfCondition`, env-derived) | Yes — frozen graph omits disabled nodes                 |
| Composable node groups                                  | Yes — manifest tier groups                              |
| Per-node namespace / remap                              | Yes — applied at codegen string-interp time             |
| Lifecycle activation order                              | Yes — `startup_order:` manifest field                   |
| Action server/client setup                              | Yes — manifest entity binding                           |
| Multi-rate timer scheduling                             | Yes — per-tier `spin_period_us` + per-timer `period_ms` |
| Cross-node dep declaration                              | Yes — `startup_order` + tier dep graph                  |
| Per-namespace parameter override                        | Yes — launch file `parameters:` flows through           |

### Flexibility lost (codegen cannot easily do)

- **Inline closure-captured shared state** (e.g. `SafetyIsland` `RefCell` holding all 11 algorithm structs). Codegen sharing model resolved in §9 (manifest-declared `shared_state:` + C-ABI accessors).
- **Cross-callback ad-hoc state mutation** in one timer body — autoware sentinel sequences MRM watchdog → handler → operators → cmd_gate → validator inline. If split across tiers, ordering becomes async. Mitigation: keep tightly-coupled algorithms in one tier (which collapses to today's pattern).
- **Verification-friendliness:** Kani harness on hand-written `main` is straightforward (single control flow). Codegen-emitted multi-task code complicates harness scope. Mitigation: codegen emits same shape as hand-written when degenerate (single-tier).

### Net assessment

Single-tier codegen output ≡ hand-written single-`main` shape (modulo formatting). All hand-written advantages preserved when user does not opt into multi-tier. Multi-tier opt-in unlocks ROS 2 launch features. **No loss in degenerate case; gain everywhere else.**

---

## 7. Zenoh-pico shared-session perf

### 7.1 Per-platform MT setting (already in `zpico-sys/build.rs`)

| Platform   | `Z_FEATURE_MULTI_THREAD` | Sync mechanism              |
| ---------- | ------------------------ | --------------------------- |
| POSIX      | 1                        | zenoh-pico internal mutexes |
| Zephyr     | 1                        | zenoh-pico internal mutexes |
| ESP-IDF    | 1                        | zenoh-pico internal mutexes |
| Bare-metal | 0                        | `ffi-sync` critical_section |
| FreeRTOS   | 0                        | `ffi-sync` critical_section |
| NuttX      | 0                        | `ffi-sync` critical_section |
| ThreadX    | 0                        | `ffi-sync` critical_section |

### 7.2 Shared-session cost (chosen)

- ISR-disabled window per FFI call: ~µs (one `zpico_spin_once(0)` = one TCP read + one CDR decode, or one `_z_send_n_msg` for publish).
- `spin_once(N)` decomposed in `zpico.rs:571–620` into loop of guarded `spin_once(0)` so critical section never spans whole timeout.
- Session footprint: 1× declaration tables (32 pub / 32 sub / 16 queryable defaults), 1× batch buffer (`Z_BATCH_UNICAST_SIZE` = 2048 B default), 1× peer state.

### 7.3 Per-tier session cost (rejected)

- N× session footprint. On 192 KB SRAM Cortex-M with 4 tiers ≈ prohibitive.
- A→B loopback transits zenohd (no intra-process shortcut in zenoh-pico). Real RTT + bandwidth cost.
- N TLS/TCP control blocks in smoltcp.
- N discovery announcements visible to network.
- No precedent — full Zenoh and rmw_zenoh on Linux both use 1 session per process.

### 7.4 Worked example — autoware sentinel on FreeRTOS

- All 11 algos in `tier: normal` (degenerate — single tier, single executor).
- Shared session w/ `ffi-sync`. Critical section per FFI call ≈ <10 µs at 25 MHz Cortex-M3.
- 30 Hz timer (33 ms period) → ISR-disabled fraction ≪ 0.1%.
- ROS 2 control loop budget ≈ 1 ms typical → headroom ample.

---

## 8. Cross-language node state & cross-node context

### 8.1 Constraints

- Nodes may be written in **Rust, C, or C++** (nano-ros ships all three APIs).
- Each node ideally owns its state — codegen-friendly `register_X(executor)` pattern.
- Some workloads need cross-node state (e.g. autoware sentinel `SafetyIsland` shares MRM watchdog state across MRM handler + cmd_gate + validator). Splitting to per-node forces all sharing through topics, which is wrong for tightly-coupled in-binary state.
- All three languages must address the same shared state w/ identical layout and sync.

### 8.2 Model

**Per-node state** = each node crate owns `static NODE_STATE: OnceCell<RefCell<State>>` (Rust) or equivalent (C: `static node_state_t state;` w/ init guard; C++: header-only template static member, see Phase 89.13). Accessed only via the node's own register fn. Default. No cross-language concern.

**Cross-node shared state** = declared in system manifest (see §4.2 `[[shared_state]]` block), codegen emits a single `nros_shared_context` C-ABI struct + accessor functions. All three languages link against the same symbol table.

Codegen emits:

```c
// nros_shared_context.h (auto-generated)
typedef struct {
    float current_velocity;
    uint8_t mrm_state;
    bool has_external_control;
    uint64_t last_external_control_ms;
} SafetyIsland;

NROS_SHARED_API void nros_safety_island_get(SafetyIsland *out);
NROS_SHARED_API void nros_safety_island_set(const SafetyIsland *in);
NROS_SHARED_API void nros_safety_island_modify(void (*fn)(SafetyIsland *, void *), void *ctx);
```

```rust
// nros_shared_context.rs (auto-generated, mirrors C ABI)
#[repr(C)]
pub struct SafetyIsland { /* same layout */ }

pub fn safety_island_get() -> SafetyIsland { /* extern fn */ }
pub fn safety_island_set(v: &SafetyIsland) { /* extern fn */ }
pub fn safety_island_modify(f: impl FnOnce(&mut SafetyIsland)) { /* trampoline */ }
```

```cpp
// nros_shared_context.hpp (auto-generated)
namespace nros::shared {
    struct SafetyIsland { /* same layout, #[repr(C)] */ };
    SafetyIsland safety_island_get();
    void safety_island_set(const SafetyIsland&);
    template<typename F> void safety_island_modify(F&& f);  // wraps trampoline
}
```

### 8.3 Sync strategy — `tier_aware`

Codegen analyzes manifest tier graph + accessor declarations:

- **All accessors in same tier** → no lock. Single-task access. Equivalent to today's `RefCell` in autoware sentinel.
- **Accessors span tiers** → wrap in `nros-platform` mutex abstraction. On `MULTI_THREAD=1` platforms (POSIX/Zephyr/ESP-IDF) this resolves to a native mutex; on `MULTI_THREAD=0` (bare-metal/FreeRTOS/NuttX/ThreadX) it resolves to `critical_section::with()`. Same per-platform mapping table as §7.1.
- **Cross-tier `*_modify(fn)` mutator** chosen over separate get/set to avoid TOCTOU. Closure runs under lock.
- **Action item:** if no `nros-platform` mutex abstraction exists yet, Phase 94.D adds it (mirrors `PlatformYield` from Phase 79).

### 8.4 Discovery & symbol resolution

`nros_shared_context` symbols emitted by codegen into a generated crate (`nros_generated_context`). Each node crate declares dep in its `Cargo.toml` (Rust) or includes generated header (C/C++). Codegen also emits CMake variable + Cargo build script glue so 3rd-party packages can opt in via `find_package(NanoRosSharedContext)` or `nros-generated-context = { path = "..." }`.

### 8.5 Verification implications

- Static layout + static accessor set → Kani harness can model the shared-state machine exactly as today's hand-written `RefCell`.
- Single-tier degenerate case = identical IR to hand-written, identical proofs.
- Multi-tier: harness must consider preemption points around `*_modify` blocks. Existing nano-ros Phase 40 buffer-state-machine harness pattern applies.

### 8.6 Worked example — autoware sentinel migration

Hand-written sentinel `SafetyIsland` becomes manifest-declared `shared_state.safety_island`. All 11 algos remain in a single tier (default `normal`) → codegen emits single-task degenerate, no locks, behaviorally identical to today. If user later reassigns control_node to `tier: high`, codegen automatically inserts cross-tier sync around `safety_island_modify`.

Phase 94.B validation criterion: codegen output must produce a binary that passes the existing autoware-sentinel integration tests bit-for-bit-equivalent in behavior. Hand-written `main.rs` retained as reference + parity oracle until all sentinel tests green against generated output; then deprecated, then removed.

---

## 9. Runtime args (`ros2 launch arg:=val` parity on RTOS)

### 9.1 The problem

Build-time freeze (§2.1) means `IfCondition`, `LaunchConfiguration` substitutions, etc. are baked into the binary. Reconfiguring requires reflash. Unacceptable for field-deployed devices. Need RTOS-side mechanism to override launch args without rebuild.

### 9.2 Insight — split args by class

A launch arg can affect either:

- **Graph topology** — `IfCondition` deciding whether a node exists, `<include if=...>`, `node_namespace`, `topic_remap`. Changing these requires re-resolving the launch tree → re-codegen → reflash. **Cannot be runtime.**
- **Values** — node parameters, QoS depth, log levels, frame_id strings, control gains. These already flow through ROS 2 parameter services + nano-ros runtime parameter API. **Already runtime-mutable** post-boot via param services.

The middle ground: launch args that drive _parameter values_. Stock ROS 2 wires `LaunchConfiguration('ctrl_gain')` into a node's `parameters=[{'gain': LaunchConfiguration('ctrl_gain')}]`. Build-time freeze captures the literal default; runtime override is then a parameter override, **not** a launch-arg override.

### 9.3 Manifest declaration

Codegen needs to know which launch args are runtime-overridable. Declared in system manifest `[[launch_args]]` (see §4.2). Each entry has `class = "graph"` (baked) or `class = "parameter"` (runtime-mutable, requires `target_param`).

Codegen rejects build if:

- `class = "parameter"` arg drives a non-parameter location (e.g. drives `IfCondition`).
- `class = "graph"` arg is missing a value at build time.

### 9.4 Runtime override interface

No unified API across RTOSes. Layered:

- **C ABI declared** in `nros-c` (`include/nano_ros/runtime_args.h`).
- **Rust API** in `nros-node` re-exports the C ABI via the `BoardArgsSource` trait.
- **Per-platform default impl** in each `nros-platform-*` (or `zpico-platform-*`) crate.
- **Board-overridable** via the standard board crate pattern (CLAUDE.md "Board Crate Transport Features").

```c
// nros-c/include/nano_ros/runtime_args.h
typedef struct {
    const char *name;
    const char *value;  // string; node-side parses to typed param
} nros_runtime_arg_t;

// Implemented by the active nros-platform-* / zpico-platform-* crate.
// Called once, pre-spawn, by codegen-emitted main().
const nros_runtime_arg_t *nros_runtime_args_get(size_t *count);
```

Per-platform impl:

| Platform           | Source                                                                | Notes                                           |
| ------------------ | --------------------------------------------------------------------- | ----------------------------------------------- |
| POSIX (native dev) | `argv` parsing                                                        | `--ros-args -p ctrl_gain:=2.5` syntax preserved |
| Zephyr             | settings subsystem (`CONFIG_SETTINGS`, NVS or littlefs backend)       | Survives reboot                                 |
| FreeRTOS           | `BoardArgsSource` impl in board crate (NVRAM / flash / UART)          | Board-specific                                  |
| NuttX              | `BoardArgsSource` impl reading `/data/nros_args.txt` (FS abstraction) | Default impl in `nros-platform-nuttx`           |
| ThreadX            | `BoardArgsSource` impl over NetX BSP FS or board NVRAM                | Default impl in `nros-platform-threadx`         |
| Bare-metal         | `BoardArgsSource` trait; default = empty array                        | User implements per board                       |

Codegen emits boot-time wiring:

```rust
// generated main.rs (excerpt)
fn nros_main() -> ! {
    open_shared_session();
    let (args, count) = nros_runtime_args_get();
    apply_runtime_param_overrides(args, count);  // populates ParameterServer
    spawn_tier_tasks();
    join_or_idle();
}
```

`apply_runtime_param_overrides` writes into the same parameter store that ROS 2 `~/get_parameters` / `~/set_parameters` services read. So a runtime override has identical effect to a `set_parameter` request after boot.

### 9.5 Discovery + UX

- Build-time tooling generates `manifest_args_help.txt`: list of `class: parameter` args + their default + target param + override syntax for each platform.
- Companion web UI (in play_launch) can render this for the device.
- ROS 2 `ros2 param list` against a deployed nano-ros device shows the same params (already supported via Phase 86 lifecycle + parameter services).

### 9.6 Open subitems

- **Schema validation at runtime.** Should `apply_runtime_param_overrides` reject typed mismatches (string in `--gain:=foo` for an `f32` param)? Lean yes, log + ignore, never panic.
- **Atomicity.** All overrides applied before tier tasks spawn — no mid-run race.
- **Per-platform impl scope.** v1 ships POSIX + Zephyr (settings) + a board-crate trait `BoardArgsSource` w/ empty default. Other platforms get default empty until user supplies.

---

## 10. RTOS execution model mapping

Tier-task → RTOS task spawn rules. See `book/src/internals/scheduling-models.md` for full per-RTOS scheduling details.

### 10.1 Per-RTOS quick reference

| RTOS                  | Priority range                                                              | Direction          | Default scheduler                  | Stack unit                                          | Yield primitive                                                                      | Mutex w/ priority inheritance                                   |
| --------------------- | --------------------------------------------------------------------------- | ------------------ | ---------------------------------- | --------------------------------------------------- | ------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| **POSIX** (Linux)     | 1..99 (FIFO/RR)                                                             | Higher = higher    | SCHED_FIFO                         | bytes                                               | `sched_yield()`                                                                      | `pthread_mutex` w/ `PTHREAD_PRIO_INHERIT`                       |
| **Zephyr**            | `K_PRIO_COOP(N)`..`K_PRIO_PREEMPT(M)` (negative coop, non-negative preempt) | Lower = higher     | Preemptive                         | bytes (`K_THREAD_STACK_DEFINE`)                     | `k_yield()`                                                                          | `k_mutex` (built-in)                                            |
| **FreeRTOS**          | 0..configMAX_PRIORITIES-1                                                   | Higher = higher    | Preemptive FPP                     | words → bytes (multiplied by `sizeof(StackType_t)`) | `taskYIELD()` (macro; nano-ros substitutes `vTaskDelay(1)` until Phase 77.22 C shim) | `xSemaphoreCreateMutex` (recursive variant for inheritance)     |
| **ThreadX**           | 0..31                                                                       | **Lower = higher** | Preemptive FPP + preempt-threshold | bytes                                               | `tx_thread_relinquish()`                                                             | `tx_mutex_create(..., TX_INHERIT=1)` (default in nros)          |
| **NuttX**             | 1..255                                                                      | Higher = higher    | SCHED_FIFO (also RR, SPORADIC)     | bytes (POSIX `pthread_attr_setstacksize`)           | `sched_yield()`                                                                      | POSIX mutex w/ `PTHREAD_PRIO_INHERIT` or `PTHREAD_PRIO_PROTECT` |
| **RTIC** (bare-metal) | 0..NVIC_PRIO_BITS (typically 0..15)                                         | **Lower = higher** | Hardware NVIC, compile-time fixed  | static link section                                 | N/A (no thread context)                                                              | SRP — compile-time ceiling, zero runtime overhead               |

### 10.2 Tier → RTOS task spawn (codegen output sketch)

For each tier in the system manifest, codegen emits one task spawn matching the active `target_rtos`:

| Target   | Spawn call                                                                                                        | Body                                                                                                                                     |
| -------- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| POSIX    | `pthread_create` + `pthread_setschedparam(SCHED_FIFO, prio)`                                                      | `loop { exec.spin_once(N); sched_yield(); }`                                                                                             |
| Zephyr   | `K_THREAD_DEFINE(tid, stack_bytes, entry, ..., prio, 0, 0)`                                                       | `loop { exec.spin_once(N); k_yield(); }`                                                                                                 |
| FreeRTOS | `xTaskCreate(entry, "tier_X", stack_words, NULL, prio, NULL)`                                                     | `loop { exec.spin_once(N); vTaskDelay(1); }`                                                                                             |
| ThreadX  | `tx_thread_create(..., entry, ..., stack_ptr, stack_bytes, prio, preempt_thresh, 0, TX_AUTO_START)`               | `loop { exec.spin_once(N); tx_thread_relinquish(); }`                                                                                    |
| NuttX    | `task_create("tier_X", prio, stack_bytes, entry, NULL)` (or `pthread_create`) w/ `sched_setscheduler(SCHED_FIFO)` | `loop { exec.spin_once(N); sched_yield(); }`                                                                                             |
| RTIC     | `#[task(priority = N, binds = SomeIRQ)]` macro at compile time; codegen emits the macro invocation                | RTIC-managed; `exec.spin_once(0)` inside async task body w/ `Mono::delay().await` for QEMU I/O yield (per CLAUDE.md "QEMU I/O Yielding") |

### 10.3 Direction-flip handling

Nano-ros tier semantics: **higher tier name = more critical** (tiers `low` < `normal` < `high` < `critical`). Codegen translates to RTOS native direction:

- POSIX/FreeRTOS/NuttX: critical → high numeric.
- ThreadX/Zephyr-preempt/RTIC: critical → **low** numeric.

Manifest writer specifies the **per-RTOS numeric value directly** (no auto-flip), to keep the schema explicit and avoid hidden bugs from misinterpreting "higher = more important". Codegen just plugs the number into the spawn call. Validation: codegen warns if `tiers.critical.<rtos>.priority` is numerically lower (or higher, per RTOS) than `tiers.normal.<rtos>.priority` in a way that contradicts the symbolic tier order. Warning, not error — power users may have legitimate reasons.

### 10.4 Stack sizing

System manifest `tiers.<X>.<rtos>.stack_bytes` always in **bytes**. Codegen converts to RTOS-native unit:

- FreeRTOS: divides by `sizeof(StackType_t)` (4 on Cortex-M) for `xTaskCreate` word count.
- Zephyr: passes to `K_THREAD_STACK_DEFINE` directly.
- Others: bytes directly.

Per-tier-task minimum stack must hold: `Executor` frame + worst-case callback depth + zenoh-pico FFI shim + RTOS overhead. Suggested defaults in node manifests (set by author who knows callback complexity):

- Sensing/IO group: 4096 B
- Control loop group: 8192 B
- Logging/diagnostics group: 4096 B

System manifest can override per-deploy.

### 10.5 Multi-core

All current targets single-core. SMP support deferred. Manifest schema allows future `tiers.<X>.cpu_affinity` field without breaking existing manifests.

### 10.6 Mutex / cross-tier sync

§8.3 cross-tier shared-state lock uses a `nros-platform` mutex abstraction:

| Target            | Backing primitive                         | Priority inheritance                           |
| ----------------- | ----------------------------------------- | ---------------------------------------------- |
| POSIX             | `pthread_mutex` w/ `PTHREAD_PRIO_INHERIT` | Yes                                            |
| Zephyr            | `k_mutex`                                 | Yes (built-in)                                 |
| FreeRTOS          | `xSemaphoreCreateRecursiveMutex`          | Yes (recursive only)                           |
| ThreadX           | `tx_mutex_create(..., TX_INHERIT=1)`      | Yes                                            |
| NuttX             | `pthread_mutex` w/ `PTHREAD_PRIO_INHERIT` | Yes                                            |
| RTIC / bare-metal | `critical_section::with()`                | N/A — masks all interrupts; bounded ~µs window |

Codegen picks per `target_rtos` automatically. Action item: if `nros-platform` mutex abstraction does not exist, Phase 94.D adds it (mirrors `PlatformYield` from Phase 79).

### 10.7 Existing platform layer state (today)

| Crate                    | Status                                                                                                                                                                 |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nros-platform-freertos` | DEFAULT_PRIORITY=3, DEFAULT_STACK_DEPTH=5120 hardcoded; codegen needs config-toml-style override hook (Phase 76 already added cargo config.toml plumbing for FreeRTOS) |
| `nros-platform-threadx`  | No task creation in platform layer; board crate handles. Codegen emits board-style task creation                                                                       |
| `nros-platform-zephyr`   | Static `K_THREAD_STACK_ARRAY`; pool needs sizing for max tier count (compile-time configurable)                                                                        |
| `nros-platform-nuttx`    | Delegates to POSIX. Codegen calls POSIX path directly                                                                                                                  |
| `nros-platform-posix`    | Generic POSIX. No tuning needed                                                                                                                                        |
| RTIC                     | Per-task priority via `#[task(priority = N)]` macro — codegen emits the macro                                                                                          |

---

## 11. Reuse / extend / build new

Inventory of existing components vs work required. Goal: maximize reuse, identify true greenfield.

### 11.1 play_launch ecosystem (parser, manifest, support)

| Component                                                                                      | Status                                                                                                                                                                      | Decision                                                                                                                   |
| ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `play_launch_parser` `RecordJson`                                                              | **Stable**. Fields: `node[]`, `container[]`, `lifecycle_node[]`, `load_node[]`, `scopes[]` (tree), `variables` (resolved launch args). 260 tests. 100% Autoware compatible. | **Reuse as-is** — primary parser-side IR. No new parser needed.                                                            |
| `play_launch_parser` IR module (`#[cfg(feature = "ir")]` `LaunchProgram`/`Action`/`Condition`) | Built but not in CLI by default. Preserves unevaluated substitution chains.                                                                                                 | **Future use** — for hybrid runtime arg eval (post-v1).                                                                    |
| `ros-launch-manifest-types::Manifest`                                                          | Defines `args/nodes/topics/services/actions/includes/paths`. Rich `EndpointProps` (rate, jitter, freshness, drop budget). YAML-only today.                                  | **Extend** — add `tier`, `callback_groups`, `shared_state`, `target_rtos` fields. Convert to TOML or support both formats. |
| `ros-launch-manifest-check` 14 SMT-backed validation rules                                     | Z3 + petgraph. Validates QoS, rate, latency, drop budgets.                                                                                                                  | **Reuse** — extend rule set for tier/group invariants from §4.3.                                                           |
| `play_launch_wasm_codegen` + `wasm_runtime`                                                    | Compiles `LaunchProgram` → WASM, executes producing `RecordJson`.                                                                                                           | **Skip for v1.** Build-time freeze doesn't need WASM. May enable post-v1 hybrid runtime arg eval.                          |
| `play_launch_container` (C++)                                                                  | Observable composable node container w/ event publishing.                                                                                                                   | **N/A on RTOS.** Linux-only; codegen subsumes container concept via tier.                                                  |
| `play_launch_interception` (LD_PRELOAD)                                                        | rcl hook-based observability for monitoring.                                                                                                                                | **N/A on RTOS** (no LD_PRELOAD). Future: nano-ros could expose equivalent in-binary instrumentation.                       |
| `play_launch` CLI (`launch`, `dump`, `replay`, `check`)                                        | Top-level orchestrator + Web UI.                                                                                                                                            | **Add subcommand** `play_launch generate-rtos --target <rtos> <pkg> <launch>` invoking nano-ros codegen end-to-end.        |
| `spsc_shm`                                                                                     | Linux memfd ring buffer for interception events.                                                                                                                            | **Orthogonal.** N/A on RTOS.                                                                                               |

### 11.2 nano-ros build system

| Component                                                                                   | Status                                                                                 | Decision                                                                                                                                                 |
| ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo-nano-ros` clap CLI w/ subcommands (`generate-rust/c/cpp`, `bindgen`, `new`, `clean`) | Complete. Discovers workspace packages, parses `package.xml`.                          | **Extend** — add `generate-main` subcommand for orchestration codegen.                                                                                   |
| `cargo-nano-ros::package_discovery`                                                         | Walks workspace, parses Cargo.toml + package.xml.                                      | **Reuse** — extend to discover `nros.toml` files in same walk.                                                                                           |
| `rosidl-codegen` Askama template engine                                                     | Jinja-style w/ Rust expressions. Templates for Rust msg/srv/action, C, C++ headers.    | **Reuse** — write new templates: `main.rs.jinja`, `tier_task.rs.jinja`, `shared_context.h.jinja`, `shared_context.rs.jinja`, `shared_context.hpp.jinja`. |
| `rosidl-parser`                                                                             | `.msg/.srv/.action` lexer + parser.                                                    | **Reuse as-is** — orthogonal; orchestration codegen consumes its outputs but doesn't extend.                                                             |
| `nros-c/build.rs` size-probing pattern (Phase 87)                                           | `nros_sizes_build::find_dep_rlib` extracts `__NROS_SIZE_*` symbols → header constants. | **Reuse pattern** — add probes for per-tier executor sizing in generated main.                                                                           |
| `nros-cpp/build.rs` storage-derivation                                                      | Computes `CppContext` upper bound from `nros` rlib symbols.                            | **Reuse** — same pattern for shared-context size in generated header.                                                                                    |
| Zephyr CMake `nros_generate_interfaces()`                                                   | Discovers `.msg/.srv/.action`, invokes `nros-codegen`, links into Zephyr `app` target. | **Mirror as `nros_generate_orchestration()`** — new CMake fn invoking `cargo-nano-ros generate-main`.                                                    |
| Zephyr Kconfig (`CONFIG_NROS_*`)                                                            | Selects API (C/C++/Rust), RMW backend, zenoh-pico features.                            | **Extend** — add `CONFIG_NROS_ORCHESTRATION_TIERS_MAX` etc.                                                                                              |

### 11.3 nano-ros runtime API

| Component                                                                     | Status                                                                                                                                               | Decision                                                                                                                                                                                 |
| ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nros-node::Executor` (`open`, `create_node`, `spin_once`, etc.)              | Single-threaded polling, arena-backed. Phase 87 size headers.                                                                                        | **Extend** — add `Executor::open_with_session(shared_session)` for tier-tasks sharing one zenoh-pico session.                                                                            |
| `nros-node::Node::create_publisher/subscriber/timer/service/action_*`         | Complete typed API. Returns `Result<Handle, NodeError>`.                                                                                             | **Reuse** — codegen calls these directly inside generated `register_X()` fns.                                                                                                            |
| `nros-node::parameter_services` (Phase 86)                                    | 6 standard services (`get/set/list/describe/get_types/set_atomically`). `nros_params::ParameterServer` typed store. `MAX_PARAMS_PER_REQUEST=64`.     | **Reuse** — runtime arg overrides write into existing ParameterServer pre-spawn.                                                                                                         |
| `nros-node::lifecycle` (Phase 86, REP-2002)                                   | `LifecyclePollingNode` + `register_on_{configure,activate,deactivate,cleanup,shutdown,error}`. State machine.                                        | **Reuse** — codegen wires `startup_order` into `activate()` calls w/ ack-poll.                                                                                                           |
| `nros-platform-api::PlatformYield`                                            | Exists as trait. POSIX/Zephyr/FreeRTOS/NuttX/ThreadX impls.                                                                                          | **Reuse** — codegen calls in generated tier-task spin loops.                                                                                                                             |
| `nros-platform-api::PlatformThreading`                                        | Trait defines `task_init/task_join/mutex_init/mutex_lock/condvar_*`. Currently inherent impls per platform; Phase 84.F4 will migrate to trait impls. | **Reuse** — codegen calls `<P as PlatformThreading>::task_init` for tier spawn, `mutex_lock`/`unlock` for cross-tier sync. **Action:** confirm Phase 84.F4 ordering vs orchestration v1. |
| `zpico-platform-shim` FFI exports                                             | Auto-generated C wrappers calling Rust trait methods.                                                                                                | **Reuse** — no changes needed.                                                                                                                                                           |
| Board crate `run(config, closure)` pattern (`nros-board-mps2-an385-freertos`, etc.) | User passes closure; board does HW init + scheduler start.                                                                                           | **Reuse** — codegen-emitted `main` calls board's `run()` w/ generated closure (which spawns tier tasks).                                                                                 |
| `nros-rmw-zenoh::ffi-sync` (Phase 61)                                         | Wraps zenoh-pico FFI in `critical_section::with()` on MT=0 platforms. Bounded ~µs.                                                                   | **Reuse** — codegen enables `ffi-sync` feature flag on MT=0 platforms. No code change.                                                                                                   |
| Composable node container concept                                             | Doesn't exist yet on nano-ros.                                                                                                                       | **Build new** — Phase 94.F: tier ≡ container.                                                                                                                                            |
| Multi-worker `Executor` (Reentrant group)                                     | Doesn't exist. Single-task `spin_once` only.                                                                                                         | **Build new** — Phase 94.H, post-v1, MT=1 platforms only.                                                                                                                                |

### 11.4 Greenfield (must build)

Components with no existing infrastructure to reuse:

1. **`nros.toml` schema crate** (`nros-orchestration-manifest`, in play_launch workspace) — TOML serde types, validation, merger of node + system manifests. Mirrors `ros-launch-manifest-types` but for our extended schema. **~500 LOC est.**
2. **`cargo nano-ros generate-main` subcommand** — consumes `record.json` + workspace `nros.toml` files → emits orchestration crate w/ generated `main.rs`, `tier_task_*.rs`, `shared_context.{h,rs,hpp}`, plus build.rs probes. **~1500 LOC est.**
3. **Tier resolver** — symbolic tier name → per-RTOS numeric (validated against per-RTOS bounds), spin period bound check, callback-set partition by tier, group→tier index. **~300 LOC est.**
4. **Per-package `nros.toml` discoverer** — walks workspace, picks up node manifests, detects exactly-one system manifest per build target. Extends `cargo-nano-ros::package_discovery`. **~150 LOC est.**
5. **Shared-context emitter** — analyzes `[[shared_state]]` accessor sets, picks sync mode (no lock vs cross-tier mutex), emits `#[repr(C)]` struct + `_get/_set/_modify` C-ABI accessors in 3 languages. **~600 LOC est.**
6. **`BoardArgsSource` trait + per-platform impls** — POSIX argv parser (priority), Zephyr settings backend (priority), default empty for FreeRTOS/NuttX/ThreadX/bare-metal (board overrides). **~400 LOC est.**
7. **Runtime arg → ParameterServer wiring** — pre-spawn pass that reads `BoardArgsSource` and calls `ParameterServer::set` for each `class = "parameter"` arg. **~150 LOC est.**
8. **`PlatformMutex` (if not subsumed by `PlatformThreading`)** — confirm trait surface w/ Phase 84.F4 owner. Possibly zero new code if `PlatformThreading::mutex_*` suffices. **0–200 LOC.**
9. **Lifecycle activation orchestrator** — pre-spawn pass that walks `startup_order`, calls `activate()` per node, polls for `STATE_ACTIVE` w/ timeout. **~200 LOC est.**
10. **`Executor::open_with_session(shared)` API** — variant of `Executor::open` that accepts pre-opened session instead of opening one. **~150 LOC est.**
11. **`ros-launch-manifest-types` extension** — add tier/callback_group/shared_state fields. **~200 LOC est.** (or fork into separate crate to avoid Linux-side impact).

**Total greenfield estimate: ~4000 LOC** across nano-ros + play_launch (manifest crate extension).

---

## 12. References

- play_launch parser: `~/repos/play_launch/src/play_launch_parser/`
- ros-launch-manifest: `~/repos/play_launch/src/ros-launch-manifest/docs/launch-manifest.md`
- autoware-nano-ros sentinel: `~/repos/autoware-nano-ros/src/autoware_sentinel_linux/src/main.rs`
- nano-ros executor: `packages/core/nros-node/src/executor/`
- nano-ros `ffi-sync`: `packages/zpico/nros-rmw-zenoh/src/zpico.rs:38–45`, Phase 61 archive
- nano-ros `PlatformYield` (Phase 79 abstraction pattern, mirror for mutex): `packages/core/nros-rmw/src/traits.rs`
- nano-ros C++ template-static-member pattern (Phase 89.13 NuttX gotcha): `docs/roadmap/archived/phase-89.md` + memory `project_nuttx_cpp_pic_fix.md`
- nano-ros scheduling-models (per-RTOS priority/scheduler/mutex): `book/src/internals/scheduling-models.md`
- nano-ros executor fairness analysis (single-slot semantics, LET mode): `docs/reference/executor-fairness-analysis.md`
- Phase 76 (FreeRTOS scheduling configuration via cargo config.toml): `docs/roadmap/archived/phase-76*.md`
- Phase 77.22 (planned `PlatformYield` trait + FreeRTOS C shim for `taskYIELD`): see CLAUDE.md "Spin/Yield Wake Primitives"
- zenoh-pico MT model: https://github.com/eclipse-zenoh/zenoh-pico `CMakeLists.txt:255`, `include/zenoh-pico/session/utils.h`
- rmw_zenoh single-session-per-context: https://github.com/ros2/rmw_zenoh `rmw_zenoh_cpp/src/detail/rmw_context_impl_s.cpp`
- Apex.OS cbg_executor: https://github.com/ros2/examples/tree/rolling/rclcpp/executors/cbg_executor
- ros-realtime reference: https://github.com/ros-realtime/reference-system
- REP-2014 callback-group priority: https://github.com/ros-infrastructure/rep PR #348 (or successor)
- REP-2009 realtime executor: https://ros.org/reps/rep-2009.html
- rclc executor: https://micro.ros.org/docs/concepts/client_library/execution_management/
