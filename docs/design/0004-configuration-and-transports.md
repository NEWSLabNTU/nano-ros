---
rfc: 0004
title: "Configuration model: `nros.toml`, transports, and node binding"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Configuration model: `nros.toml`, transports, and node binding

**Status:** Approved design (Phase 172.K, 2026-05-27; manifest model approved
2026-05-28). Supersedes the per-example `config.toml`
(`[network]`/`[zenoh]`/`[scheduling]`) and the never-shipped `nano-ros.toml`
idea (archived Phase 116). The Cargo-style manifest model below
(one `nros.toml` schema, section-discriminated, `component_nros.toml` folded
into a `[component]` table) is the canonical config shape; implementation is
tracked as Phase 172 W.1.

## One file, two read modes

`nros.toml` is the single, language-agnostic nano-ros project config. It is read
two ways from the **same** schema:

- **Direct mode** — a hand-written single-node app reads its `nros.toml` via the
  board `Config::from_toml` (compile-baked with `include_str!` on embedded;
  filesystem/env on hosted). No launch file, no planner, no generated `main`.
  This is what the `examples/**` copy-out templates use ("boilerplate IS
  lesson" — they keep their hand-written `main()`).
- **Planned mode** — the orchestration pipeline (Phase 126): launch files +
  component metadata + the system `nros.toml` → `nros plan` → `nros-plan.json` →
  generated `main()` → one binary with all nodes wired at compile time.

`package.xml` owns ROS identity + msg `<depend>` (codegen) in both modes;
`.cargo/config.toml` is dep-injection only; `Cargo.toml`/`CMakeLists.txt` own the
build. `nros.toml` owns all nano-ros runtime/deployment config.

## Manifest kinds & resolution (Cargo-style)

**Problem this fixes.** Three config roles exist, and two were *both*
named `nros.toml` — the workspace-root deployment SSOT and the direct-mode
single-node config — discriminated only by whether a `[workspace]` table is
present. A direct-mode `nros.toml` handed to `nros deploy --config nros.toml`
(which assumes a root) fails confusingly. The third role,
`component_nros.toml`, had a distinct name, but that asymmetry is its own
surprise — and a fourth file per package is friction.

**The fix: borrow Cargo.** Cargo solved exactly this — one `Cargo.toml` schema,
with `[package]` and/or `[workspace]` sections; the *sections present* decide
what the file is, and a "root package" is simply a file with **both**. Commands
resolve by walking up to the nearest `[workspace]`. nano-ros adopts the same:

**One `nros.toml` schema; the kind is decided by which sections are present
(never by filename or a separate file):**

| Section(s) present | Manifest kind | Cargo analogue |
|---|---|---|
| `[workspace]` | **workspace root** — deployment SSOT (`[system]`/`[systems.*]`, `[deploy.*]`, `[overlays.*]`, `[[bridge]]`) | `[workspace]` |
| `[component]` | **component manifest** — a reusable node (linkage, metadata, default overrides). *Replaces `component_nros.toml`.* | `[package]` (a library crate) |
| `[node]` / `[[transport]]` | **direct-mode node** — a standalone single-node binary's runtime config | a standalone `[package]` (binary) |
| `[workspace]` + `[component]` | **root component** — a component that is also the workspace root (the common single-project case) | root package (`[package]` + `[workspace]`) |
| `[workspace]` + `[node]` | **root node** — a single-node project whose one file is also the deploy SSOT | — |

**Resolution (Cargo-identical):**

1. A command (`nros deploy`/`build`/`check`) finds the nearest `nros.toml`.
2. If it has `[workspace]`, that file *is* the workspace root.
3. If it has only `[component]`/`[node]` and no `[workspace]`, the command
   **walks up** to the enclosing `[workspace]` root (a member). If none is
   found, the file is a **standalone** project (direct-mode node, or a
   detached component) — exactly Cargo's "package with no workspace".
4. `nros deploy` requires a `[workspace]` (it deploys *systems*); fed a
   bare `[node]` file with no enclosing workspace, it says so plainly:
   *"this is a direct-mode node config (no `[workspace]`) — build it
   directly, or add it to a workspace's `[system].components`."*

**Migration.** `component_nros.toml` folds into `nros.toml` as a `[component]`
table (a deprecation window accepts the old filename, warning once). Direct-mode
and root `nros.toml` are unchanged in content — only the *resolution* (walk-up +
section-discrimination) and the error messages change. `deny_unknown_fields`
stays per-table, so a file mixing a typo'd section is still caught.

### When is `[component]` (today's `component_nros.toml`) used?

It is the component **declaration**, and it is a **planned-mode** artifact only:

- **Planned mode** (orchestration): `nros plan`/`nros deploy` discover each
  `[system].components` entry through its `[component]` manifest — they need its
  **linkage** (`executable` / `exported_symbol` / `crate_name`), the
  `source_metadata` path, and the component's default `[overrides]`
  (namespace/params/remaps a launch overlay can then override). Without it,
  planning bails *"package has no exported nros component"*.
- **Direct mode**: **not used.** The hand-written `main()` *is* the component;
  there is no declaration, no planner, no metadata file.

**UX fixes (the boilerplate friction):**
- Fold it into `nros.toml` `[component]` — one file per package, not two.
- `[overrides]` and its `parameters`/`remaps` become `#[serde(default)]` — an
  empty/absent `[overrides]` is legal (today they are wrongly required, so a
  minimal manifest fails with *"missing field `parameters`"*).
- `nros new` emits the `[component]` table for the package it scaffolds, so the
  first `nros deploy` works with no hand-editing.
- `nros metadata --build` derives `linkage` from `package.xml` + the crate where
  it can, so the human writes as little as possible.

### When are `{vars}` used in `build[]` / `package[]`?

Only inside a `[deploy.<name>].build[]` / `package[]` shell step, substituted by
`nros deploy` **at deploy time, after the entry lib is emitted**. They are not
config a node reads — they are paths the runner fills into the vendor's own
shell lines:

| var | resolves to | which kinds use it |
|---|---|---|
| `{self}` | abs `deploy/<name>/` (your startup/shell/linker dir) | vendor-lib, vendor-module |
| `{entry_lib}` | the compiled `lib<sys>.a` | vendor-lib (link line) |
| `{entry_src}` | the generated entry crate dir (source form) | vendor-module (`add_subdirectory`) |
| `{entry_header}` | the cbindgen `<sys>.h` | vendor-lib / vendor-module C callers |
| `{board}` / `{target}` | `[deploy].board` / `[deploy].target` | any |
| `{vendor.dir}` | the resolved vendor SDK root | vendor-lib / vendor-module |

`self` deploys have **no** `build[]` — the generated self-shim *is* the binary,
so no vars are substituted. An unknown `{token}` that is not in this set is left
verbatim (it is shell brace syntax); a *known* var the target can't resolve is a
hard error. See the pinned interface in
`docs/roadmap/phase-172-orchestration-deferred.md`.

## Transports are top-level, decoupled, `id`-addressable

A **transport** is a physical link + the RMW session that rides it. Transports are
declared at top level, independent of nodes:

```toml
[[transport]]
id      = "eth"            # optional; defaults to `rmw` when each rmw is unique
kind    = "ethernet"       # ethernet | wifi | serial | can
ip      = "10.0.2.50/24"   # ethernet/wifi; CIDR carries the prefix
mac     = "02:00:00:00:00:01"
gateway = "10.0.2.2"
rmw     = "zenoh"          # the RMW session this transport runs
locator = "tcp/10.0.2.2:7447"
# interfaces = ["eth0", "eth1"]  # multi-homing: this ONE session spans both
                                 # NICs as one graph (see the taxonomy below).
                                 # Single-NIC: omit, or `interface = "eth0"`.

[[transport]]
id       = "bus"
kind     = "serial"
device   = "UART0"
baudrate = 115200
rmw      = "cyclonedds"

# wifi adds credentials:
# [[transport]] kind = "wifi"  ssid = "Net"  password = "secret"  rmw = "zenoh"
```

Per-kind field rules (validated by `PlanBuildOptions::validate_transports`):

| kind | fields |
|------|--------|
| `ethernet` | `ip` (CIDR), `mac`, `gateway`, `interface`/`interfaces` |
| `wifi` | `ip` (optional/static), `ssid`, `password`, `interface`/`interfaces` |
| `serial` / `can` | `device`, `baudrate` |
| all | `id`, `rmw`, `locator` |

The `id` makes a transport first-class and addressable, and disambiguates two
transports that share an `rmw` (which a bind-by-`rmw` scheme cannot).

`interfaces` (a list) **multi-homes one session over several NICs** — the
session listens/connects on each and folds them into **one** discovery graph.
This is distinct from declaring two `[[transport]]` entries (two *separate*
sessions). See the taxonomy below.

## Two axes: interfaces-per-transport × transports-per-rmw

A transport is *a session over a set of interfaces*; a node binds to a
transport. Two orthogonal axes generate the full design space:

- **interfaces per transport** (1 vs N) — multi-homing: one session, one merged
  graph, reachable across several NICs.
- **transports per rmw** (1 vs N) — one session vs multiple *segregated*
  sessions of the same backend.

| Case | transports | rmw | interfaces / transport | node binding |
|------|-----------|-----|------------------------|--------------|
| **A. cross-RMW bridge** | N | **distinct** per node | 1 each | by `rmw` (works today) |
| **B. single node, multi-homed** | 1 | one | **list** `["eth0","eth1"]` | implicit |
| **C. cross-RMW bridge, multi-homed** | N | distinct (e.g. zenoh + uORB) | **list** each | by `rmw` |
| **D. segregated same-rmw** | N | **same** (e.g. two zenoh) | 1+ each, **not merged** | by `id` (needs the K.5 runtime) |

Cases **A–C bind by `rmw`** (distinct or single) and need only the
**`interfaces` list** for multi-homing (B, C). Only case **D** — two *separate*
sessions of the *same* backend, intentionally not merged — needs
`create_node_on`-by-`id` (Phase 172.K.5); `interfaces` (merge) is the opposite
intent from D (segregate).

**Multi-homing vs DDS/zenoh native behavior.** Folding several NICs into one
graph IS what stock Fast DDS (all interfaces by default), Cyclone
(`<General><Interfaces>` list), and zenoh (multiple `listen`/`connect`
endpoints + scouting interface) already do — `interfaces` just surfaces that
config through `nros.toml`. The generator maps it per backend: zenoh → one
listen/connect endpoint per NIC + `scouting.multicast.interface`; Cyclone →
the `<Interfaces>` list; Fast DDS → whitelist (or none). Case D (separate
sessions) is the thing the middleware does *not* give you for free.

```toml
# Case C — two multi-homed sessions, distinct rmw (bind by rmw)
[[transport]]
id = "z"; kind = "ethernet"; rmw = "zenoh"; interfaces = ["eth0", "eth1"]
[[transport]]
id = "u"; kind = "ethernet"; rmw = "uorb";  interfaces = ["eth2", "eth3"]
# node_zenoh → "z" (one graph over eth0+eth1); node_uorb → "u" (over eth2+eth3)
```

## Runtime mapping

Each `[[transport]]` becomes one `SessionSpec { rmw, locator, domain_id, … }`.
The executor opens them with `Executor::open_multi([specs])`; a node's entities
ride the session of the transport it is bound to (`create_node_on`).

## Binding: how nodes connect to transports

A node binds to exactly one transport (= one session). Binding is keyed by
transport `id`, with implicit defaulting for the common cases:

| transports | binding | executor |
|------------|---------|----------|
| **0** | board-default link + the single linked RMW; zero-config | `Executor::open` |
| **1** | every node rides it implicitly; no binding syntax | `Executor::open` |
| **N** | each node names its transport; unbound nodes fall to the `default = true` transport | `Executor::open_multi` |

**Where the link is declared:**

- **Direct mode** — one implicit node rides the single/default transport; no
  binding syntax. (Direct mode is single-node by definition; the N-transport
  case is the planned/bridge path.)
- **Planned mode** — nodes come from launch; the system `nros.toml` binds
  *instances* to transports:

  ```toml
  [[component]]
  package = "sensor_pkg"; component = "sensor"
  transport = "eth"        # this component's nodes ride the eth/zenoh session

  [[component]]
  package = "logger_pkg"; component = "logger"
  transport = "bus"        # omitted ⇒ the default transport
  ```

**Runtime note.** `create_node_on(name, rmw)` selects a session by RMW name
today. When every transport has a distinct `rmw`, `id == rmw` and binding works
unchanged. Binding two transports that share an `rmw` (distinct `id`s) needs
`create_node_on` to select by transport id / session index — a small, additive
runtime extension; until it lands, the bind-by-`rmw` path covers the common
multi-RMW bridge.

## Scheduling / RT — `[node.rt]`

Scheduling has no transport home; it stays a node-level block (replacing
`config.toml [scheduling]`), and in planned mode maps to the `SchedContextConfig`
the planner already carries:

```toml
[node.rt]
app_priority = 12;  app_stack_bytes = 262144
zenoh_read_priority = 16;  zenoh_read_stack_bytes = 5120
zenoh_lease_priority = 16; zenoh_lease_stack_bytes = 5120
poll_priority = 16; poll_interval_ms = 5
```

## Worked examples

### Single-node ethernet (the common direct-mode case)

```toml
[node]
domain_id = 0

[[transport]]
kind    = "ethernet"
ip      = "10.0.2.10/24"
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
rmw     = "zenoh"
locator = "tcp/10.0.2.2:7450"
```

### ESP32 wifi

```toml
[node]
domain_id = 0

[[transport]]
kind     = "wifi"
ssid     = "MyNetwork"
password = "secret"
rmw      = "zenoh"
locator  = "tcp/192.168.1.1:7447"
```

### Two-transport bridge (planned mode)

```toml
[node]
domain_id = 0

[[transport]]
id = "eth"; kind = "ethernet"; ip = "10.0.2.50/24"; rmw = "zenoh"; locator = "tcp/10.0.2.2:7447"
[[transport]]
id = "bus"; kind = "serial"; device = "UART0"; baudrate = 115200; rmw = "cyclonedds"

# instances bind via [[component]] transport = "eth" / "bus" (see above)
```

## Migration from `config.toml` (Phase 172.K)

| `config.toml` | `nros.toml` |
|---------------|-------------|
| `[zenoh] locator`/`domain_id` | `[node] domain_id` + `[[transport]] locator` |
| `[network] ip`/`mac`/`gateway`/`prefix`/`netmask` | `[[transport]] kind="ethernet"` `ip` (CIDR) / `mac` / `gateway` |
| `[wifi] ssid`/`password` | `[[transport]] kind="wifi"` `ssid` / `password` |
| `[serial] baudrate` | `[[transport]] kind="serial"` `device` / `baudrate` |
| `[scheduling] *` | `[node.rt] *` |
| `[platform] interface` (threadx-linux) | `[[transport]] interface` |

The 8 board `Config::from_toml` parsers read the new shape; the 88 example
`config.toml` files + `include_str!` sites move to `nros.toml`; `config.toml` is
deleted.

## See also

- Phase 172 (`docs/roadmap/phase-172-orchestration-deferred.md`) — work items
  J–N; the configuration consolidation absorbed from Phase 116.
- Phase 126 (archived) — the orchestration pipeline (planned mode).
- `book/src/reference/nros-bridge-toml.md` — the separate runtime topic-forward
  bridge config (do not confuse with this build/deploy `nros.toml`).
