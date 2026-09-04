---
rfc: 0004
title: "Configuration model: language-agnostic config across single-node and workspace"
status: Stable
since: 2026-05
last-reviewed: 2026-09
implements-tracked-by: [phase-212, phase-227]
supersedes: []
superseded-by: null
---

# Configuration model: language-agnostic config across single-node and workspace

> **Canonical path (phase-296 R3/R4).** The play_launch-resolved
> **SystemModel** (RFC-0050 consumer half, RFC-0052 RTOS mapper) is now the
> canonical configuration input: `play_launch resolve … -o system_model.yaml`,
> then `--model` / `MODEL` / `nros::main!(model = …)`. The `system.toml` +
> launch-XML bake path this RFC describes is **transitional** — deprecated in
> phase-296 R3 (emits `warning[deprecated]`, silence with
> `NROS_ALLOW_LEGACY_BAKE=1`) and removed in R4 once the example/consumer
> ecosystem has migrated. `system.toml` survives as the integrator's
> *authoring* input that `play_launch resolve` consumes; nothing downstream
> parses it directly on the canonical path.

> **Revised 2026-06 to the unified Phase 212 model.** The earlier Phase 172.K
> shape — a single `nros.toml` whose *kind* (workspace root / `[component]` /
> direct-mode `[node]`) was discriminated by which sections were present, with
> Cargo-style walk-up resolution — is **superseded**. A workspace-root
> `nros.toml` is now **rejected by the CLI** (`NrosTomlNotSupported`; migrate
> via the `nros-v0.5.0` tag's `nros migrate workspace` — the one-shot verb is
> retired on newer trees, #186). Workspace/system config lives in `Cargo.toml`
> metadata + a language-agnostic `system.toml`. RMW selection has its own home —
> see RFC-0031.
>
> **Grounded-reality update (2026-06).** A sweep of `examples/**` found **0
> `nros.toml` files** — the workspace-root / overlay `nros.toml` is legacy in full
> and retired (phase-256). The bulk of embedded net/RT config lives in
> `[package.metadata.nros.deploy.<t>]` (`DeployOverlay`) + board features + Kconfig,
> baked by the `nros::main!()` codegen pipeline. See §3 "Config in practice".
>
> **Standalone `config.toml` is a SUPPORTED file path (2026-06, issue 0081 wontfix).**
> Hand-written `no_std` embedded apps/fixtures that bypass the `nros::main!()` /
> `DeployOverlay` codegen read a dedicated `config.toml` (`[node]` / `[[transport]]` /
> `[node.rt]`) at compile time via `include_str!("../config.toml")` →
> `Config::from_toml`. This keeps board net config **in a file, not hardcoded in
> Rust** — the maintainer's standing principle. The `logging-smoke-*` fixtures use it.
> Only the OLD `config.toml [network]/[zenoh]/[scheduling]` schema is retired (§8).

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

A workspace-root `nros.toml` is rejected. `config.toml`
(`[network]`/`[zenoh]`/`[scheduling]`) is retired (§8). The per-package `nros.toml`
file is **legacy in full** — both its Phase-172 build-overlay misuse (§3.1) and its
intended §5 embedded-runtime role are unused in practice (see "Config in practice"
below); it is slated for complete retirement (phase-256).

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
| **embedded runtime net/RT (single-node)** | **`[package.metadata.nros.deploy.<t>]` → `DeployOverlay`** (codegen apps, Rust) / `nano_ros_deploy(...)` (C/C++) + board-crate features + Kconfig; **or a standalone `config.toml` → `Config::from_toml`** (hand-written `no_std` apps that bypass codegen — §5) |
| ROS identity + msg `<depend>` (codegen) | `package.xml` (both languages, both scales) |

### Config in practice (verified 2026-06)

A sweep of `examples/**` grounds the table above and corrects the §5-§7 schemas
below (which describe an `nros.toml` home that **never materialized**):

- **`config.toml`: 0 *example* files, but a SUPPORTED file path.** The old
  `[network]/[zenoh]/[scheduling]` schema is retired (§8); the direct-mode
  `[node]`/`[[transport]]`/`[node.rt]` `config.toml` read via `Config::from_toml`
  (`include_str!`) is a kept standalone-file home for hand-written `no_std` apps that
  bypass the codegen pipeline (§5). The `logging-smoke-{esp32-qemu,freertos-mps2,
  threadx-riscv64}` test fixtures ship one. (The `.cargo/config.toml` files in
  examples are Cargo's own, unrelated.)
- **`nros.toml`: 0 files, retired.** No example declares one — not as overlay, not as
  embedded-runtime. Its `[node]`/`[[transport]]`/`[node.rt]` direct-mode schema did
  land — but as a standalone **`config.toml`** (above) parsed by the same
  `Config::from_toml`, not as an `nros.toml` file.
- **Embedded net/RT config lives in `[package.metadata.nros.deploy.<t>]`.** E.g.
  `examples/qemu-arm-baremetal/rust/talker-rtic` declares `locator`/`ip`/`gateway`/`netmask` there;
  `nros::main!()` bakes them into a `DeployOverlay` that `BoardEntry::run_with_deploy`
  applies onto the board boot `Config`. RT/stack/heap come from board-crate Cargo
  features + (Zephyr) `prj*.conf` Kconfig. There is **no `[[transport]]` file block**
  in any example — the per-app physical link is a set of `deploy` fields.

The implication: **transport/network is part of the `deploy` class, not its own
file surface**; the `[[transport]]` schema (§6) survives only as the design for
*explicit multi-session / cross-RMW topology* (still read by the planner overlay +
`validate_transports`), not as the embedded single-app net home.

### 3.1 Single source of truth — no cross-file overlay merge

Each concern has **exactly one home** (the table above). nano-ros is **SSoT-per-concern,
not an overlay system**: a value is never silently *merged* across several files for the same
concern. To know a system, read its `system.toml`; to know a node, read its package metadata.
This is deliberate — an overlay system (config blended from N files, last-wins) is
**action-at-a-distance**: a value set in some package's file changes the build with no local
sign, and "what is the effective config?" needs N files reconciled in your head.

**Resolution, where a concern has both a native-idiom projection and a `system.toml`,** is a
**fixed, short precedence ladder** (not an open merge): explicit CLI/build flag
(`--rmw` / `-DNANO_ROS_*`) > `system.toml` (`[image.<id>]` > `[system]`) > the per-package
native projection (`[package.metadata.nros.*]` / CMake) > built-in default. One ladder, each
rung a known file — auditable, unlike an arbitrary-file overlay.

**Legacy — the Phase-172 per-package `nros.toml` build/capability overlay.** The orchestration
planner historically *also* read a per-package `nros.toml` as a build overlay
(`[build]`/`[safety]`/`[param_services]`/`[lifecycle]`/`[param_persistence]`/`[[transport]]`/`[[shared_state]]`).
That **contradicts this section + the table** and is exactly the action-at-a-distance hazard
above. It is **deprecated and being retired**: phase-254 moved `[safety]`/`[param_services]` to
typed `system.toml`, phase-255 RMW, phase-256 `[lifecycle]`/`[param_persistence]` (the rest
follow — issue 0076 §A). During retirement the overlay is a **fallback that warns** (surfaced by
`nros check`); after it, the *same-name* collision is gone.

**`nros.toml` is then empty of any role** — the §5 embedded-runtime job it was *supposed* to keep
also never landed (no example ships one; embedded net/RT is `[package.metadata.nros.deploy.<t>]`
+ board features + Kconfig — see "Config in practice"). So phase-256 retires the **file**, not
just the overlay blocks: there is no surviving role to preserve.

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

[image.native]
board = "native"
# rmw = "cyclonedds"         # optional per-image override of [system].rmw

[host.native]                # WHERE the nodes run; no `nodes` means all of them

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
`[image.<id>].rmw` is the one declared home — both the Rust board-feature lowering and the
C/C++ `#define NROS_SYSTEM_RMW_<TOKEN>` resolve from it via `SystemToml::resolved_rmw(target,
cli)` (RFC-0031 precedence `--rmw` > `[image.<id>]` > `[system]` > `zenoh`; `--rmw` is on `nros
plan` + `nros codegen-system`). The legacy `[build].rmw` / `[[transport]].rmw` `nros.toml`
overlay is now a **deprecated fallback that warns** (no fixture declares it), retired after the
next release; a binary's multi-RMW link set comes from `[[bridge]]` here (`bridged_rmws()` →
`PlanBuildOptions::bridged_rmws` → `rmw_set`), not the overlay.

## 5. Standalone `config.toml` — embedded direct-mode runtime config *(supported, file-based)*

> **Status (2026-06): supported as a file, not via `nros.toml`.** This section
> describes the embedded home for **hand-written `no_std` single-node apps that bypass
> the `nros::main!()` codegen** — a `config.toml` read at compile time via
> `Config::from_toml(include_str!("../config.toml"))`, carrying `[node]` /
> `[[transport]]` / `[node.rt]`. The original design filed this under `nros.toml`;
> **`nros.toml` never materialized (0 files) and is retired**, but the direct-mode
> schema itself is alive — parsed from a standalone **`config.toml`** by every board
> crate's `Config::from_toml`. This keeps net config **in a file, not hardcoded in
> Rust builder calls** (maintainer principle; issue 0081 wontfix). The
> `logging-smoke-*` test fixtures use exactly this path.
>
> For apps that DO go through `nros::main!()`, the codegen route is preferred:
> `[package.metadata.nros.deploy.<t>]` → `DeployOverlay` (applied via
> `BoardEntry::run_with_deploy`) for net config, board-crate Cargo features + Kconfig
> for RT/stack. Both file homes are supported; pick by whether the app uses codegen.

The deploy-overlay shape that replaced it (the real embedded-runtime home):

```toml
# <app>/Cargo.toml — single-node embedded app
[package.metadata.nros.entry]
deploy = "rtic-mps2-an385"

[package.metadata.nros.deploy.rtic-mps2-an385]   # net config → DeployOverlay → board Config
locator = "tcp/192.168.1.1:7447"
ip      = "192.168.1.10"
gateway = "192.168.1.1"
netmask = "255.255.255.0"
```

## 6. Transports — the multi-session / cross-RMW topology schema

> **Scope (2026-06).** This schema is the design for **explicit multi-session and
> cross-RMW topology** (cases A-D below) — read by the planner overlay +
> `PlanBuildOptions::validate_transports`. It is **not** the embedded single-app net
> home: that is the `deploy` class (§3, "Config in practice"), where one app's
> physical link is a flat set of `[..deploy.<t>]` fields (`ip`/`gateway`/`netmask`/
> `locator`), not a `[[transport]]` file block (no example declares one). A future
> tidy folds the genuinely-needed multi-session topology under `system.toml`
> (alongside `[[domain]]`/`[[bridge]]`); the `nros.toml` `[[transport]]` file is part
> of the retired surface.

A **transport** is a physical link + the RMW session that rides it (historically
declared at top level in `nros.toml`), independent of nodes:

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
transports that share an `rmw`.

> **AMENDED 2026-09-04 — `interface` / `interfaces` are RETRACTED.** See the
> amendment below. The rest of this section stands; those two fields do not.

*Retracted text:* `interfaces` (a list) multi-homes one session
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

> **AMENDED 2026-09-04.** Cases **B** and **C** are retracted along with the
> `interfaces` field; **A** and **D** stand unchanged. The per-backend mapping
> sentence below is retracted in full — see the amendment.

*Retracted text:* Multi-homing maps per backend (zenoh → listen/connect per NIC + scouting
interface; Cyclone → `<Interfaces>`; Fast DDS → whitelist).

## Amendment (2026-09-04) — the middleware's configuration is the middleware's own

**Retracts:** the `interface` / `interfaces` transport fields, cases **B** and
**C** of the two-axes table, and the per-backend multi-homing mapping.
**Tracked by:** [phase-206](../roadmap/phase-206-multi-homing-transport-interfaces.md).
**Sibling reading:** RFC-0087 (a package is a `package.xml`) applies the same
"do not invent a second vocabulary" move to provider identity.

### The principle

> **nano-ros transports the user's backend config VERBATIM; it does not
> re-model it.** The board brings the devices up before ROS exists; the
> middleware is configured in the middleware's own language, parsed by the
> middleware's own parser.

This is what ROS 2 does, and it is why a ROS 2 user never writes an interface
list into a node: they set `CYCLONEDDS_URI`, or hand zenoh a config, naming an
IP or a device. The application code knows nothing about NICs.

### Why the retracted design could not work

`interfaces = ["eth0", "eth1"]` requires nano-ros to own a second vocabulary for
NICs, and there is no vocabulary that spans the targets:

| platform | how a NIC is named |
| --- | --- |
| POSIX | `getifaddrs` / `ifa_name` — real names, resolved at run time by the HOST |
| Zephyr | `net_if_get_default()` — **cannot be named at all** |
| smoltcp | exactly one `Interface`, **no names** |
| lwIP / FreeRTOS / esp-idf / ThreadX | the platform `iface` argument is accepted and ignored; the ports say so in their own source |

A portable declaration naming `eth0` is issue 0623's shape one layer up — a
value authored in one vocabulary and resolved in another, with nothing checking
they agree. Every fix for that needs a resolver, a gate, and a per-platform
story only Linux can satisfy.

### The feature already exists, in the backend

Measured 2026-09-04 in the CycloneDDS we link (`third-party/dds/cyclonedds`,
0.10.5, `src/core/ddsi/include/dds/ddsi/ddsi_cfgelems.h:79-88`):

```c
GROUP("NetworkInterface", NULL, network_interface_attributes, INT_MAX,
  MEMBER(network_interfaces), ...
```

`INT_MAX` multiplicity — an unbounded `<NetworkInterface>` list, each with
`name` / `address` / `autodetermine` / `priority`, and the group's own
description: *"Multiple interfaces can be specified with an assigned priority.
The list in use will be sorted by priority."* Multi-homing is therefore not a
nano-ros feature at all; it is an element in a config the user writes.

### What each backend's "own language" IS — and it differs

The verbatim unit is **not the same shape for both backends**, because the
backends are not the same shape. This is the part most likely to be
mis-generalised, so it is written down:

| backend | native config surface | how nano-ros carries it |
| --- | --- | --- |
| **CycloneDDS** | an XML **document**, and `dds_create_domain` parses inline XML itself (`ddsrt_xmlp_new_string`) | hand the file's bytes over as one comma token; **nano-ros parses no XML** |
| **zenoh-pico** | **no document format exists** — the API is `z_config_default` / `zp_config_insert(config, uint8_t key, const char *value)` over ~23 numbered keys | a flat `key = value` list whose key NAMES are upstream's; each pair goes to `zp_config_insert` |

**Cyclone composes; nano-ros must stop replacing.** `dds_create_domain` accepts
a comma-separated list of sources and merges every token into one `cfgst`
(`ddsi_config.c:2538-2570`). nano-ros selects exactly one with a three-way
ternary, so a user config **silently discards the baked baseline** — including
`<Threads>` stack sizes that are load-bearing on FreeRTOS and ThreadX.

**zenoh-pico's lack of a config file is a position, not a gap.** Upstream's
manual defines three categories and names run-time options — `zp_config_insert`
— as *"the primary configuration method"*. JSON5/YAML config files are
documented for **`zenohd`**, the router; the zenoh-pico manual does not mention
a file format at all. **nano-ros must not add a JSON5 parser**: that would mean
owning a parser for a format its own backend does not speak, which is this
amendment's mistake wearing a friendlier hat.

### Consequences for this RFC's model

* `[[transport]]` keeps `id`, `rmw`, `locator`, and the device fields that name
  hardware (`device`, `baudrate`, `ip`, `mac`, `gateway`, `ssid`, `password`).
  It does **not** grow transport tuning: `system.toml` says what the system
  IS — nodes, topics, capabilities — and the middleware's tuning lives in the
  middleware's own config. That is the ROS 2 split.
* `Executor::open_multi([SessionSpec…])` stays exactly as it is: one session per
  spec, **segregate**, which is phase-172 K.5. There is no "merge" primitive to
  build, because merging NICs is something Cyclone does inside one participant
  when its config says so.
* `rmw_session_options_t` does not grow a list field. It is
  `{u8 localhost_only; u8 _reserved[7]; const char *enclave;}` and nothing in
  the tree passes it non-NULL; config does not need it.

### Runtime mapping & binding

Each `[[transport]]` becomes one `SessionSpec { rmw, locator, domain_id, … }`;
the executor opens them with `Executor::open_multi([specs])`. A node binds to
exactly one transport: 0 transports → board default + the single linked RMW
(`Executor::open`); 1 → implicit; N → each node names its transport (default =
the `default = true` one). In-process multi-RMW is the explicit `[[bridge]]`
path (RFC-0009).

## 7. Scheduling / RT — `[node.rt]` *(nros.toml form unused)*

> **In practice (2026-06).** Single-node RT/stack/priority is set via **board-crate
> Cargo features + Kconfig** (`prj*.conf` on Zephyr), not an `nros.toml [node.rt]`
> block (0 examples). Multi-node RT is `system.toml` `[tiers.<name>.<rtos>]` +
> `[[node_overrides]]` (below). The `[node.rt]` schema here is retained as the
> conceptual node-RT model; the `nros.toml` file carrying it is part of the retired
> surface.

The intended node-level block (it replaced `config.toml [scheduling]`):

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

The **old** `config.toml` schema (`[network]`/`[zenoh]`/`[scheduling]`) is **retired**
(Phase 172.K.6) — superseded by the `deploy` class (net) + board features / Kconfig
(RT). The `nros config show --config <path>` legacy reader for it is scrubbed.

The **direct-mode** `config.toml` (`[node]`/`[[transport]]`/`[node.rt]`, parsed by
`Config::from_toml`) is **NOT retired** — it is the supported standalone-file home for
hand-written `no_std` apps that bypass the `nros::main!()` codegen (§5; issue 0081
wontfix). 0 examples ship one, but the `logging-smoke-*` fixtures do, and the board
crates keep `Config::from_toml`. The maintainer principle — net config belongs in a
file, not hardcoded in Rust — keeps this path first-class alongside `DeployOverlay`.

## 9. Gaps & the config tidy (phase-227 / phase-256)

- Implicit single-node `system.toml` synthesis + the optional single-node read path.
- `nano_ros_application()` CMake function for C/C++ single-node parity.
- Per-component RT/scheduling exposure in multi-node `system.toml`.
- Book sync: `user-guide/configuration.md` still documents the Phase 172.K model.

**Config tidy (phase-256), from the 2026-06 grounded sweep:** the live config
taxonomy is **four surfaces** — `[package.metadata.nros.*]` (Rust) / `nano_ros_*`
(C/C++) for node + deploy, `system.toml` for the multi-node system, `package.xml`
for ROS identity, launch XML for topology — plus Kconfig for the embedded build.
`config.toml` and `nros.toml` are **both fully legacy** (0 example files). The tidy:
(1) finish migrating the deprecated `nros.toml` overlay blocks to typed `system.toml`
(`[lifecycle]` done; `[param_persistence]` **disabled** — in scope but incomplete, no
embedded `ParamStore` backends, issue 0080; `[[shared_state]]` **removed** — out of ROS
scope, issue 0079; `[[scheduling.contexts]]` → `[tiers]`, decision A); (2) **retire the
`nros.toml` file entirely** (no surviving role); (3) scrub the `config.toml` reader;
(4) treat transport/network as part of the **`deploy` class**, not a separate file surface;
(5) make the `deploy`-class precedence (`[..deploy.<t>]` projection vs `system.toml
[image.<id>]`) explicit. Option *scope* classes: **node** (identity/params/remaps/qos/
callback-groups), **system** (topology/capabilities/tiers — agnostic),
**deploy** (target/board/build-tuning + net + rmw/domain/locator overrides),
**build/capability** (lowered, not authored).

## 10. Runtime parameters (compile-baked initials + volatile reconfig)

ROS 2 node parameters in nano-ros follow the project's compile-time baking model, in
three layers (decided 2026-06-20; implemented by phase-264 W4):

1. **Initial values are COMPILE-BAKED from the launch file.** The toolchain reads the
   launch XML at build time (`nros::main!` for the cargo path, `generate.rs` for the
   bake path) and bakes each `<param name="…" value="…"/>` as the declaring node's
   **initial** parameter value — a compile-time constant in the generated entry, NOT a
   runtime launch-string lookup. This mirrors how topology + capabilities are baked:
   nothing about the launch file is parsed at runtime. Changing an initial value is a
   rebuild, like any other declarative change.
2. **Runtime reconfiguration is VOLATILE (RAM).** A node `declare_parameter`s its
   parameters; a runtime store is seeded from the baked initials; the node reads the
   current value with `ctx.parameter::<T>(name)`. When `[param_services]` is enabled the
   standard ROS 2 parameter services (`ros2 param get/set`) read + update that store.
   **Updates live only until the next boot** — there is no write-back to the launch file
   or any non-volatile medium.
3. **Persistence is OUT OF SCOPE.** Surviving a reboot needs a consistent non-volatile
   store (flash / NVS) — a separate concern with its own failure/atomicity model, tracked
   by the dormant `nros-params` `ParamStore` backends (**issue 0080**). nano-ros does not
   persist parameter values today; a node always boots from its baked initials.

So: **launch file → baked initial (compile time) → RAM value (boot) → live reconfig via
param services (until reboot)**. The typed `[param_persistence]` config block stays
disabled (RFC-0004 §3 / issue 0080) until the storage layer lands.

**Implementation status (2026-06-20).** For the `nros::main!` cargo path, all three
layers have landed: baked-initial read at `register` time via `NodeContext::param` (W4a);
`[param_services]` registration + volatile-store seeding behind `nros/param-services`
(W4b); and the *in-node* live typed read `ctx.parameter::<T>(name)` in `on_callback` /
`tick` (W4c — the executor's volatile store is threaded into `CallbackCtx`/`TickCtx` via
the component cell). The bake path (`generate.rs`) reaches the same
`register_parameter_services()`/`declare_parameter()` executor seam — the two paths
converge on one store. Runtime-verified: a `param_talker` node publishes its launch-baked
initial read live, and (with a wire-matched `rmw_zenoh_cpp`) a `ros2 param set` is observed
on the next callback.

## See also

- RFC-0031 (RMW selection & lowering), RFC-0024/0025 (multi-node workspace layout).
- `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md` — the file-consolidation work.
- `book/src/reference/nros-bridge-toml.md` — the separate runtime topic-forward bridge config.

## Changelog

- 2026-08-31 (`[deploy.*]` retired; issue 0951) — This page described
  `[deploy.<t>]` as the per-target rung of the precedence ladder and showed it
  in the §4 example, while mentioning `[image.*]` and `[host.*]` nowhere. The
  tree had moved: `[deploy.*]` conflated three facts and split into
  `[image.<id>]` (what is BUILT), `[host.<name>]` (WHERE it runs) and
  `[board_config.<board>]` (site config), and there are now **zero**
  `[deploy.*]` blocks in any `system.toml`.

  Corrected: the ladder rung is `[image.<id>]` (§5, §6, §9), and the §4 example
  declares an image and a host instead of a deploy. Being a **Stable** page is
  what made this worth doing promptly rather than at leisure — a stable doc is
  the one people trust without checking, so it is the most expensive one to
  leave wrong. RFC-0031 had already been amended for the same split; this page
  was the one that had not.

  `[deploy.*]` still PARSES, for out-of-tree users who have not migrated, and
  warns. Removal is phase-383 W10.b, a 0.6.0 action.
- 2026-06 (config.toml kept as a file path; issue 0081 wontfix) — Reversed the
  "config.toml fully retired" stance. The OLD `[network]/[zenoh]/[scheduling]` schema
  stays retired, but the direct-mode `config.toml` (`[node]`/`[[transport]]`/`[node.rt]`)
  read via `Config::from_toml(include_str!())` is a **supported standalone-file home**
  for hand-written `no_std` apps that bypass the `nros::main!()` codegen — net config in
  a file, not hardcoded in Rust (maintainer principle). The 3 `logging-smoke-*` fixtures
  moved their inline `const CONFIG` into sibling `config.toml` files. Updated the header
  note, §3 table + "Config in practice", §5 (renamed to standalone `config.toml`), §8.
- 2026-06 (grounded-reality revision) — A sweep of `examples/**` (0 `config.toml`,
  0 `nros.toml`) showed `nros.toml`'s §5 embedded-runtime role never landed: embedded
  net/RT lives in `[package.metadata.nros.deploy.<t>]` → `DeployOverlay` + board
  features + Kconfig. Corrected §3 (ownership + "Config in practice"), §5/§6/§7
  (marked the `nros.toml` schemas as design-that-didn't-land), §8 (config.toml reader
  scrub), §9 (the phase-256 config tidy + scope-class taxonomy). `nros.toml` is now
  legacy in full and retired as a file, not just its overlay blocks.
- 2026-06 — Revised to the unified Phase 212 model: workspace-root `nros.toml`
  rejected; config homes are Cargo/CMake metadata + universal-optional
  `system.toml`; `nros.toml` narrowed to embedded direct-mode runtime; RMW
  selection split out to RFC-0031. Transport/binding/RT schema retained.
- 2026-05 — Phase 172.K manifest model (single `nros.toml`, section-discriminated
  kinds, Cargo-style walk-up). Superseded.
