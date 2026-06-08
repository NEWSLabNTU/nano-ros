---
rfc: 0003
title: "rtos-integration-pattern"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

## 1. The universal pattern

Hypothesis mostly right, one refinement. Vendor SDK keep native build tool — west, make+Kconfig, cmake, idf.py, pio. Always. nano-ros never replace. nano-ros instead **plug into vendor's "external library / external module / component" hook** — Zephyr module, NuttX `apps/external/`, ESP-IDF component, PX4 `EXTERNAL_MODULES`, ThreadX `add_subdirectory`, PIO `library.json`. Adapter shim expose nano-ros runtime + user components as vendor-native artifact.

Bringup pkg + `system.toml` + `launch/*.xml` are **host-time inputs**, never reach device. `nros codegen system` read them at vendor's **configure phase** (cmake configure, NuttX `context::`, west cmake-time), emit `system_config.h` + `system_main.c` + per-component register stubs. Same precedent as today's `config.toml → app_config.h` baker (CLAUDE.md "Domain ID: compile-time on embedded"). Device see only baked C. Pattern proven across 5 RTOS — refines the hypothesis only by exception: **PlatformIO + PX4 lack a configure-time hook rich enough to read `system.toml` from inside vendor tool**, so codegen run **ahead** of vendor tool, emit vendor-native tree, vendor tool consume post-codegen state. Hypothesis stands.

Universal rule: **Vendor own build + link. nano-ros own (a) per-vendor adapter shim exposing runtime as vendor-native lib/component/module, and (b) host-time codegen baking `system.toml` + launch.xml → compile-time C config consumed at vendor's configure phase (or generated ahead of vendor tool for hookless vendors). The bringup pkg never reaches the device — only its baked output does.**

## 2. Embedded layering

```
+--------------------------------------------------------------+
| USER LAYER                                                   |
|   component crates (Cargo.toml + #[nros::component])         |
|   user C++ components (CMakeLists.txt + nros-cpp)            |
|   bringup pkg: system.toml + launch/*.xml                    |
+--------------------------------------------------------------+
                          |  read at host configure time
                          v
+--------------------------------------------------------------+
| PHASE 212 LAYER (host-only, never on device)                 |
|   nros plan        — resolve [[components]] x [[deploy]]     |
|   nros codegen sys — system.toml -> system_config.h +        |
|                      system_main.c + Cargo workspace stub    |
|   (no deploy verb)  — native vendor tool builds/flashes      |
|                       the baked tree (Phase 222)             |
+--------------------------------------------------------------+
                          |  emits baked C + Cargo stub
                          v
+--------------------------------------------------------------+
| ADAPTER LAYER (per-vendor shim, <=200 LoC)                   |
|   zephyr/        — Zephyr module (module.yml + CMake glue)   |
|   integrations/nuttx/    — apps/external/ Make.defs+Kconfig  |
|   integrations/nano-ros/ — ESP-IDF component                 |
|   integrations/px4/      — px4_add_module template           |
|   cmake/platform/nano-ros-threadx.cmake — direct add_subdir  |
|   (FreeRTOS: no shim - cargo+cc board crate is the adapter)  |
+--------------------------------------------------------------+
                          |  vendor-native artifact
                          v
+--------------------------------------------------------------+
| VENDOR SDK LAYER (untouched, native build tool)              |
|   west build  |  make+Kconfig  |  cmake  |  idf.py  |  pio   |
+--------------------------------------------------------------+
                          |  ELF + flash
                          v
+--------------------------------------------------------------+
| HW LAYER (board / SoC)                                       |
+--------------------------------------------------------------+
```

Arrows: user → 212 (host inputs); 212 → adapter (baked sources + Cargo stub); adapter → vendor (vendor-native target registration); vendor → HW (ELF, flash).

## 3. Per-RTOS one-liner

| RTOS | top-level build tool | adapter shape | bringup pkg lives where | bake mechanism |
|---|---|---|---|---|
| Zephyr | `west build` | `zephyr/module.yml` + `zephyr/CMakeLists.txt` (in-tree module) | host-side | `nros_system_generate()` cmake fn at configure → `system_config.h` + `system_main.c` consumed by `rust_cargo_application()` |
| NuttX | `make` + Kconfig | `integrations/nuttx/` symlinked into `apps/external/<name>/` | host-side | `Makefile`'s `context::` rule calls `NROS_CARGO_BUILD`; Kconfig choices map to Cargo features; baked header emitted into staticlib |
| FreeRTOS | `cargo` (Rust path) / cmake+Corrosion (Cyclone path) | per-board crate `build.rs` (`cc::Build`) compiles kernel + lwIP + glue | host-side | `build.rs` emits `nros_config_generated.h` from `system.toml` + Cargo features; staticlib is the firmware |
| ThreadX | `cmake` | `cmake/platform/nano-ros-threadx.cmake` + board overlay `cmake/board/nano-ros-board-<b>.cmake` (no `integrations/threadx/`) | host-side | cmake configure runs `nros codegen`; Corrosion imports component crates as staticlibs; `app_define.c` spawns one tx thread per component |
| ESP-IDF | `idf.py` | `integrations/nano-ros/` ESP-IDF component (`idf_component_register` + `Kconfig.projbuild`) | host-side | configure-time `add_subdirectory(<repo-root>)` runs `nros codegen`; `CONFIG_NROS_*` → cmake cache vars → baked header |
| PlatformIO | `pio run` | repo-root `library.json` (no `integrations/platformio/`); pre-build extra_script | host-side, **codegen runs ahead** | extra_script invokes `nros codegen` before pio's library resolver; vendor sees post-codegen tree |
| Orin SPE | `cmake` (JetPack BSP) | proposed `cmake/platform/nano-ros-orin-spe.cmake` (does not exist; blocked on `NV_SPE_FSP_DIR` license) | host-side | same cmake-configure path as ThreadX; FSP via `-D` cache var |
| PX4 | `cmake`+`make` hybrid (`px4_add_module`) | `integrations/px4/module-template/` copy-out | host-side, **codegen runs ahead** | each component = one PX4 module dir emitted by codegen; uORB↔DDS bridge at runtime; C++-only |

## 4. The deploy contract

> **Deploy model (Phase 222, decided 2026-06).** There is **no `nros deploy`
> verb** — Phase 222 deprecated `nros build`/`run`/`deploy`/`monitor`/`launch`
> (thin wrappers). `nros` is provisioner + codegen + metadata only. Embedded
> deploy is a **documented native multi-step**: `nros codegen system` bakes the
> `[deploy.<board>]` config, then the vendor's own tool builds + flashes with the
> `-D` args derived from `[deploy.<board>]` (per-RTOS args live in the book
> command-map + `just <plat>` recipes). Read the "`nros deploy <board>`" wording
> below as that three-step native sequence, not a single command.

Embedded deploy means three things universally: **(a)** `nros codegen system`
reads `system.toml` + `[deploy.<board>]` + `launch/*.xml` and emits the baked tree
under `build/<board>/`; **(b)** the **vendor tool** builds with the right args
(board, target, sdkconfig); **(c)** the **vendor tool** flashes + monitors.

```
Zephyr    1) nros codegen system --out build/<b>/nros-system
          2) west build -b <board> -d build/<b> -- -DEXTRA_CONF_FILE=prj-<rmw>.conf
          3) west flash -d build/<b>   (or west build -t run for qemu_*)

NuttX     1) nros codegen system --out build/<b>/baked-headers
          2) make -C $NUTTX_DIR -j   (apps/external/<bringup> staged by stage-external-apps.sh; cargo runs from context::)
          3) make -C $NUTTX_DIR flash   (board-specific; or qemu runner)

FreeRTOS  1) nros codegen system --out target/<triple>/baked/
          2) cargo build --release --target <triple> -p <bringup>
          3) probe-rs run --chip <mcu> target/<triple>/release/<bringup>.elf
          (Cyclone path: cmake -B build/<b> && cmake --build build/<b> && probe-rs run ...)

ThreadX   1) nros codegen system --out build/<b>/nros-system
          2) cmake -B build/<b> -DNANO_ROS_PLATFORM=threadx -DNANO_ROS_BOARD=<b> -DNROS_RMW=<rmw> && cmake --build build/<b>
          3) build/<b>/firmware   (threadx-linux: just run it; rv64-qemu: qemu-system-riscv64 ...)

ESP-IDF   1) nros codegen system --out build/<b>/nros-system  (or run from cmake configure)
          2) idf.py -B build/<b> -DIDF_TARGET=esp32c3 build
          3) idf.py -B build/<b> flash monitor

PlatformIO 1) nros codegen system --out .pio/<env>/nros-system   (pre-build extra_script)
           2) pio run -e <env>
           3) pio run -e <env> -t upload

PX4       1) nros codegen px4-modules --out src/modules/nros_<comp>/   (one dir per component)
          2) make px4_sitl_default   (or px4_fmu-v5x_default)
          3) make px4_sitl_default upload   (HW) / make px4_sitl_default jmavsim (SITL)
```

## 5. Host-side workflow

Embedded iteration loop:

1. Edit Rust/C++ component code
2. (Optional) `cargo check -p <component>` — fast feedback, no vendor tool
3. `nros plan` — emit baked configs + Cargo workspace stub under `build/<board>/`
4. native vendor tool builds the baked tree → ELF, flashes (no `nros deploy` — §4)
5. Monitor: `west monitor` / `idf.py monitor` / `probe-rs attach` / `pio device monitor` / vendor-specific

**Differ from desktop multi-node.** Desktop deploy spawns N processes (one per `[[components]]`), each its own ROS node, comms via DDS/zenoh on loopback. Embedded deploy produces **one ELF** containing all N components linked together — single-binary-multi-thread, one DDS/zenoh participant per component but sharing the address space. The bringup pkg surface is identical; the codegen lowers to wildly different targets. Domain ID + locator are **runtime env** on native (`unique_ros_domain_id()` from `NEXTEST_TEST_GLOBAL_SLOT`), **compile-time baked** on MCU (CLAUDE.md exception). Phase 212's job: hide that split behind one `system.toml`.

## 6. Per-RTOS adapter ≤200 LoC budget

| RTOS | current LoC | budget fit | notes |
|---|---|---|---|
| Zephyr | ~160 (3 new cmake files inside existing module) | comfortable | module + cargo glue already present; Phase 212 just adds `nros_system_generate.cmake` + `nros_component_link.cmake` + `Kconfig.system` |
| NuttX | ~30 per crate skeleton + existing `integrations/nuttx/` (~300 LoC total today) | strains, kind of | per-crate is tiny; integration root already exceeds 200 because of templates + extra_libs.mk plumbing. Honest: budget should be "per-crate adapter ≤200", not "whole shell ≤200" |
| FreeRTOS | 0 new shell; board crate `build.rs` ~50 LoC | trivial fit | no shell needed; `cc::Build` IS the adapter |
| ThreadX | 0 new shell; `cmake/platform/nano-ros-threadx.cmake` ~306 LoC exists | over budget if counted | platform module is shared across all boards, not a per-RTOS shim; ThreadX has NO integrations/ dir — comfortable in spirit |
| ESP-IDF | ~150 (CMakeLists.txt + Kconfig.projbuild + idf_component.yml) | comfortable | already shipped |
| PlatformIO | 0 today; ~100 estimated (library.json + extra_script.py) | comfortable | hookless vendor; codegen runs pre-build |
| Orin SPE | 0 today; ~150 estimated (single cmake platform file) | comfortable | blocked on NV_SPE_FSP_DIR license, not LoC |
| PX4 | template-only ~80 LoC, blank PX4 module skeleton | comfortable per-module; orchestration extra | per-module is fine; codegen emits N modules → cumulative grow with N |

Fallback when exceeded: **split shell into shared core + per-board overlays** (ThreadX precedent: platform module + board overlay) and count per-board only. NuttX should adopt same split — `integrations/nuttx/core/` (shared Make.defs templates, Kconfig glue) + `integrations/nuttx/<crate>/` (per-bringup, ~30 LoC). Don't paper over by raising the budget; raise the budget only when shared core is genuinely irreducible (Zephyr module CMakeLists at ~400 LoC counts as runtime, not shim).

## 7. Open questions

- **RESOLVED (2026-06):** Codegen timing — **ahead-of-vendor is the contract.**
  `nros codegen system` runs **before the native build tool**, producing the baked
  tree the vendor tool then consumes (there is no `nros deploy` orchestrator — §4
  deploy model). For hook-capable vendors (Zephyr/ESP-IDF/ThreadX/NuttX/FreeRTOS)
  the configure-time hook is kept as an **idempotent convenience** so a raw
  `west build` / `idf.py build` still works in dev — it runs the *same* codegen and
  yields the *same* tree. One contract (ahead-of-vendor), optional second trigger
  (configure-time), not two divergent codepaths.
- **OPEN:** Multi-component on FreeRTOS — one DDS/zenoh participant per component or one shared per ELF? Memory budget on Cortex-M3 likely forces shared; breaks domain-isolation semantics.
- **OPEN:** Bridge mode (`Executor::open_multi`, Phase 128) interaction with embedded — does `[[bridge]]` in `system.toml` even make sense on MCU (multi-RMW = double the link cost)?
- **OPEN:** Zephyr **snippet** vs `[deploy.<board>]` for RMW choice — pick one. Snippet is Zephyr-native + composable (`west build -S nros-cyclonedds`); `[deploy]` is portable across vendors. Today's `prj-<rmw>.conf` overlay is third option already shipped.
- **OPEN:** PX4 multi-component mapping — one PX4 module per `[[components]]` entry, or one PX4 module hosting N nano-ros components as in-process threads? Phase 115.K.4 C++-only collapse constrains the choice.
- **OPEN:** Orin SPE platform name — new `platform-orin-spe` or reuse `platform-bare-metal` with a board arch entry? `zenoh_platforms.toml` precedent for ESP32-C3 + Cortex-M3 sharing `bare-metal` suggests reuse.

## 8. Recommendations: minimum needed for Phase 212 to land on embedded

1. **`nros codegen system` subcommand** — scope: read `system.toml` + `launch/*.xml` + `[deploy.<board>]`, emit `system_config.h` + `system_main.c` + generated Cargo workspace stub under `build/<board>/nros-system/`. Justification: this IS the deploy contract on embedded; without it nothing else has anything to consume.

2. **`zephyr/cmake/nros_system_generate.cmake` + `nros_component_link.cmake`** — scope: ~150 LoC inside existing Zephyr module wrapping `nros codegen system` + multi-component `rust_cargo_application()` shape (§5 of Zephyr investigation). Justification: Zephyr is the highest-volume embedded target; the single-node path already works, multi-node is the new surface.

3. **Per-RTOS `[deploy.<board>]` schema in `nros-sdk-index.toml`** — scope: add `kind` (zephyr/nuttx/freertos/threadx/esp-idf/pio), `board`, `target` (Rust triple), `rmw`, optional `flash_cmd` + `monitor_cmd`. Justification: the native deploy step (codegen + vendor build/flash) needs one place to read the vendor-tool invocation pattern; today this is scattered across 8 `just/*.just` recipes.

4. **NuttX adapter split** — scope: refactor `integrations/nuttx/` into `core/` (shared templates) + per-bringup-pkg overlays, document `≤200 LoC per crate` budget explicitly. Justification: §6 shows the budget definition is ambiguous; codifying per-crate vs per-shell prevents future drift.

5. **PlatformIO + PX4 pre-codegen path** — scope: add `nros codegen --emit-vendor-tree <pio|px4>` that runs ahead of vendor tool and produces a complete vendor-native directory (library.json or PX4 module dirs). Justification: these two vendors lack a configure-time hook; without pre-codegen, Phase 212 silently doesn't cover them.

## 9. `std` vs `alloc` per platform — core stays `no_std`

**Decision.** Every core crate is `#![no_std]`
(`nros-core`, `nros-rmw`, `nros-serdes`, `nros-platform`, `nros-platform-cffi`,
`nros-rmw-zenoh`, `zpico-sys`, …). `nros` and `nros-node` are `#![no_std]`
too and only `extern crate std` under `#[cfg(feature = "std")]`. The `std`
Cargo feature is **opt-in** and merely *forwards* down
(`nros/std → nros-core/std + nros-node/std + …`), adding std-only conveniences
(`Clock::now()` via `SystemTime`, `spin_blocking`, `ExecutorConfig::from_env`,
`std::error::Error` impls). Per-crate surface: `docs/reference/std-alloc-requirements.md`.

**The std-vs-alloc choice is per-platform, made at the board / Entry layer —
never in core.** A platform picks it via the node-pkg feature
(`nros/std` vs `nros/alloc`) plus its board crate:

| Platform | Feature | Why |
|---|---|---|
| native (posix) | `std` | hosted |
| threadx-linux | `std` | hosted Linux simulation |
| **NuttX** | **`std`** | `*-nuttx-*` is a **std-capable POSIX Rust target** — NuttX ships a std port mapping std's unix-pal onto NuttX libc |
| freertos | `alloc` | bare embedded; RTOS heap via `pvPortMalloc`, no std port |
| esp32 (bare-metal) | `alloc` | `riscv32imc-unknown-none-elf`, no std |
| bare-metal / threadx (embedded) | `alloc` / pure `no_std` | no std port |

**Keep `std` on NuttX (rationale).** NuttX is POSIX and the Rust target is
std-capable, so std is the idiomatic, lowest-friction choice: the board layer
(`nros-board-nuttx`) uses `std::thread::sleep` / `std::io` / `std::process::exit`
rather than re-deriving thread/io/time/exit over raw libc FFI. Crucially, **the
std surface is not a deploy blocker.** The "missing" std symbols
(`pthread_cond_*`, `clock_gettime`, `getcwd`, `__errno`, `strerror_r`, `exit`, …)
are all defined in NuttX's own `libc.a`; they resolve when the NuttX deploy
links the kernel/export libs (the `apps/external/` + `libapps.a` link of §4, or a
`build.rs` that links the prebuilt `nuttx-export/libs/*.a` + `dramboot.ld`). A
standalone `cargo build` against arm-none-eabi **newlib** is the only context
where they go unresolved — a missing-link issue, not a reason to drop std. See
known-issue #18 (`docs/known-issues.md`).

**Consequence.** Do not "fix" a NuttX link failure by forcing the path to
`no_std` — going no_std only shrinks the symbol surface, it does not remove the
need to link NuttX's kernel/libc export libs (which back even `alloc`'s
`malloc`/`free` and the platform's `sem_*`/clock primitives). The platform's
std/alloc choice and the deploy's link inputs are independent axes.
