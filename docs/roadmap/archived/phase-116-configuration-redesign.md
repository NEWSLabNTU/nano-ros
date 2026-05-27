# Phase 116: Configuration Redesign — one `nros.toml`, native build files stay native

> **Archived 2026-05-27 — subsumed by Phase 172.** Investigation showed
> configuration is not a standalone concern: it is the **input contract of the
> compile-time orchestration pipeline** that Phase 126 already shipped
> (launch + component/system `nros.toml` → `nros-plan.json` → generated single
> binary). The surviving config work (network/peripheral schema, `config.toml`
> retirement, `nros.toml` name-collision fix, colcon RMW wiring, single-node
> "direct mode") moved into **Phase 172** (orchestration follow-ups) as items
> 172.J–172.N. This file is retained as the design exploration that led there;
> its single-`[node]` schema and the package.xml-vs-`nros.toml` (A/B) framing
> are **superseded** by the Phase 126 component/system model — do not implement
> from this doc, see Phase 172.

**Goal:** Collapse the nano-ros config surface to a clear one-lane-per-file model.
`nros.toml` becomes the single, language-agnostic nano-ros config (RMW activation,
peripherals/transport, per-node options, real-time/scheduling). The two
overlapping runtime TOMLs (`nros.toml` bridge config + `config.toml` app config)
merge into one. Language-native build files (`Cargo.toml`, `CMakeLists.txt`) keep
owning the build; `package.xml` keeps owning ROS message deps; `.cargo/config.toml`
is dep-injection only.

**Status:** Proposed (rescoped 2026-05-27)
**Priority:** Medium (consolidation; unblocks clean RT config story)
**Depends on:** Phase 111 (`nros` CLI — the consumer, shipped), Phase 124 follow-up
(`nros-bridge` `run_from_config` — the existing `nros.toml` loader to extend)
**Supersedes:** the original "unified `nano-ros.toml` + package registry + board
descriptors" scope (see *History / what changed* below).

---

## Why rescoped

The original Phase 116 proposed a **new** `nano-ros.toml` that would own both build
and runtime, emitting transient `Cargo.toml`/`.cargo/config.toml`/`CMakeLists.txt`
the user never edits. The architecture moved against that:

- **Examples are deliberately hand-written copy-out templates** (CLAUDE.md, Phase
  118/131: "boilerplate IS lesson, no walk-up, no workspace reliance"). A
  generate-and-hide build model fights that bet head-on.
- **Per-RMW build dirs already isolate the build** (Phase 176/181: `build-<rmw>/`
  + `target-<rmw>/`). The "config sprawl / RMW directory explosion" the old phase
  targeted was already collapsed by Phase 118/168 (RMW selected at build time).
- **`find_package(NanoRos)` is gone** (Phase 140) — the old §B registry's
  `find_package` / `cmake_target` consumer path is dead.
- **Per-RTOS integration shells exist** (Phase 139: west / ESP-IDF component /
  PlatformIO / NuttX Kconfig / PX4) — the old §B "C/C++ users have nothing"
  premise is mostly closed by native package managers.

What remains genuinely worth doing is **config consolidation**, not a build-system
takeover. Today there are two runtime TOMLs with overlapping intent:

| File | Scope | Consumed | Reach |
|------|-------|----------|-------|
| `config.toml` | single-app: `[network]` `[zenoh]` `[wifi]` `[scheduling]`/stack | compile-baked via `Config::from_toml(include_str!("config.toml"))` (bare-metal) + board `build.rs` (FreeRTOS/ThreadX); `nros config show/check` reads it | **88 examples**, 8 board `from_toml` parsers, 5 board `build.rs` |
| `nros.toml` | multi-backend bridge: `[[node]]` + `[[bridge]]` | FS-read at runtime via `nros_bridge::run_from_config` | **0 examples** (doc-only) |

Both pick locator/domain. The redesign keeps the better-named one (`nros.toml`) and
makes it the single nano-ros config across all languages; `config.toml` is deleted.

---

## Architecture

### File ownership — one lane each

| File | Owner | Holds | Notes |
|------|-------|-------|-------|
| `Cargo.toml` | Rust build | crate metadata, language deps, the **RMW feature menu** (`rmw-zenoh`/`rmw-cyclonedds`/`rmw-xrce`) | Rust projects build with `cargo` directly. Unchanged shape. |
| `CMakeLists.txt` | C/C++ build | targets, language deps, the `NROS_RMW` **option**, `add_subdirectory(<repo-root>)` glue | C/C++ projects build with `cmake` directly. Unchanged shape. |
| `.cargo/config.toml` | cargo | **`[patch.crates-io]` dep injection only** — local crate paths + generated msg paths. **No nano-ros semantic boilerplate.** | Audited clean of any config that belongs in `nros.toml`. |
| `package.xml` | ROS | package type + ROS deps. **Only `<depend>` msg packages are relevant today** (codegen input for `nros generate`). | Unchanged. The interface SSOT. |
| **`nros.toml`** | **nano-ros** | **RMW activation, peripherals/transport, per-node options, RT/scheduling** | **Universal — same file for Rust, C, C++.** Compile-baked on embedded, FS/env on hosted. |
| ~~`config.toml`~~ | — | **deleted** — content folds into `nros.toml` `[node.network]` / `[node.transport]` / `[node.rt]` | Migration below. |

**Boundary rule:** if a knob changes *what is compiled or linked*, it lives in the
build file (`Cargo.toml` feature / `CMakeLists.txt` option). If it changes *what
nano-ros does at run time or how a node is configured*, it lives in `nros.toml`.
`package.xml` owns message-package identity; `.cargo/config.toml` owns nothing but
dependency path injection.

### `nros.toml` schema (universal; single-node shorthand + multi-node superset)

```toml
# nros.toml — nano-ros project config (language-agnostic)

[project]
ros_edition = "humble"            # humble | iron

# --- single-node shorthand (the common case) ---
[node]                            # use ONE [node], or many [[node]] for bridges
rmw       = "zenoh"               # ACTIVE backend; must match a LINKED backend
domain_id = 0
namespace = "/"
locator   = "tcp/10.0.2.2:7450"

[node.network]                    # peripheral config (was config.toml [network])
ip      = "10.0.2.10"
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
prefix  = 24

[node.transport]                  # transport selection (was board feature + config.toml)
kind = "ethernet"                 # ethernet | wifi | serial
# [node.transport.wifi]   ssid = "...", password = "..."
# [node.transport.serial] device = "/dev/ttyUSB0", baud = 115200

[node.rt]                         # scheduling / real-time (was config.toml [scheduling])
app_priority         = 12
zenoh_read_priority  = 16
zenoh_lease_priority = 14
poll_priority        = 10
app_stack_bytes      = 65536
# future RT (Phase 162): deadline_us, period_us, cpu_affinity, exec_model

[interfaces]
generate = ["std_msgs"]           # mirrors package.xml msg <depend>s; codegen input

# --- multi-node / bridge (today's nros.toml, unchanged) ---
# [[node]] name = "field"   rmw = "zenoh"      locator = "tcp/10.0.0.1:7447"
# [[node]] name = "control" rmw = "cyclonedds" locator = "domain=0"
# [[bridge]] type = "std_msgs/Int32" from = { node="field", topic="/x" }
#                                    to   = { node="control", topic="/x" }
```

The single-node `[node]` is exactly the degenerate one-`[[node]]` bridge case — one
schema, two spellings. `[node.network]`/`[node.transport]`/`[node.rt]` are the homes
for the content that `config.toml` carried today.

### RMW selection — the linked-vs-active split

This is the crux of "how do we configure RMW selection across languages."

- **Linked** = *which backend's code is compiled and linked into the binary.* This is
  intrinsically a build/link operation, so it stays in the build file:
  - Rust: a Cargo feature (`--features rmw-zenoh`), mutually exclusive.
  - C/C++: `cmake -DNROS_RMW=zenoh`.
  - Zephyr: the `prj-<rmw>.conf` Kconfig overlay.

  The build file exposes the **menu** of available backends; it does not need to
  hard-pick one (a `default` feature stays only for bare-`cargo run` ergonomics).

- **Active** = *which linked backend opens a session, and how each node behaves.*
  This lives in `nros.toml` `rmw =` per node. Naming a backend that was not linked
  surfaces the existing `ConfigError::OpenSession` (the bridge loader already does
  this).

- **Single source of truth for the choice = `nros.toml`.** `nros build` reads
  `nros.toml`'s `rmw` and threads it to the native tool: Rust →
  `cargo build --features rmw-<x>`, C/C++ → `cmake -DNROS_RMW=<x>`, Zephyr → select
  the `prj-<x>.conf` overlay. Building **manually** with `cargo`/`cmake` still works
  unchanged — you supply the feature/`-D` yourself (the build files remain
  standalone-buildable; nothing reads `nros.toml` *implicitly* during a raw
  `cargo build`).

- **No duplication.** The multi-RMW bridge case already requires `nros.toml` to name
  `rmw` per node, so single-node simply reuses that field. The choice is stated once
  (`nros.toml`); the build file lists the menu, not "the" selection.

### Peripherals & RT on embedded (compile-baked)

Bare-metal targets have no filesystem, so `nros.toml`'s peripheral/RT blocks are
**baked at compile time**, exactly like `config.toml` today:
`Config::from_toml(include_str!("nros.toml"))`. Hosted targets read it from the
filesystem (or env overrides like `ROS_DOMAIN_ID`/`ZENOH_LOCATOR` continue to win).
The 8 board `Config::from_toml` parsers and 5 board `build.rs` files retarget from
`config.toml`'s `[network]`/`[scheduling]` to `nros.toml`'s
`[node.network]`/`[node.rt]`. Real-time grows under `[node.rt]` (one place,
per-node), feeding the Phase 162 RT scheduling harness.

---

## Work Items

### A — `nros.toml` schema + loader

- [ ] **116.A.1** Extend the `nros-bridge` config loader: add the single-node
  `[node]` shorthand (alias for a one-element `[[node]]`) + `[project]`,
  `[node.network]`, `[node.transport]`, `[node.rt]`, `[interfaces]` sections.
- [ ] **116.A.2** JSON-schema for `nros.toml` bundled with the `nros` binary;
  `nros config check` validates against it.
- [ ] **116.A.3** `Config::from_toml` (the 8 board crates) reads
  `[node.network]`/`[node.rt]` from `nros.toml` instead of `config.toml`'s
  `[network]`/`[scheduling]`. Keep the `&'static str` / `include_str!` signature
  (compile-bake on embedded).
- [ ] **116.A.4** `docs/reference/nros-toml.md` (+ `book/src/reference/nros-toml.md`)
  grow the single-node + peripherals + RT sections; mark the bridge sections as the
  multi-node superset.

### B — RMW selection via `nros build`

- [ ] **116.B.1** `nros build` reads `nros.toml` `node.rmw`, threads it to the
  native tool (Cargo `--features rmw-<x>` / CMake `-DNROS_RMW=<x>` / Zephyr
  `prj-<x>.conf`). Raw `cargo`/`cmake` builds keep working with hand-passed
  selection.
- [ ] **116.B.2** Mismatch handling: `nros.toml` names an `rmw` whose feature/option
  is absent from the build file → clear error at `nros build` time (don't wait for
  the runtime `OpenSession`).
- [ ] **116.B.3** Document the linked-vs-active split in
  `book/src/internals/rmw-backends.md` + the build-system handbook.

### C — kill `config.toml`, audit `.cargo/config.toml`

- [ ] **116.C.1** Migrate all 88 example `config.toml` files to `nros.toml`
  (`[network]`→`[node.network]`, `[zenoh]`→`[node]` locator/domain,
  `[scheduling]`→`[node.rt]`, `[wifi]`→`[node.transport.wifi]`).
- [ ] **116.C.2** Flip the 86 `include_str!("config.toml")` example call sites +
  5 board `build.rs` to `nros.toml`. Delete `config.toml`.
- [ ] **116.C.3** Audit every `.cargo/config.toml`: confirm it holds **only**
  `[patch.crates-io]` (local crate + generated msg paths). Move any stray nano-ros
  config out.
- [ ] **116.C.4** `nros config show/check` default path → `nros.toml`.
- [ ] **116.C.5** `book/src/user-guide/configuration.md` rewritten around the
  one-lane-per-file model; one worked example showing all five files.

**Files:**
- `packages/.../nros-bridge/` (loader + schema)
- `packages/boards/nros-board-*/src/config.rs` (8 `from_toml` parsers)
- `packages/boards/nros-board-*/build.rs` (5 bakers)
- `packages/codegen/packages/nros-cli-core/src/cmd/{config,build}.rs`
- `examples/**/{config.toml → nros.toml}` (88) + `src/*.rs` `include_str!` (86)
- `docs/reference/nros-toml.md`, `book/src/reference/nros-toml.md`
- `book/src/user-guide/configuration.md`
- `docs/reference/nros-toml-schema.md` (new JSON schema)

---

## Acceptance criteria

- A project carries at most: `Cargo.toml` **or** `CMakeLists.txt` (build, by
  language), `package.xml` (msg deps), `.cargo/config.toml` (Rust dep injection
  only), and **one** `nros.toml` (all nano-ros config). No `config.toml`.
- `nros.toml` is byte-identical in shape across Rust/C/C++ projects for the same
  node setup.
- `nros build` on a project with `node.rmw = "cyclonedds"` produces the same binary
  as `cmake -DNROS_RMW=cyclonedds` by hand; with `node.rmw = "zenoh"` the same as
  `cargo build --features rmw-zenoh`.
- An embedded example (e.g. `qemu-arm-baremetal/rust/talker`) boots with its network
  + scheduling read from `nros.toml` via `include_str!`, no `config.toml` present.
- `.cargo/config.toml` in every example contains only `[patch.crates-io]`.
- The existing multi-backend bridge demo still runs from the same `nros.toml`
  (multi-`[[node]]` + `[[bridge]]`) unchanged.

## Notes

- **Build is never owned by `nros.toml`.** Cargo/CMake/Kconfig stay the canonical
  build drivers; `nros build` is a thin front that *derives* their invocation from
  `nros.toml`. Raw `cargo build` / `cmake --build` always work standalone.
- **Migration is the bulk of the risk:** 88 example configs + 86 `include_str!`
  sites + 8 board parsers + 5 build.rs. Stage it: schema/loader first (A), then
  board parsers (A.3) behind a `nros.toml`-or-`config.toml` fallback, then sweep
  examples (C), then drop the fallback.
- **Out of scope (was old Phase 116):** the package registry / `nros add` (dead with
  `find_package`; RTOS shells cover native package managers) and board-descriptor
  TOMLs (`§C` of the old phase — a board-*naming* concern, not configuration; spin
  out separately if board friction bites).

---

## History / what changed (2026-05-27)

Rescoped from "unified `nano-ros.toml` + package registry + board descriptors" to
"configuration redesign." Drivers: the new `nano-ros.toml` file + transient-emit
model (`§A`) conflicts with the copy-out-template example philosophy and duplicates
the existing `nros.toml`/`config.toml`; the package registry (`§B`) is obsolete
after Phase 140 (`find_package` removed) + Phase 139 (per-RTOS package-manager
shells); board descriptors (`§C`) are a separable board-naming concern. The
surviving, worthwhile core is consolidating the two runtime TOMLs into one universal
`nros.toml` and drawing clean file-ownership lanes.
