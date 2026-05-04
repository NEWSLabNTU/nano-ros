# Phase 116: `nano-ros.toml` Unified Config + Package Registry + Board Descriptors

**Goal:** Collapse the four-files-per-example config sprawl (`Cargo.toml` + `.cargo/config.toml` + `config.toml` + `CMakeLists.txt` [+ `prj.conf` on Zephyr]) into a single user-edited `nano-ros.toml` with a PlatformIO-style env matrix, ship a curated package registry so C/C++ users can declare third-party deps, and decouple board names from runtime crates so adding a new STM32 part is one TOML file instead of one Rust crate.

**Status:** Not Started
**Priority:** Medium (long-term north star)
**Depends on:** Phase 100 (Orin SPE — schema must cover IVC-only platforms), Phase 111 (`nros` CLI is the consumer), Phase 112 (typed config struct from §D is superseded by this phase), Phase 75 (relocatable install)
**Related:** `docs/research/sdk-ux/SYNTHESIS.md` UX-40, UX-41, UX-42, UX-8

---

## Overview

The cross-RTOS UX research identified three convergent long-term improvements that all hinge on a single declarative project file:

1. **Config sprawl.** PlatformIO collapses the build matrix into one `platformio.ini`. nano-ros has 4–5 parallel knobs per example. A `nano-ros.toml` analogue restores the single-source-of-truth.
2. **No 3rd-party C/C++ package manager.** PlatformIO `lib_deps`, ESP-IDF Component Registry, Arduino Library Registry all converge on the same shape: declarative manifest, named versions, transitive resolution. nano-ros has nothing for C/C++ users beyond `find_package(NanoRos)`.
3. **Per-board crate per chip.** PIO/Arduino/Mbed = JSON / FQBN string. nano-ros = full Rust crate. Adding STM32F7 to an STM32F4 codebase = new crate. A board descriptor TOML on top of family runtime crates flattens this.

These three are co-designed because the config schema, the registry, and the board names share vocabulary.

---

## Architecture

### A. `nano-ros.toml`

One file per project. Read by `nros build`, `nros run`, `nros generate`. Emits transient `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` + `prj.conf` + (legacy) `config.toml` into `target/<env>/` for the underlying tools. User never edits the generated files.

```toml
# nano-ros.toml
[project]
name        = "talker"
language    = "rust"             # or "c", "cpp"
ros_edition = "humble"

[[env]]
name      = "freertos-mps2"
board     = "mps2-an385-freertos"
rmw       = "zenoh"
transport = ["tcp", "udp-unicast"]

[env.network]
ip      = "10.0.2.20"
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
netmask = "255.255.255.0"

[env.zenoh]
locator   = "tcp/10.0.2.2:7451"
domain_id = 0

[env.scheduling]
app_priority         = 12
zenoh_read_priority  = 16
zenoh_lease_priority = 14
poll_priority        = 10
app_stack_bytes      = 65536

[[env]]
name      = "zephyr-native-sim"
board     = "native-sim"
rmw       = "zenoh"
transport = ["tcp"]
zenoh     = { locator = "tcp/127.0.0.1:7456" }

[interfaces]
generate = ["std_msgs", "geometry_msgs"]
# uses nros-msgs-common bundle by default; lists here mean "also (re)generate locally"

[dependencies]
# C/C++ user libraries — see §B
"acme/nros-vision-msgs" = "^0.3"
```

Schema validation: `nros config check` runs against a JSON schema bundled with the binary.

### B. Package registry + `nros add`

GitHub repo `newslabntu/nano-ros-registry` with TOML index files:

```
registry/
├── nros-vision-msgs/
│   ├── 0.3.0.toml
│   └── 0.3.1.toml
├── nros-imu-fusion/
│   └── 0.1.0.toml
└── ...
```

Each index file:

```toml
[package]
name        = "nros-vision-msgs"
version     = "0.3.1"
description = "Vision message types for nano-ros"
license     = "Apache-2.0"
homepage    = "https://github.com/foo/nros-vision-msgs"

[source]
git = "https://github.com/foo/nros-vision-msgs"
tag = "v0.3.1"

[provides]
rust_crate = "nros-vision-msgs"      # for cargo deps
cmake_target = "VisionMsgs::nros"    # for find_package consumers
```

`nros add nros-vision-msgs` resolves name → URL+version, updates `nano-ros.toml`'s `[dependencies]`, runs `cargo add` for Rust users, runs cmake `FetchContent` glue for C/C++ users.

Bootstrap with the existing in-tree packages (`nros-rcl-interfaces`, `nros-lifecycle-msgs`, ROS message families) as the v1 registry contents.

### C. Board descriptor TOML

`packages/boards/registry/<board-name>.toml` decouples board name from runtime crate:

```toml
# packages/boards/registry/stm32f429zi-nucleo.toml
display_name = "STM32 NUCLEO-F429ZI"
chip         = "stm32f429zi"
flash_kb     = 2048
ram_kb       = 256
runtime_crate     = "nros-board-stm32f4"
runtime_features  = ["stm32f429zi", "ethernet"]
memory_x          = "memory/stm32f429zi.x"

[default_priorities]
app          = 12
zenoh_read   = 16
zenoh_lease  = 14
poll         = 10

[supported]
rmw       = ["zenoh", "xrce"]
transport = ["tcp", "udp-unicast"]
```

`nano-ros.toml`'s `board = "stm32f429zi-nucleo"` resolves through the registry. Adding STM32F7 in the same family = drop a TOML file + a `memory.x`. No new crate. New chip families that need fundamentally different HAL still get a runtime crate, but the user-facing knob remains a string.

`nros board list` (already in Phase 111) reads from the registry.

### D. Migration

Two-phase rollout:

1. **Read-only emit.** `nros build` reads `nano-ros.toml` and emits transient configs to `target/<env>/`. Existing per-example hand-written `Cargo.toml` + `.cargo/config.toml` + `config.toml` + `CMakeLists.txt` keep working in parallel — users who don't migrate notice nothing.
2. **Opt-in switch.** `nano-ros.toml` gains a `[generate] managed = true` knob. When set, `nros build` rewrites the generated artifacts on every invocation; user-edited copies become read-only stubs that delegate.
3. **Default flip** at a major release. New examples generated by `nros new` default to managed. Existing examples migrated by tooling.

---

## Work Items

### A — `nano-ros.toml`

- [ ] **116.A.1** Schema in `docs/reference/nano-ros-toml-schema.md`. JSON-schema-validatable.
- [ ] **116.A.2** Parser crate `nano-ros-config` (workspace member). Used by `nros-cli-core` and codegen.
- [ ] **116.A.3** `nros build` (Phase 111 hookup) — read-only emit pass.
- [ ] **116.A.4** `nros run --env <name>` selects from `[[env]]` matrix.
- [ ] **116.A.5** Schema covers Phase 100 IVC-only platforms (no transport block, IVC mailboxes instead).
- [ ] **116.A.6** Migration tool `nros migrate` — converts existing `Cargo.toml + .cargo/config.toml + config.toml` into `nano-ros.toml`.
- [ ] **116.A.7** Opt-in `[generate] managed = true` mode that rewrites generated configs.
- [ ] **116.A.8** `book/src/user-guide/nano-ros-toml.md` (new). Replaces today's `book/src/user-guide/configuration.md`.

### B — Package registry + `nros add`

- [ ] **116.B.1** Stand up `newslabntu/nano-ros-registry` repo with TOML index format documented.
- [ ] **116.B.2** Bootstrap with in-tree packages: `nros-rcl-interfaces`, `nros-lifecycle-msgs`, message families.
- [ ] **116.B.3** `nros add <name>[@<version>]` resolves and updates `nano-ros.toml`.
- [ ] **116.B.4** Cargo-side wiring: emit `cargo add` for Rust deps.
- [ ] **116.B.5** CMake-side wiring: emit `FetchContent_Declare` + `find_package` glue.
- [ ] **116.B.6** Versioning: semver constraints (`^0.3`, `=0.3.1`, etc.).
- [ ] **116.B.7** Offline cache `~/.nros/registry-cache/` for air-gapped builds.
- [ ] **116.B.8** Document publish process for 3rd-party packages (PR to registry repo).

### C — Board descriptor TOML

- [ ] **116.C.1** Board descriptor schema; bundled JSON schema.
- [ ] **116.C.2** Author descriptors for all 9 existing boards (`mps2-an385`, `mps2-an385-freertos`, `stm32f4`, `esp32`, `esp32-qemu`, `nuttx-qemu-arm`, `threadx-linux`, `threadx-qemu-riscv64`, `orin-spe`).
- [ ] **116.C.3** `nros board list` reads from `packages/boards/registry/`.
- [ ] **116.C.4** `nros build` resolves `nano-ros.toml` `board = "..."` to the right runtime crate + features + memory layout.
- [ ] **116.C.5** Add an STM32F7 NUCLEO descriptor as the proof-point: one TOML + one `memory.x`, no new crate.
- [ ] **116.C.6** `book/src/porting/custom-board.md` rewritten — JSON-first, crate-only when family is new.

**Files:**
- `nano-ros.toml` (new schema + parser)
- `packages/codegen/packages/nano-ros-config/` (new crate)
- `packages/boards/registry/*.toml` (new — 9 + new ones)
- `docs/reference/nano-ros-toml-schema.md` (new)
- `docs/reference/board-descriptor-schema.md` (new)
- `book/src/user-guide/nano-ros-toml.md` (new)
- `book/src/porting/custom-board.md` (rewrite)
- `book/src/user-guide/dependencies.md` (new — package registry doc)
- External: `newslabntu/nano-ros-registry` (new repo)

---

## Acceptance criteria

- A new project created via `nros new` ships exactly one user-edited file: `nano-ros.toml`. All other build artifacts are generated.
- `nros build --env freertos-mps2` produces the same binary as the current per-example `cmake -B build && cmake --build build` flow on the migrated examples.
- `nros add nros-rcl-interfaces` works on a fresh project: updates `nano-ros.toml`, fetches the source, builds successfully.
- An STM32F7 NUCLEO board is added by writing one TOML + one `memory.x` (no new crate) and runs an existing example.
- `nros migrate` converts every existing `examples/**/` to `nano-ros.toml` form; the converted examples build identically.
- The Phase 100 Orin-SPE example fits the schema (IVC-only env block, no network).

## Notes

- **Sequencing:** This phase is a north-star; do not start until Phase 100 is complete (so the schema covers IVC-only platforms from day one) and Phase 111 has shipped (so the consumer CLI is real).
- **Risk:** Hand-written cmake + Cargo configs are battle-tested; generated ones aren't. Run for at least one release in read-only-emit mode before flipping defaults.
- **Risk:** Registry curation cost. v1 = curated, PR-based; v2 = open submission with review. Set expectations in the docs.
- **Out of scope:** binary registry hosting (cargo crates.io / IDF Component Registry / Arduino registry handle the actual binaries; ours is a pointer registry).
- **Open question:** does `nano-ros.toml` replace `package.xml` or live alongside? Likely alongside — `package.xml` is the ROS-2 interop key and Phase 78 (colcon-nano-ros) needs it.
