---
rfc: 0004
title: "Configuration model: language-agnostic config across single-node and workspace"
status: Stable
since: 2026-05
last-reviewed: 2026-06
implements-tracked-by: [phase-212, phase-227]
supersedes: []
superseded-by: null
---

# Configuration model: language-agnostic config across single-node and workspace

> **Revised 2026-06 to the unified Phase 212 model.** The earlier Phase 172.K
> shape — a single `nros.toml` whose *kind* (workspace root / `[component]` /
> direct-mode `[node]`) was discriminated by which sections were present, with
> Cargo-style walk-up resolution — is **superseded**. A workspace-root
> `nros.toml` is now **rejected by the CLI** (`NrosTomlNotSupported`; run
> `nros migrate workspace`). Workspace/system config lives in `Cargo.toml`
> metadata + a language-agnostic `system.toml`; `nros.toml` survives only as the
> optional embedded *direct-mode runtime* file (the transport schema below).
> RMW selection has its own home — see RFC-0031.

## 1. Unifying principle

nano-ros config is **language-agnostic**: `system.toml` + `package.xml` + launch
XML are the source of truth. Per-language manifests (Cargo `[package.metadata.nros.*]`,
CMake `nano_ros_*` functions) are **native-idiom projections** of the same data.
The toolchain **lowers** declared config to each language's build mechanics. A
user declares config once, in the surface natural to their language; they never
hand-maintain the lowered form (a cargo feature, a `-D` flag).

## 2. `system.toml` — universal, single-node optional

`system.toml` is the nano-ros **system descriptor** at every scale, with one
schema for Rust, C, and C++:

- **Workspace (multi-node):** required, in the bringup package. Owns the
  component topology, deploy targets, domain, and RMW.
- **Single-node:** optional. Absent → the toolchain **synthesizes an implicit
  1-component system** from the node's manifest + defaults. Present → declares
  rmw / domain / deploy in the same language-agnostic place. (Implicit-system
  synthesis + the single-node read path are tracked by phase-227.)

This gives **one declarative home that scales down**, and makes C/C++ single-node
config symmetric with Rust (both read the same `system.toml`) without forcing a
file on the trivial "hello world" case.

## 3. Config homes by language × scale

`nros.toml` = embedded **direct-mode runtime file only** (§6). A workspace-root
`nros.toml` is rejected. `config.toml` (`[network]`/`[zenoh]`/`[scheduling]`) is
retired (§8).

| | Single-node | Workspace |
|---|---|---|
| **Rust** | `Cargo.toml [package.metadata.nros.application]` (deploy) + `nros::main!`; optional `system.toml` to pin rmw/domain | root `[workspace.metadata.nros]` + node `[..nros.node]` + entry `[..nros.entry]` + bringup `system.toml` |
| **C / C++** | `CMakeLists.txt` (`nano_ros_application(...)`, `NANO_ROS_PLATFORM/RMW`) + `package.xml`; optional `system.toml` | `nano_ros_node_register` / `nano_ros_entry` per pkg + **same `system.toml`** + `package.xml` |

Ownership:

| Concern | Owner |
|---|---|
| workspace members, default system | `[workspace.metadata.nros]` (root `Cargo.toml`) |
| node identity (class, name, namespace) | `[package.metadata.nros.node]` / `nano_ros_node_register(...)` |
| entry / boot / deploy target | `[package.metadata.nros.entry]` (+ `[..deploy.<t>]`) / `nano_ros_entry(...)` |
| system topology, components, launches, deploy, **rmw**, **domain** | `system.toml` (bringup pkg; or optional single-node) |
| embedded direct-mode runtime (transports, RT) | `nros.toml` (board parses at boot) |
| ROS identity + msg `<depend>` (codegen) | `package.xml` (both languages, both scales) |

### 3.1 Single source of truth — no cross-file overlay merge

Each concern has **exactly one home** (the table above). nano-ros is **SSoT-per-concern,
not an overlay system**: a value is never silently *merged* across several files for the same
concern. To know a system, read its `system.toml`; to know a node, read its package metadata.
This is deliberate — an overlay system (config blended from N files, last-wins) is
**action-at-a-distance**: a value set in some package's file changes the build with no local
sign, and "what is the effective config?" needs N files reconciled in your head.

**Resolution, where a concern has both a native-idiom projection and a `system.toml`,** is a
**fixed, short precedence ladder** (not an open merge): explicit CLI/build flag
(`--rmw` / `-DNANO_ROS_*`) > `system.toml` (`[deploy.<t>]` > `[system]`) > the per-package
native projection (`[package.metadata.nros.*]` / CMake) > built-in default. One ladder, each
rung a known file — auditable, unlike an arbitrary-file overlay.

**Legacy — the Phase-172 per-package `nros.toml` build/capability overlay.** The orchestration
planner historically *also* read a per-package `nros.toml` as a build overlay
(`[build]`/`[safety]`/`[param_services]`/`[lifecycle]`/`[param_persistence]`/`[[transport]]`/`[[shared_state]]`).
That **contradicts this section + the table** (where `nros.toml` is the embedded-runtime file
only) and is exactly the action-at-a-distance hazard above. It is **deprecated and being
retired**: phase-254 moved `[safety]`/`[param_services]` to typed `system.toml` (the others
follow — issue 0076 §A; RMW in phase-255). During retirement the overlay is a **fallback that
warns**; after it, the *same-name* collision is gone — `nros.toml` serves only its §6
embedded-runtime role.

**Auditability (issue 0076).** Two guards make the SSoT legible:
- `nros config show` — prints the **resolved effective config** for a system + **per-value
  provenance** (which file each value came from). The plan's `trace` records file-level source
  today; this surfaces it per value.
- `nros check` — **flags any value still sourced from a legacy `nros.toml` overlay** (with a
  removal date), so the action-at-a-distance is visible before it bites.

## 4. `system.toml` schema

```toml
[system]
name      = "demo"
rmw       = "zenoh"          # default backend for this system (see RFC-0031)
domain_id = 0
default_launch = "system.launch.xml"

[[component]]
pkg   = "talker_pkg"
class = "talker_pkg::Talker"
name  = "talker"

[deploy.native]
kind   = "self"
target = "x86_64-unknown-linux-gnu"
# rmw  = "cyclonedds"        # optional per-deploy override of [system].rmw

# Declared capability axes (RFC-0031 §Generalization; phase-250/252/254). Typed,
# system-wide, optional. Lower to build features (and, for the C/C++ bake, a
# `#define`) — declared ONCE here, read by BOTH codegen paths.
[safety]                     # E2E message-integrity (CRC + seq gap/dup), zenoh
enabled = true               # optional, default true
crc     = true               # optional, default true
[param_services]             # the external ROS 2 parameter server
enabled = true               # optional, default true
```

Launch files (`launch/*.launch.xml`) use the ROS 2 launch XML schema verbatim
(stock nav2/Autoware syntax) and are resolved at build time. Multi-node
RT/scheduling exposure in `system.toml` is **not yet designed** (open; tracked
by phase-227 / Phase 212.M).

**One SSoT, both codegen paths (phase-254).** `system.toml` is read by BOTH the Rust
orchestration (`planner` → `NrosPlan` → `generate`) and the C/C++ bake
(`codegen_system` → `system_config.h`). The capability axes above are typed here so
`deny_unknown_fields` accepts them and the bake sees them — a declared `[safety]`
yields both the Rust `nros/safety-e2e` lowering AND `#define NROS_SYSTEM_SAFETY_E2E`
for C/C++. The legacy per-package `nros.toml` **capability-overlay** read (Phase-172)
is retired by phase-254: `nros.toml` is the embedded direct-mode runtime file only (§5),
not a build-capability overlay.

The same single-source rule applies to **RMW** (phase-255, **landed**): `[system].rmw` +
`[deploy.<t>].rmw` is the one declared home — both the Rust board-feature lowering and the
C/C++ `#define NROS_SYSTEM_RMW_<TOKEN>` resolve from it via `SystemToml::resolved_rmw(target,
cli)` (RFC-0031 precedence `--rmw` > `[deploy.<t>]` > `[system]` > `zenoh`; `--rmw` is on `nros
plan` + `nros codegen-system`). The legacy `[build].rmw` / `[[transport]].rmw` `nros.toml`
overlay is now a **deprecated fallback that warns** (no fixture declares it), retired after the
next release; a binary's multi-RMW link set comes from `[[bridge]]` here (`bridged_rmws()` →
`PlanBuildOptions::bridged_rmws` → `rmw_set`), not the overlay.

## 5. `nros.toml` — embedded direct-mode runtime config

A hand-written single-node embedded app reads its `nros.toml` via the board
`Config::from_toml` (compile-baked with `include_str!` on embedded;
filesystem/env on hosted). No launch file, no planner, no generated `main()` —
the `examples/**` copy-out templates use this ("boilerplate IS lesson"). It
carries `[node]`, `[[transport]]`, and `[node.rt]` only. It is **not** a
workspace manifest and **not** read by `nros plan`/`check`/`codegen-system`.

## 6. Transports — top-level, decoupled, `id`-addressable

A **transport** is a physical link + the RMW session that rides it, declared at
top level in `nros.toml`, independent of nodes:

```toml
[[transport]]
id      = "eth"            # optional; defaults to `rmw` when each rmw is unique
kind    = "ethernet"       # ethernet | wifi | serial | can
ip      = "10.0.2.50/24"   # CIDR carries the prefix
mac     = "02:00:00:00:00:01"
gateway = "10.0.2.2"
rmw     = "zenoh"
locator = "tcp/10.0.2.2:7447"
# interfaces = ["eth0", "eth1"]  # multi-homing: ONE session spans both NICs

[[transport]]
id       = "bus"
kind     = "serial"
device   = "UART0"
baudrate = 115200
rmw      = "cyclonedds"
```

Per-kind field rules (validated by `PlanBuildOptions::validate_transports`):

| kind | fields |
|------|--------|
| `ethernet` | `ip` (CIDR), `mac`, `gateway`, `interface`/`interfaces` |
| `wifi` | `ip` (optional/static), `ssid`, `password`, `interface`/`interfaces` |
| `serial` / `can` | `device`, `baudrate` |
| all | `id`, `rmw`, `locator` |

The `id` makes a transport first-class and addressable, and disambiguates two
transports that share an `rmw`. `interfaces` (a list) multi-homes one session
over several NICs (one merged discovery graph) — distinct from two `[[transport]]`
entries (two *separate* sessions).

### Two axes: interfaces-per-transport × transports-per-rmw

| Case | transports | rmw | interfaces / transport | node binding |
|------|-----------|-----|------------------------|--------------|
| **A. cross-RMW bridge** | N | **distinct** per node | 1 each | by `rmw` |
| **B. single node, multi-homed** | 1 | one | **list** | implicit |
| **C. cross-RMW bridge, multi-homed** | N | distinct | **list** each | by `rmw` |
| **D. segregated same-rmw** | N | **same** | 1+ each, not merged | by `id` (K.5 runtime) |

A–C bind by `rmw`; only D (two separate sessions of the same backend,
intentionally not merged) needs `create_node_on`-by-`id` (Phase 172.K.5).
Multi-homing maps per backend (zenoh → listen/connect per NIC + scouting
interface; Cyclone → `<Interfaces>`; Fast DDS → whitelist).

### Runtime mapping & binding

Each `[[transport]]` becomes one `SessionSpec { rmw, locator, domain_id, … }`;
the executor opens them with `Executor::open_multi([specs])`. A node binds to
exactly one transport: 0 transports → board default + the single linked RMW
(`Executor::open`); 1 → implicit; N → each node names its transport (default =
the `default = true` one). In-process multi-RMW is the explicit `[[bridge]]`
path (RFC-0009).

## 7. Scheduling / RT — `[node.rt]`

Scheduling is a node-level block in `nros.toml` (it replaced `config.toml
[scheduling]`):

```toml
[node.rt]
app_priority = 12;  app_stack_bytes = 262144
zenoh_read_priority = 16;  zenoh_read_stack_bytes = 5120
zenoh_lease_priority = 16; zenoh_lease_stack_bytes = 5120
poll_priority = 16; poll_interval_ms = 5
```

In planned mode this maps to the `SchedContextConfig` the planner carries
(RFC-0015/0016). **Multi-node RT home (decided 2026-06):** the *node* declares its
callback groups (`[package.metadata.nros.node]` / `nano_ros_node_register`);
`system.toml` owns tier definitions + group→tier assignment
(`[tiers.<name>.<rtos>]` priority/stack + `[[node_overrides]]` deploy-time tier reassignment) and
`[[shared_state]]`. See RFC-0015 (Phase 212 reconciliation). Schema/loader is
tracked by phase-227; the per-tier codegen by phase-228.

## 8. RMW selection & retired files

RMW backend selection (declared in `system.toml` / deploy override / flag, then
lowered to a cargo feature or `-DNANO_ROS_RMW`, per-deploy scope) is owned by
**RFC-0031**, not this RFC.

`config.toml` (`[network]`/`[zenoh]`/`[scheduling]`) is **retired** (Phase
172.K.6); its fields moved to `nros.toml` (`[node]` / `[[transport]]` /
`[node.rt]`).

## 9. Gaps (tracked by phase-227)

- Implicit single-node `system.toml` synthesis + the optional single-node read path.
- `nano_ros_application()` CMake function for C/C++ single-node parity.
- Per-component RT/scheduling exposure in multi-node `system.toml`.
- Book sync: `user-guide/configuration.md` still documents the Phase 172.K model.

## See also

- RFC-0031 (RMW selection & lowering), RFC-0024/0025 (multi-node workspace layout).
- `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md` — the file-consolidation work.
- `book/src/reference/nros-bridge-toml.md` — the separate runtime topic-forward bridge config.

## Changelog

- 2026-06 — Revised to the unified Phase 212 model: workspace-root `nros.toml`
  rejected; config homes are Cargo/CMake metadata + universal-optional
  `system.toml`; `nros.toml` narrowed to embedded direct-mode runtime; RMW
  selection split out to RFC-0031. Transport/binding/RT schema retained.
- 2026-05 — Phase 172.K manifest model (single `nros.toml`, section-discriminated
  kinds, Cargo-style walk-up). Superseded.
