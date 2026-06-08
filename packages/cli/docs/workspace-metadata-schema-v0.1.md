# `[workspace.metadata.nros]` / `[package.metadata.nros]` / `[package.metadata.ament]` schema — v0.1 (FROZEN)

**Status:** v0.1 frozen 2026-06-03 (Phase 212.B.1).
**Owner:** `nros-cli` — parser at `packages/nros-cli-core/src/orchestration/cargo_metadata_schema.rs`.
**Cross-refs:**
- nano-ros design doc `docs/design/0024-multi-node-workspace-layout.md` §5 (workspace root metadata) + §11 (3-pkg-role lock, 2026-06-03).
- nano-ros roadmap `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md` §212.B (cargo-native metadata) + §212.G (`package.xml` emit) + §212.L.7 (self-entry planner).
- Companion doc `docs/system-toml-schema-v0.1.md` for `<bringup>/system.toml`.

This document is the single source of truth for the **cargo-manifest-side** Phase 212 metadata surfaces:

1. `[workspace.metadata.nros]` — workspace-root pointers (this file).
2. `[package.metadata.nros]` — per-pkg Node / Component / Application / Entry role markers + per-target deploy table.
3. `[package.metadata.ament]` — `package.xml` source-of-truth for the (internal) `nros emit package-xml` helper.

Every struct sets `#[serde(deny_unknown_fields)]`. Typos surface as `unknown field <name>, expected one of …` parse errors at the user's terminal — no silent drops.

---

## 1. `[workspace.metadata.nros]`

Lives in the workspace-root `Cargo.toml`. Thin pointer to the system the workspace defaults to.

```toml
[workspace.metadata.nros]
default_system     = "demo_bringup"   # OR an Entry/Node pkg name (Phase 212.L.7)
rmw_override       = "cyclonedds"     # optional
domain_id_override = 7                # optional
```

| Key                  | Type   | Required | Default | Notes                                                                                                                                                |
|----------------------|--------|----------|---------|------------------------------------------------------------------------------------------------------------------------------------------------------|
| `default_system`     | string | no       | —       | Bringup pkg name (`<system>_bringup`) OR Entry/Node pkg name (Phase 212.L.7 self-entry shape). `nros plan`, Entry codegen, and `nros codegen-system` resolve via this pointer when no `--bringup` hint is supplied. |
| `rmw_override`       | string | no       | —       | Workspace-wide RMW override — rare, intended for `nros plan --override` workflows. Values: `"zenoh"` / `"xrce"` / `"cyclonedds"`.                    |
| `domain_id_override` | u32    | no       | —       | Workspace-wide `ROS_DOMAIN_ID` override. When present, propagates into generated `system_config.h` instead of the per-deploy / `[system].domain_id` value. |

Schema struct: `WorkspaceMetadataNros`.

---

## 2. `[package.metadata.nros]`

Lives in each component / application / entry / bringup pkg's `Cargo.toml`. Tags the pkg's role in the system and (optionally) carries per-target deploy parameters.

### 2.1 Shape exclusivity

A pkg picks **exactly one** of the following role shapes:

* Single-node crate — `[package.metadata.nros.node]` (canonical) or `[package.metadata.nros.component]` (deprecated alias, Phase 212.N.12 rename).
* Multi-node crate — `[package.metadata.nros.nodes.<Name>]` (canonical) or `[package.metadata.nros.components.<Name>]` (deprecated alias).
* Application crate — `[package.metadata.nros.application]` (native-only orchestration root).

`[package.metadata.nros.entry]` is **orthogonal** and may coexist with `node` / `nodes` / `application`: a pkg that carries both a node role AND `[entry]` eats its own Entry role (Phase 212.L.7 self-entry; the L.7 self-entry planner emits a 1-node plan directly from cargo metadata).

Declaring more than one role shape (`component` + `components`, `node` + `application`, …) is a hard validation error. Declaring `node` AND `component` (the rename alias) is also a hard error.

### 2.2 `[package.metadata.nros.node]` / `.components.<Name>` / `.nodes.<Name>` — `ComponentMetadata`

```toml
[package.metadata.nros.node]
class             = "talker_pkg::TalkerNode"   # required for codegen
name              = "talker"                   # optional instance name
default_namespace = "/demo"                    # optional

[package.metadata.nros.node.parameters]
rate_hz  = 10
greeting = "hello"

[[package.metadata.nros.node.remaps]]
from = "chatter"
to   = "topic/chatter"
```

| Key                 | Type                        | Required | Notes                                                                                                  |
|---------------------|-----------------------------|----------|--------------------------------------------------------------------------------------------------------|
| `class`             | string                      | no       | Fully-qualified component class (`<crate-path>::<UserClass>`). `nros check` enforces `<pkg-name>::…` match. |
| `name`              | string                      | no       | Short instance name; falls back to the pkg name on synthesis.                                          |
| `default_namespace` | string                      | no       | Default ROS namespace; absent → `/`.                                                                   |
| `parameters`        | table of TOML values        | no       | Raw ROS parameter declarations. Lowering happens in the planner.                                       |
| `remaps`            | array of `{from, to}` rows  | no       | Topic / service remaps, mirrors rclpy / rclcpp semantics.                                              |

### 2.3 `[package.metadata.nros.application]` — `ApplicationMetadata`

```toml
[package.metadata.nros.application]
name   = "demo_app"
deploy = ["native", "qemu-arm-baremetal"]
```

| Key      | Type           | Required | Notes                                                                                                                |
|----------|----------------|----------|----------------------------------------------------------------------------------------------------------------------|
| `name`   | string         | no       | App display name; absent → `[package].name`.                                                                         |
| `deploy` | array<string>  | no       | Allow-list of deploy targets the application accepts. MUST NOT name an RTOS target — `nros check` enforces. Keys cross-ref `[package.metadata.nros.deploy.<target>]`. |

### 2.4 `[package.metadata.nros.entry]` — `EntryMetadata`

Phase 212.N.7. Marks an Entry pkg (the firmware bin) so the planner can route it to the right `[deploy.<target>]` block.

```toml
[package.metadata.nros.entry]
deploy = "freertos"
```

| Key      | Type   | Required | Notes                                                                          |
|----------|--------|----------|--------------------------------------------------------------------------------|
| `deploy` | string | yes      | Deploy-target key naming which `[deploy.<target>]` block the Entry pkg runs on. |

### 2.5 `[package.metadata.nros.deploy.<target>]` — `DeployTargetMetadata`

Per-target deploy parameters. Keyed by target name (`native`, `qemu-mps2-an385`, `flash-stm32f4-disco`, …). Populates application pkgs and self-bringup component/Entry pkgs (Phase 212.L.7/L.8).

```toml
[package.metadata.nros.deploy.native]
board      = "native_sim/native/64"
rmw        = "zenoh"
domain_id  = 7
locator    = "tcp/127.0.0.1:7447"
```

| Key         | Type   | Required | Notes                                                                          |
|-------------|--------|----------|--------------------------------------------------------------------------------|
| `board`     | string | no       | Board identifier (`mps2-an385`, `native_sim/native/64`, …).                    |
| `rmw`       | string | no       | RMW backend (`zenoh` / `xrce` / `cyclonedds`).                                 |
| `domain_id` | u32    | no       | Baked `ROS_DOMAIN_ID`. Embedded targets bake at build time.                    |
| `locator`   | string | no       | RMW locator URI (e.g. `tcp/127.0.0.1:7447`).                                    |

### 2.6 Opaque stubs (Phase 212.B.2)

`[package.metadata.nros.domain]`, `[package.metadata.nros.bridge]`, `[package.metadata.nros.embedded]` are accepted as opaque `toml::Value` pass-throughs during the schema in-flight window. Strict typed shapes land with the F.4 follow-up. `deny_unknown_fields` still surfaces typos at every sibling table.

---

## 3. `[package.metadata.ament]`

Lives in every pkg's `Cargo.toml`. The source of truth for the (internal) `nros emit package-xml` helper. Mirrors ament/colcon's `package.xml` vocabulary 1-to-1.

```toml
[package.metadata.ament]
description      = "A talker that publishes std_msgs/String at 10 Hz."
license          = "Apache-2.0"
maintainer       = { name = "Ada Lovelace", email = "ada@example.com" }
buildtool_depend = ["ament_cargo"]
build_depend     = ["rosidl_default_generators", "std_msgs"]
exec_depend      = ["std_msgs", "rosidl_default_runtime"]
test_depend      = ["ament_lint_auto"]
build_type       = "ament_cargo"
```

| Key                | Type                      | Required | Default                            | Notes                                                                  |
|--------------------|---------------------------|----------|------------------------------------|------------------------------------------------------------------------|
| `description`      | string                    | no       | synthesised from pkg name          | `<description>` body. `[package].description` is a fallback.           |
| `license`          | string                    | no       | `"Apache-2.0"`                     | `<license>` body. `[package].license` is a fallback.                    |
| `maintainer`       | `{name, email}` table     | no       | `Developer <dev@example.com>`      | Single `<maintainer email="…">…</maintainer>` row.                      |
| `build_depend`     | array<string>             | no       | `[]`                               | `<build_depend>` rows.                                                  |
| `buildtool_depend` | array<string>             | no       | `[]`                               | `<buildtool_depend>` rows (e.g. `"ament_cargo"`, `"ament_cmake"`).      |
| `exec_depend`      | array<string>             | no       | `[]`                               | `<exec_depend>` rows.                                                   |
| `test_depend`      | array<string>             | no       | `[]`                               | `<test_depend>` rows.                                                   |
| `build_type`       | string                    | no       | `"ament_cargo"` for component pkgs, `"ament_cmake"` for bringup pkgs | `<export><build_type>…</build_type></export>` body.                     |

`maintainer = { name, email }` is strict: both fields mandatory when the table is present (matches `package.xml` schema requirements). Stray fields (e.g. `affiliation = "…"`) fail at parse time.

Dependency rows are sorted and deduplicated by the emitter so list ordering doesn't drift across `Cargo.toml` edits.

---

## 4. Strict mode

Every struct sets `#[serde(deny_unknown_fields)]`. The following raise hard parse errors with the underlying serde diagnostic verbatim (`unknown field <name>, expected one of …`):

- A typo'd top-level table (e.g. `[workspace.metadata.nrso]` instead of `[workspace.metadata.nros]`).
- A stray key on `[package.metadata.nros]` / `[package.metadata.nros.node]` / `[package.metadata.nros.application]` / `[package.metadata.nros.entry]` / `[package.metadata.nros.deploy.<target>]`.
- A stray key on `[package.metadata.ament]` or the nested `maintainer = { … }` row.
- A stray key on `[workspace.metadata.nros]`.

`nros check` surfaces the underlying serde diagnostic verbatim so users land on the exact bad line.

---

## 5. Vocabulary discipline

Field names are a strict subset of names that already appear in:

- `nros-sdk-index.toml` (the canonical SDK pin file).
- `app_config.h` (the codegen baker's emit keys).
- The existing planner schema (`packages/nros-cli-core/src/orchestration/schema.rs`).

No second TOML dialect. Renames go through the Phase 212.N.12 in-flight alias mechanism (e.g. `component` → `node`), not parallel vocabulary.

---

## 6. Stability + versioning

v0.1 is the FROZEN initial schema. Forward-compatible additions (new optional keys) are minor bumps (v0.2, v0.3, …). Removing or renaming keys requires a major bump (v1.0) and a `nros migrate workspace` sweep.

The schema version is **implicit** in v0.1 — no `schema_version` field on any table. v1.0 will introduce one if and when a breaking change is needed.

---

## 7. Parser reference

The canonical types are defined in
[`packages/nros-cli-core/src/orchestration/cargo_metadata_schema.rs`][parser]:

- `WorkspaceMetadataNros` — `[workspace.metadata.nros]`
- `PackageMetadataNros` — `[package.metadata.nros]` outer table
- `ComponentMetadata` — `.node` / `.component` / `.nodes.<N>` / `.components.<N>` shape
- `ApplicationMetadata` — `.application` shape
- `EntryMetadata` — `.entry` shape
- `DeployTargetMetadata` — `.deploy.<target>` rows
- `PackageMetadataAment` — `[package.metadata.ament]`
- `AmentMaintainer` — `maintainer = { name, email }` row

Loader: `NrosConfig::from_cargo_metadata` in
`packages/nros-cli-core/src/orchestration/nros_config.rs` walks the cargo
workspace via `cargo_metadata --no-deps` and threads every member's
`[package.metadata.nros]` + `[package.metadata.ament]` through the strict
schema. The path-based helper `orchestration::ament::parse_ament_metadata`
is the alternate entry point for callers that have a pkg dir on disk.

[parser]: ../packages/nros-cli-core/src/orchestration/cargo_metadata_schema.rs

---

## 8. Worked example

A minimal-but-realistic two-Node-pkg workspace with a sibling bringup pkg:

```toml
# workspace-root Cargo.toml
[workspace]
resolver = "2"
members  = ["talker_pkg", "listener_pkg", "demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
```

```toml
# talker_pkg/Cargo.toml
[package]
name    = "talker_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.node]
class             = "talker_pkg::TalkerNode"
name              = "talker"
default_namespace = "/demo"

[package.metadata.nros.node.parameters]
rate_hz = 10

[package.metadata.nros.deploy.native]
rmw       = "zenoh"
domain_id = 0
locator   = "tcp/127.0.0.1:7447"

[package.metadata.ament]
description      = "Talker — publishes std_msgs/String at 10 Hz."
license          = "Apache-2.0"
maintainer       = { name = "Ada Lovelace", email = "ada@example.com" }
buildtool_depend = ["ament_cargo"]
build_depend     = ["std_msgs"]
exec_depend      = ["std_msgs", "rosidl_default_runtime"]
```

```toml
# demo_bringup/Cargo.toml — no [package.metadata.nros]; sibling system.toml
[package]
name    = "demo_bringup"
version = "0.1.0"
edition = "2021"

[package.metadata.ament]
description = "Bringup for the demo system."
license     = "Apache-2.0"
exec_depend = ["talker_pkg", "listener_pkg"]
```

`<bringup>/system.toml` shape is documented separately in `docs/system-toml-schema-v0.1.md`.
