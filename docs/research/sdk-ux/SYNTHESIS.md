# nano-ros C/C++ SDK UX — Synthesis & Roadmap

Date: 2026-05-04
Source reports:
- `micro-ros.md` — direct competitor (rclc, micro_ros_setup, freertos_apps, Arduino, Zephyr module)
- `zephyr-and-esp-idf.md` — vendor RTOS SDKs with mature CLIs (`west`, `idf.py`)
- `platformio-arduino-mbed.md` — cross-RTOS unified UX (`platformio.ini`, FQBN, mbed-tools)

This file consolidates findings across the three reports into a single ranked roadmap.

---

## 1. Convergent findings (cited by ≥2 reports)

| Finding | Reports | Severity |
|---|---|---|
| **No project scaffolder.** Users `cp -r` an example and hand-edit ~4 files. | A,B,C | High |
| **Config sprawl.** 4–5 parallel knobs (`Cargo.toml` features + `.cargo/config.toml` `[patch.crates-io]` + `config.toml` + `prj.conf` + CMake `-D APP_*` macros + Kconfig `CONFIG_NROS_*`). No single source of truth. | A,B,C | High |
| **No `run` verb.** `idf.py build flash monitor` chained loop is the addictive killer feature. nano-ros has it only on ESP32 via `cargo run` (espflash). FreeRTOS-on-QEMU, Zephyr `native_sim`, ThreadX-on-QEMU all require hand-rolled `qemu-system-…`. | A,B,C | High |
| **`[patch.crates-io]` leaks into every Rust example.** Templates break the moment user copies them out of tree. Root cause: nros core crates not on crates.io. | A,B | High |
| **No precompiled distribution.** Single channel = "clone the monorepo + `just setup`". micro-ROS has 10 channels (Arduino zip, ESP-IDF component, west module, PIO library, STM32CubeMX, Renesas, Docker agent…). | A,C | High |
| **No 3rd-party C/C++ package manager.** Cargo handles Rust users. C/C++ users beyond `find_package(NanoRos)` have no registry. Phase 23 (Arduino lib) needs this anyway. | A,C | Med |
| **Justfile is maintainer CLI, not user CLI.** 60+ recipes; new users see internal ones (`_cmake-cargo-stale-guard`, `install-local-posix`, `refresh-cmake-cargo`). | B,C | Med |
| **Per-board crate per chip.** PlatformIO/Arduino/Mbed = JSON/FQBN string. nano-ros = full Rust crate (~300+ lines). Porting STM32F4 → STM32F7 takes a new crate. | A,C | Med |
| **Platform leaks in user main.** FreeRTOS uses `app_main(void)` + CMake `-D APP_ZENOH_LOCATOR`; Zephyr uses `int main(void)` + `zpico_zephyr_wait_network()` + Kconfig string. Different across RTOSes. | A,B | Med |
| **C/C++ publish is two-step.** rclc: `rcl_publish(&pub, &msg)`. nros-c: `serialize → check → publish_raw`. Doubles every publish-site LOC. | A | Med |
| **No agent Docker images.** micro-ROS: `docker run microros/micro-ros-agent:kilted serial …`. nano-ros: user builds `zenohd` and `MicroXRCEAgent` themselves. | A | Med |
| **`nano-ros-workspace` sibling-directory surprise** for Zephyr Rust. Symlinked back as `zephyr-workspace/`. New users hit it immediately. | B | Low |
| **`rustapp` package-name requirement** (zephyr-lang-rust constraint). Documented as code comment, not in book. | B | Low |
| **No curated common-msgs bundle.** Users `cargo nano-ros generate-*` per package. micro-ROS Arduino ships 100+ pre-baked types. | A | Low |
| **No size profiling.** `idf.py size-components` lists every component's `.text/.data/.bss` cost. nano-ros has `scripts/stack-analysis*.sh` undocumented. | B | Low |

---

## 2. Strengths to preserve

All three reports agree:

- **Rust core, Cargo-native.** Memory safety + true `no_std`. Don't trade for colcon-meta workspace.
- **Compile-time bounded executor** (`NROS_EXECUTOR_MAX_CBS` + arena). Bound enforceable; no surprise heap.
- **CMake `find_package(NanoRos)` + `nros_generate_interfaces()` discipline.** Phase 75 relocatable install solves consumption.
- **Verification (Verus/Kani).** micro-ROS has none.
- **Self-contained example trees.** Good for IDE / debugger / license. (Caveat: today they cheat via `../../../cmake/freertos-support.cmake` escapes — see UX-2 below.)
- **Per-platform debugging guides** (LAN9118, QEMU icount, panic decoding). Differentiator.
- **Strong RMW abstraction.** Three orthogonal axes (RMW × platform × ROS edition) with mutual-exclusion checks.

The proposals below change the **meta layer** (project gen, CLI, distribution, transport plug-in). None require rewriting the Rust core.

---

## 3. Unified prioritized roadmap

Each item maps back to source-report proposals (e.g. `[A.P1, B.P1, C.P1]`). Effort: S = ≤1 week, M = 2–6 weeks, L = 2+ months.

### Tier 0 — Quick wins (do first, weeks)

| # | Item | Effort | Risk | Maps to |
|---|------|--------|------|---------|
| **UX-1** | **Surface `cargo nano-ros init/new` from `just` and book.** Already exists in `packages/codegen/cargo-nano-ros`, undocumented. Add `just new <name> --platform … --rmw … --lang …`. Templates emit working `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` + `prj.conf` + `config.toml` + `src/main.{rs,c,cpp}` + `README.md`. | S | Low | A.P1, B.P1, C.P1 |
| **UX-2** | **Fix self-contained example rule.** Today `examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:5` does `include(.../../../cmake/freertos-support.cmake)` — hard escape. Move shared cmake into `find_package(NanoRosFreeRTOS)` (already partially there in `cmake/`). Examples become true copy-out templates. | S–M | Low | A.P3 |
| **UX-3** | **Typed publish in C/C++** — generate `nros_publish_<type>(&pub, &msg)` + `_Generic` macro `NROS_PUBLISH(pub, msg)`. Pure codegen change in `packages/codegen/rosidl-c/`. Keeps raw path for zero-copy. | S | Low | A.P2 |
| **UX-4** | **Default error-check macros** `<nros/check.h>` with `NROS_CHECK(...)`/`NROS_SOFTCHECK(...)`. Replace 4-line `if(ret != …) printf+cleanup+return` boilerplate. Sweep examples. | S | None | A.P12 |
| **UX-5** | **Curate `just --list`.** Add `[private]` attribute to internal recipes. New `just --list-all` for maintainers. | S | None | B.P11 |
| **UX-6** | **Top-level `just doctor`** consolidating per-platform doctors. Colour table + JSON summary + non-zero exit on P1 platform broken. | S | None | B.P6, C.P8 |
| **UX-7** | **Per-example `README.md`** auto-emitted by `cargo nano-ros init`. Backfill existing examples via script. | S | None | B.P12 |
| **UX-8** | **Single config-source.** Auto-generate `nros_app_config.h` from `config.toml` (or Kconfig) at build time. Expose `nros_app_config_t` from `<nros/app_config.h>`. User writes `nros_support_init(&support, cfg.zenoh.locator, cfg.zenoh.domain_id)` instead of `APP_ZENOH_LOCATOR` macros. Zephyr backend reads Kconfig into same struct. | S | Low | A.P8 |
| **UX-9** | **Drop `nano-ros-workspace` sibling-directory surprise.** Default to `west init -l . zephyr-workspace/` so workspace is gitignored subdir, not sibling. Keep env override. | S | Low | B.P9 |
| **UX-10** | **Docker images for agents.** Publish `ghcr.io/newslabntu/nano-ros-zenoh-router:<tag>` and `…/nano-ros-xrce-agent:<tag>`. Mention `docker run` verbatim in every getting-started page. | S | Low | A.P7 |
| **UX-11** | **`board list` enumeration.** `cargo nano-ros board list` lists every board in `packages/boards/` with chip/RAM/flash/supported-RMWs. | S | None | C.P9 |

### Tier 1 — Core UX (next, ~quarter)

| # | Item | Effort | Risk | Maps to |
|---|------|--------|------|---------|
| **UX-20** | **`nano-ros run -e <env>` / `just run <example>` chained verb.** Mimics `idf.py build flash monitor`. Picks runner per board family (qemu / espflash / openocd / native). Reuses existing `nros-tests/src/qemu.rs`, `tests/run-test.sh`, espflash runner. Phase rollout: POSIX → QEMU → ESP32 → real ARM. | M | Med | A.P6, B.P2, C.P6 |
| **UX-21** | **Unified `nros_app_main(int argc, char **argv)` entry across RTOSes.** Per-platform glue calls it after network-wait, executor-init, board-init, RTOS-task-create. Backward-compat shims for `app_main`/`main` with deprecation. | M | Low | A.P9 |
| **UX-22** | **Runtime transport vtable for nros-c.** `nros_set_custom_transport(ops)` with 4 fn-ptrs (open/close/write/read), backed by `zpico-platform-custom` + XRCE custom transport profile. Same trait as Phase 79 platform abstraction. | M | Med | A.P5 |
| **UX-23** | **`nano-ros monitor`** that decodes panics. Reuse `defmt-print`/`probe-rs` for ARM; QEMU pipe → `addr2line`. Pairs with Phase 88 (`nros-log`). | M | Low | B.P8, C.P7 |
| **UX-24** | **Curated `nros-msgs-common` bundle.** Pre-generate `std_msgs`, `geometry_msgs`, `sensor_msgs`, `nav_msgs`, `tf2_msgs`, `lifecycle_msgs`, `action_msgs`, `example_interfaces` for Rust+C+C++. Users add to `target_link_libraries`; codegen still flows for custom. | M | Low | A.P10 |
| **UX-25** | **`sample.yaml` per example.** Adopt Zephyr Twister format verbatim — `description`, `name`, `tags`, `integration_platforms`, `harness_config.regex`. `just test-all` reads it for stdout match. | S–M | Low | B.P7 |
| **UX-26** | **Drop `rustapp` rename.** Wrap `rust_cargo_application()` in `nros_rust_application(<name>)` macro that aliases the static lib internally, OR upstream relaxation to `zephyr-lang-rust`. | M | Med | B.P5 |

### Tier 2 — Distribution (months, strategic)

| # | Item | Effort | Risk | Maps to |
|---|------|--------|------|---------|
| **UX-30** | **Publish nros core crates to crates.io.** `nros`, `nros-core`, `nros-serdes`, `nros-node`, `nros-rmw`, `nros-rmw-zenoh`, `zpico-sys`. Examples depend on `nros = "0.1"`; `[patch.crates-io]` opt-in for nros-tree dev only. **Single highest-leverage move** — kills `[patch]`-pollution, `rustapp` rename, sibling-workspace dance in one stroke. | L | High (semver pressure) | A.P4, B.P4 |
| **UX-31** | **Phase 23: Arduino library** with umbrella `<NanoROS.h>`. Concrete `.ino` API target written out (see `platformio-arduino-mbed.md` §3). Ship `NanoROS-1.0.0.zip` to Arduino Library Registry. v1: one arch + serial. | M (v1) → L | Low (micro-ROS Arduino is precedent) | A.P4, C.P3 |
| **UX-32** | **ESP-IDF component.** Build `nros-c` precompiled for `esp32`/`esp32c3`/`esp32s3`. Publish as `aeon-iot/nros` on https://components.espressif.com. Pairs with UX-30. | L | Med (IDF maintenance burden) | A.P4, B.P10 |
| **UX-33** | **Templates as separate repo `nano-ros-templates/`** (or `templates/` zip artifacts via CI). Each template builds *outside* the workspace using only `find_package(NanoRos)`/`find_package(NanoRosFreeRTOS)`. In-tree examples re-render via `-DNROS_TEMPLATE_DIR=…`. | M | Med (Phase 75 helps) | A.P3 |

### Tier 3 — Long-term north star

| # | Item | Effort | Risk | Maps to |
|---|------|--------|------|---------|
| **UX-40** | **`nano-ros.toml` as single user-edited config.** PlatformIO-style env matrix. `cargo nano-ros build` reads it, emits transient cargo + cmake + prj.conf into `target/<env>/`. Phased: (a) read-only emit alongside hand-written; (b) opt-in switch flips to generated; (c) docs/templates make it canonical. Schedule **after Phase 100** so descriptor schema covers IVC-only platforms. | L | Med | C.P2 |
| **UX-41** | **`nano-ros add <pkg>` + minimal package registry.** GitHub repo `nano-ros/registry` with TOML index pointing at git-tagged releases. Resolves name → URL+version. Cargo dep / cmake FetchContent. Bootstrap with in-tree packages (`nros-rcl-interfaces`, `nros-lifecycle-msgs`, msg families). | L | Low (additive) | C.P4 |
| **UX-42** | **Board descriptor TOML** decouples board name from runtime crate. `packages/boards/registry/<name>.toml` with `chip`, `flash_kb`, `ram_kb`, `runtime_crate`, `runtime_features`, `memory_x`, `default_priorities`. New STM32F7 in same family = one TOML + memory.x, not new crate. v1 limited to "same-family" derivations. | M | Med (chip quirks) | C.P5 |
| **UX-43** | **Doc consolidation.** Once UX-1, UX-20, UX-31, UX-40 land, replace 1305 lines of per-RTOS getting-started with one "Getting Started" (5 commands) + per-RTOS appendices for HW-specific notes. | S | Low (gated on tooling) | C.P10 |

---

## 4. Recommended sequencing

```
Now (Tier 0):           UX-1 ──┐
                        UX-2 ──┤
                        UX-3 ──┼── ship as a "v0.X user-experience pass"
                        UX-4 ──┤   ~4-6 weeks total, all S, no risk
                        UX-5..11┘

Next quarter (Tier 1):  UX-20 (run verb)         ──┐
                        UX-21 (nros_app_main)     ──┼── "v0.Y unified front-end"
                        UX-23 (monitor)           ──┘
                        UX-22, 24, 25, 26 in parallel

Strategic (Tier 2):     UX-30 (crates.io publish) ──── unblocks UX-31..33
                        UX-31 (Arduino lib = Phase 23)
                        UX-32 (IDF component)

Long-term (Tier 3):     UX-40 (nano-ros.toml) — north star, after Phase 100
                        UX-41 (registry)
                        UX-42 (board descriptors)
                        UX-43 (doc consolidation)
```

**Single highest-leverage move: UX-30 (crates.io publish).** It deletes an entire class of foot-guns and unblocks every Tier-2 distribution channel.

**Single fastest visible win: UX-1 (`just new`).** One week, ships a "type one command, get a working project" experience that nano-ros currently lacks entirely.

---

## 5. What's explicitly out of scope

- Replacing the Rust core. All proposals are surface-layer.
- Dropping `find_package(NanoRos)` discipline. UX-2 strengthens it.
- Adopting colcon-meta-workspace. micro-ROS' biggest pain point per multiple reports; nano-ros's avoidance is a strength.
- Forcing Kconfig on non-Zephyr RTOSes. UX-8 generates a unified config struct from whichever native config mechanism (TOML, Kconfig, future `nano-ros.toml`).

---

## Appendix — Source-report mapping table

| UX-# | Source proposal(s) | Source file |
|------|--------------------|-------------|
| UX-1 | A.P1, B.P1, C.P1 | all 3 |
| UX-2 | A.P3 | micro-ros.md §11 |
| UX-3 | A.P2 | micro-ros.md §11 |
| UX-4 | A.P12 | micro-ros.md §11 |
| UX-5 | B.P11 | zephyr-and-esp-idf.md §"P3" |
| UX-6 | B.P6, C.P8 | zephyr-and-esp-idf.md, platformio… |
| UX-7 | B.P12 | zephyr-and-esp-idf.md §"P3" |
| UX-8 | A.P8 | micro-ros.md §11 |
| UX-9 | B.P9 | zephyr-and-esp-idf.md §"P3" |
| UX-10 | A.P7 | micro-ros.md §11 |
| UX-11 | C.P9 | platformio-arduino-mbed.md §7 |
| UX-20 | A.P6, B.P2, C.P6 | all 3 |
| UX-21 | A.P9 | micro-ros.md §11 |
| UX-22 | A.P5 | micro-ros.md §11 |
| UX-23 | B.P8, C.P7 | zephyr-and-esp-idf.md, platformio… |
| UX-24 | A.P10 | micro-ros.md §11 |
| UX-25 | B.P7 | zephyr-and-esp-idf.md §"P3" |
| UX-26 | B.P5 | zephyr-and-esp-idf.md §"P2" |
| UX-30 | A.P4, B.P4 | micro-ros.md, zephyr-and-esp-idf.md |
| UX-31 | A.P4, C.P3 | micro-ros.md, platformio-arduino-mbed.md |
| UX-32 | A.P4, B.P10 | micro-ros.md, zephyr-and-esp-idf.md |
| UX-33 | A.P3 | micro-ros.md §11 |
| UX-40 | C.P2 | platformio-arduino-mbed.md §7 |
| UX-41 | C.P4 | platformio-arduino-mbed.md §7 |
| UX-42 | C.P5 | platformio-arduino-mbed.md §7 |
| UX-43 | C.P10 | platformio-arduino-mbed.md §7 |
