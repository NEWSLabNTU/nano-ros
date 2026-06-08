# `system.toml` schema — v0.1 (FROZEN)

**Status:** v0.1 frozen 2026-06-03 (Phase 212.F.4).
**Owner:** `nros-cli` — parser at `packages/nros-cli-core/src/orchestration/cargo_metadata_schema.rs::SystemToml`.
**Cross-refs:**
- nano-ros design doc `docs/design/0024-multi-node-workspace-layout.md` §4 (bringup-pkg LOCKED shape) and §11 (3-pkg-role lock, 2026-06-03).
- nano-ros roadmap `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md` §212.F (bringup pkg shape) and §212.E (`nros codegen system`).

This document is the single source of truth for:
1. What `nros check` (Phase 212.F.2) validates.
2. What `nros plan` (Phase 212.E + 172) consumes.
3. What `nros codegen system` (Phase 212.E.1) reads when baking `system_config.h`.
4. What `nros new system <name>_bringup` (Phase 212.F.1) scaffolds.

---

## 1. File location & role

A bringup package owns one `system.toml` at its root:

```
<workspace>/src/<system>_bringup/
├── package.xml
├── system.toml          ← THIS FILE
├── launch/
│   ├── system.launch.xml
│   └── …
├── config/
└── README.md
```

Per the §11 LOCKED 2026-06-03 three-pkg-role split:

| role | carries `system.toml`? |
|---|---|
| **Bringup pkg** (pure declarative; `package.xml` + `system.toml` + `launch/`) | **Yes** — exactly one |
| **Node pkg** (`nros::node!()` / `NROS_NODE()`) | No |
| **Entry pkg** (binary; `nros::main!(…)` / `NROS_MAIN(…)`) | No |

`nros check` rejects any `system.toml` found *outside* a bringup pkg
(Phase 212.F.2 lint).

A workspace with a single Entry pkg may **fold** the bringup pkg's
`launch/` + `system.toml` directly into the Entry pkg (no separate
bringup pkg). Schema in that folded case is identical; the file lives
next to the Entry pkg's `Cargo.toml` instead of standing alone.

---

## 2. Top-level tables

The whole file is parsed as `SystemToml`:

```rust
#[serde(deny_unknown_fields)]
pub struct SystemToml {
    pub system:     SystemHeader,                          // required
    pub component:  Vec<SystemComponentEntry>,             // 0..n
    pub deploy:     BTreeMap<String, DeployTarget>,        // 0..n
    pub domain:     Vec<SystemDomainEntry>,                // 0..n
    pub bridge:     Vec<SystemBridgeEntry>,                // 0..n
}
```

Every struct in v0.1 sets `deny_unknown_fields`. A typo or stale key
fails with a clear `unknown field <name>` error at parse time — no
silent drops.

| Table                | Cardinality | Required? | Purpose                                                                        |
|----------------------|-------------|-----------|--------------------------------------------------------------------------------|
| `[system]`           | one         | yes       | system-wide identity + RMW/domain SSoT + multi-launch default                  |
| `[[component]]`      | zero-to-n   | no        | node instances composed into the system                                        |
| `[deploy.<target>]`  | zero-to-n   | no        | per-target build/runner knobs (board, triple, launch override, RMW override)   |
| `[[domain]]`         | zero-to-n   | no        | named domain groups (in-binary multi-domain topology)                          |
| `[[bridge]]`         | zero-to-n   | no        | cross-RMW or cross-domain bridge declarations                                  |

---

## 3. `[system]` — SystemHeader

```toml
[system]
name           = "demo"           # required, string
rmw            = "zenoh"          # required, "zenoh" | "xrce" | "cyclonedds"
domain_id      = 0                # required, u32
locator        = "tcp/10.0.2.2:7447"   # optional, string
default_launch = "system.launch.xml"   # optional, string (RELATIVE to launch/) — see §3.1
```

| Key              | Type     | Required | Default | Notes                                                              |
|------------------|----------|----------|---------|--------------------------------------------------------------------|
| `name`           | string   | yes      | —       | MUST equal `package.xml` `<name>`; `nros check` cross-validates    |
| `rmw`            | string   | yes      | —       | Default RMW (per-deploy may override). `zenoh` / `xrce` / `cyclonedds`. |
| `domain_id`      | u32      | yes      | —       | Default `ROS_DOMAIN_ID`. Embedded targets BAKE this at build time. |
| `locator`        | string   | no       | —       | Default endpoint URI. Per-deploy may override.                     |
| `default_launch` | string   | no       | `"system.launch.xml"` | RELATIVE to `<bringup>/launch/`. See §3.1. |

### 3.1 `default_launch` — multi-launch resolution semantics

A bringup pkg may carry several `launch/*.launch.xml` files (nav2
convention — `system.launch.xml`, `talker_only.launch.xml`,
`sim.launch.xml`, …). `default_launch` names the file picked when the
caller does not specify one.

**Resolution order** (first match wins):

1. `[deploy.<target>].launch` (per-target explicit launch — strongest)
2. CLI flag: `nros plan <bringup> --launch <file>`
3. Macro arg: `nros::main!(launch = "<bringup>:<file>")` /
   `NROS_MAIN(<board>, "<bringup>:<file>")` (the `:<file>` suffix
   selects a specific launch)
4. `[system].default_launch`
5. Hard fallback: literal `"system.launch.xml"` when `default_launch`
   is absent (matches Phase 212.F.1 scaffold)

`default_launch` is a **filename relative to `<bringup>/launch/`** —
not a path. `system.launch.xml` ✓, `launch/system.launch.xml` ✗
(rejected by `nros check`), `../shared/x.launch.xml` ✗ (rejected).

---

## 4. `[[component]]` — SystemComponentEntry

Each `[[component]]` row composes one node instance into the system.
The set of components is the authoritative runtime set (per design
doc §4 "system.toml `[system].components` → nros planner's
authoritative runtime set").

```toml
[[component]]
pkg   = "talker_pkg"            # required, Node pkg name (package.xml <name>)
class = "talker_pkg::talker"    # required, fully-qualified class id
name  = "talker"                # required, instance name

[[component]]
pkg   = "listener_pkg"
class = "listener_pkg::listener"
name  = "listener"
```

| Key     | Type   | Required | Notes                                                                                          |
|---------|--------|----------|------------------------------------------------------------------------------------------------|
| `pkg`   | string | yes      | The Node pkg's `package.xml` `<name>`. Must appear in the bringup `<exec_depend>` list. Cross-validated by `nros check --bringup` (Phase 212.G.2). |
| `class` | string | yes      | Fully-qualified component class (`<crate-path>::<UserClass>` for Rust; mirrors `[package.metadata.nros.component].class`). Used by codegen to land at the right type. |
| `name`  | string | yes      | Instance name — unique within the file. Used as the node name and as the planner / codegen instance id. |

**Naming:** historical "Component pkg" terminology is retired in
favour of "Node pkg" (per §11.1). The TOML key remains `[[component]]`
in v0.1 — the field name change is a v1 mechanical sweep tracked by
roadmap N.12.

---

## 5. `[deploy.<target>]` — DeployTarget

Per-target build + runner knobs. `<target>` is the target identifier
(e.g. `native`, `qemu-mps2-an385`, `zephyr_native_sim`,
`flash-stm32f4-disco`). Cross-references the per-target deploy
metadata at `[package.metadata.nros.deploy.<target>]` (in component
pkgs' `Cargo.toml`).

```toml
[deploy.native]
kind   = "self"
target = "x86_64-unknown-linux-gnu"
launch = "system.launch.xml"   # optional override of [system].default_launch
board  = "native"              # optional

[deploy.qemu-mps2-an385]
kind   = "freertos"
target = "thumbv7m-none-eabi"
board  = "mps2-an385"
launch = "system.launch.xml"
```

| Key      | Type   | Required | Notes                                                                    |
|----------|--------|----------|--------------------------------------------------------------------------|
| `kind`   | string | yes      | Runner kind. Free-form string interpreted by the runner stage. Values seen in tree: `"self"`, `"qemu"`, `"flash"`, `"freertos"`, `"zephyr"`, `"nuttx"`, `"threadx"`, `"esp-idf"`, `"platformio"`. |
| `target` | string | yes      | Cargo target triple (`x86_64-unknown-linux-gnu`, `thumbv7m-none-eabi`, …) or a runner key whose semantics depend on `kind`. |
| `launch` | string | no       | Per-target launch-file override (relative to `<bringup>/launch/`). Beats `[system].default_launch`. |
| `board`  | string | no       | Board identifier (`mps2-an385`, `native_sim/native/64`, `qemu-armv7a-nsh`, …). Cross-refs the `nros-board-<id>` crate. |
| `framework` | string | no    | PlatformIO-specific framework selector (`"espidf"`, `"arduino"`, …). Held verbatim and forwarded to the runner; ignored by non-PlatformIO targets. Added 2026-06-03 (F.4 §12 gap #3 resolution). |

---

## 6. `[[domain]]` — SystemDomainEntry (optional, in-binary multi-domain)

Names additional domain groups beyond the default `[system].domain_id`.
Used by `Executor::open_multi(&[…])` to bake a multi-domain bringup
into one binary.

```toml
[[domain]]
name = "telemetry"
rmw  = "zenoh"
id   = 7
```

| Key    | Type   | Required | Notes                                            |
|--------|--------|----------|--------------------------------------------------|
| `name` | string | yes      | Domain handle, referenced by `[[bridge]].from`/`to`. |
| `rmw`  | string | yes      | RMW backend running this domain.                 |
| `id`   | u32    | yes      | `ROS_DOMAIN_ID` for this domain.                 |

---

## 7. `[[bridge]]` — SystemBridgeEntry (optional, cross-domain / cross-RMW)

Declares a bridge between two domains by name. Topic filtering is a
v1 concern; v0.1 bridges are all-topics.

```toml
[[bridge]]
name = "telem_to_default"
from = "telemetry"   # references a [[domain]].name (or "default" for [system].domain_id)
to   = "default"
```

| Key    | Type   | Required | Notes                                                                                   |
|--------|--------|----------|-----------------------------------------------------------------------------------------|
| `name` | string | yes      | Bridge instance name, unique within the file.                                           |
| `from` | string | yes      | Source domain — references `[[domain]].name` (or `"default"` for `[system].domain_id`). |
| `to`   | string | yes      | Sink domain — same vocabulary as `from`.                                                |

---

## 8. Strict mode

Every struct in v0.1 sets `#[serde(deny_unknown_fields)]`. The
following raise hard parse errors:

- A typo'd top-level table (e.g. `[deplly.native]` instead of
  `[deploy.native]`).
- A stray key on `[system]` (e.g. `default_lunch = "…"`).
- An unknown `[[component]]` / `[[domain]]` / `[[bridge]]` field.

`nros check` surfaces the underlying serde diagnostic verbatim
(`unknown field <name>, expected one of …`) so users land on the
exact bad line.

---

## 9. Complete example

A minimal-but-realistic 2-Node-pkg bringup with two deploy targets, a
non-default domain, and a bridge:

```toml
# src/demo_bringup/system.toml

[system]
name           = "demo"
rmw            = "zenoh"
domain_id      = 0
locator        = "tcp/127.0.0.1:7447"
default_launch = "system.launch.xml"

[[component]]
pkg   = "talker_pkg"
class = "talker_pkg::talker"
name  = "talker"

[[component]]
pkg   = "listener_pkg"
class = "listener_pkg::listener"
name  = "listener"

[deploy.native]
kind   = "self"
target = "x86_64-unknown-linux-gnu"

[deploy.qemu-mps2-an385]
kind   = "freertos"
target = "thumbv7m-none-eabi"
board  = "mps2-an385"
launch = "talker_only.launch.xml"   # this target boots a smaller topology

# Optional: a second domain for high-rate telemetry…
[[domain]]
name = "telemetry"
rmw  = "zenoh"
id   = 7

# …and a bridge that gateways selected traffic back to the default domain.
[[bridge]]
name = "telem_to_default"
from = "telemetry"
to   = "default"
```

Matching `package.xml` `<exec_depend>` block (cross-validated by
`nros check --bringup`):

```xml
<exec_depend>talker_pkg</exec_depend>
<exec_depend>listener_pkg</exec_depend>
```

Matching `launch/` contents (selectable via `nros plan demo_bringup
--launch <file>` or via the deploy override above):

```
launch/
├── system.launch.xml         # default per [system].default_launch
└── talker_only.launch.xml    # selected by [deploy.qemu-mps2-an385].launch
```

---

## 10. Stability + versioning

v0.1 is the FROZEN initial schema. Forward-compatible additions
(new optional keys) are minor bumps (v0.2, v0.3, …). Removing or
renaming keys requires a major bump (v1.0) and a `nros migrate
workspace` sweep.

The schema version is **implicit** in v0.1 — no `schema_version`
field. v1.0 will introduce one if and when a breaking change is
needed.

---

## 11. Parser reference

The canonical types are defined in
[`packages/nros-cli-core/src/orchestration/cargo_metadata_schema.rs`][parser]:

- `SystemToml` — whole file
- `SystemHeader` — `[system]` table
- `SystemComponentEntry` — `[[component]]` row
- `DeployTarget` — `[deploy.<target>]` block
- `SystemDomainEntry` — `[[domain]]` row
- `SystemBridgeEntry` — `[[bridge]]` row

Loader: `NrosConfig::load` in
`packages/nros-cli-core/src/orchestration/nros_config.rs` reads the
file via `toml::from_str`, surfacing `BringupSystemTomlParse` errors
with file-path context.

[parser]: ../packages/nros-cli-core/src/orchestration/cargo_metadata_schema.rs

---

## 12. Known gaps surfaced by the F.4 audit (parser follow-ups)

These are open items the schema-vs-parser cross-check uncovered while
landing v0.1. None block freezing the schema — each is a parser
implementation gap to be fixed under a follow-up commit (NOT under
F.4 itself per phase-doc scope).

1. **RESOLVED 2026-06-03** — *`[system].default_launch` not accepted by
   `SystemHeader`.* Fixed in the phase-212-f4-parser-fixes branch
   (commit `f1e42a9`). `SystemHeader` now carries
   `pub default_launch: Option<String>` alongside `name` / `rmw` /
   `domain_id` / `locator`. The §3.1 five-step resolution order is
   unchanged; the parser just records the user's value (or `None` for
   the §3.1 step-5 hard fallback).
2. **RESOLVED 2026-06-03** — *`[deploy.<target>]` `kind`/`target`
   mandatory.* Fixed in commit `a2b2098` via path (a) — relax parser.
   Both fields are now `Option<String>`. Rationale: deploy is
   configuration-by-TARGET (the map key already names the runner), and
   the in-tree `multi_pkg_workspace_threadx` /
   `multi_pkg_workspace_platformio` fixtures author the looser shape.
   `deny_unknown_fields` is preserved. The companion `nros check` lint
   (warn when `kind`/`target` is absent AND the runner can't
   synthesise sensible defaults from the target name) is **deferred** —
   it depends on the runner-side synthesis-defaults table that lives
   in pkg_index / launch territory (concurrent N.* worker scope).
3. **RESOLVED 2026-06-03** — *`[deploy.platformio].framework` not a
   schema field.* Fixed in commit `83d99c9` by extending `DeployTarget`
   with `pub framework: Option<String>` (§6 above lists it as an
   optional PlatformIO-specific field). Round-trip-tested through the
   `multi_pkg_workspace_platformio` fixture shape.
4. **`[[component]]`/`[[domain]]`/`[[bridge]]` use the
   `#[serde(rename = "component"/"domain"/"bridge")]` singular form
   on the Rust side but Vec-of-rows TOML form on disk.** Documented
   here as `[[component]]` / `[[domain]]` / `[[bridge]]`. No code
   change needed — just noting the rename so future maintainers
   reading the parser don't mistake it for a single-row table.
