# Zephyr `west` and ESP-IDF `idf.py` — UX reference for nano-ros

**Date:** 2026-05-04
**Sources:**
`/home/aeon/repos/nano-ros/zephyr-workspace/zephyr/` (v3.7.0),
`/home/aeon/repos/nano-ros/external/esp-idf/` (master, shallow),
`/home/aeon/repos/nano-ros/external/example-application/` (Zephyr canonical project template),
`/home/aeon/repos/nano-ros/external/esp-idf-template/` (IDF canonical project template).

---

## Executive summary

- **`west` and `idf.py` are *user-facing CLIs* with one verb per developer intent** (`build`, `flash`, `monitor`, `menuconfig`, `create-project`, `size`). nano-ros's `just` is a *developer-facing* CLI for the nano-ros maintainers — it surfaces internal recipes (`build-test-fixtures`, `_cmake-cargo-stale-guard`, `install-local-posix`) that downstream users never want to know about.
- **Both reference SDKs ship a project-template repository** (`example-application`, `esp-idf-template`) plus a `create-project` command (`west init`, `idf.py create-project`). nano-ros has `cargo nano-ros new` (in `packages/codegen/packages/cargo-nano-ros/src/main.rs`), but it is undocumented and not surfaced from `just`. There is no equivalent of `west init` that bootstraps an entire nano-ros workspace from a manifest URL.
- **Configuration converges on Kconfig** in both SDKs, with a single interactive entry point (`menuconfig`/`guiconfig`) and one declarative file (`prj.conf` / `sdkconfig.defaults`). nano-ros today scatters config across `.cargo/config.toml`, Cargo `[features]`, top-level `cmake -D…` flags, per-example `prj.conf`, per-example `config.toml` parsed by `nano_ros_read_config()`, and `package.xml`.
- **`idf.py build flash monitor` is the addictive loop**: a single chained command goes "edit C code → see UART output on the device" without leaving the shell. nano-ros has `cargo run` for ESP32 (via `espflash flash --monitor` runner), but no equivalent for FreeRTOS-on-QEMU, ThreadX-on-QEMU, Zephyr-on-`native_sim`, or any C/C++ example. Users have to compose `just <plat> build` + a hand-crafted `qemu-system-…` line.
- **Module/component shipping is solved by `module.yml` (Zephyr) and `idf_component.yml` + the IDF Component Registry (Espressif).** nano-ros already integrates as a Zephyr module (good), but the user-side ergonomics (forcing `[patch.crates-io]` into every example's `.cargo/config.toml`, plus a "package name must be `rustapp`" rule in `examples/zephyr/rust/zenoh/talker/Cargo.toml`) leak nano-ros internals into user projects. There is no analogous IDF component manifest, so a hypothetical "nano-ros for ESP-IDF" user has nothing to depend on.

---

## What `west` does that nano-ros's `just` does not

| Intent | `west` | nano-ros `just` | Gap |
|---|---|---|---|
| Bootstrap a multi-repo workspace | `west init -m <url>` | `just zephyr setup` (Zephyr-only, hard-codes Zephyr layout via `scripts/zephyr/setup.sh`) | No equivalent for "give me a fresh nano-ros workspace pointing at backend X, board Y". |
| Update vendored deps | `west update` | manual `git submodule update` or rerun `just <plat> setup` | nano-ros has no manifest-driven update. SDKs live in `third-party/` and are gitignored in some places, submoduled in others. |
| List workspace state | `west list`, `west status`, `west diff` | none | A `just status` listing all SDK / submodule revisions would help diagnostics. |
| Configure a project | `west build -t menuconfig` (or `west build -t guiconfig`) | `cmake --build … --target menuconfig` for the C examples *only when* `CONFIG_NROS=y` is already in `prj.conf`; no Rust path | Rust users have no interactive way to flip features. |
| Build with cached generator | `west build -b <board> -d <dir> -p auto <app>` | `west build …` is what `just zephyr build` literally invokes; for non-Zephyr boards there is `cargo build` + custom cmake | Each platform has its own incantation; no unified `nros build -b <board> <app>`. |
| Flash | `west flash` (auto-discovers runner from `board.cmake`) | none for QEMU, none for STM32F4/MPS2, only `cargo run` (espflash) for ESP32 | Each board would need a runner registry. |
| Debug | `west debug` (spawns OpenOCD/JLinkGDB + GDB; per-board defaults) | manual `qemu … -gdb tcp::1234` + hand-launched `gdb-multiarch` | nano-ros has `scripts/debug/` helpers but no top-level verb. |
| Monitor / serial console | `west espressif monitor`, board-specific extensions | none | Same as flash — would need per-platform runners. |
| Run on simulator | board-cmake recognises `native_sim` and `qemu_*` boards | `just qemu run-baremetal-talker`, custom-named recipes per platform | west generalises "run after build" for any sim board. |
| Test runner | `west twister --test sample.basic.helloworld -p native_sim` | `just test`, `just zephyr test`, `cargo nextest run` | Twister is a *board × test-spec matrix* runner with `sample.yaml` describing what passes. nano-ros's nextest tests don't carry per-board metadata. |
| Multi-image build | `west build --sysbuild …` | none (each example is an island) | Bootloader+app composition is a recurring need (PX4, secure boot). |
| Sign / package | `west sign --tool imgtool` | none | mcuboot signing for OTA paths. |
| Custom commands | `scripts/west-commands.yml` lets each module/repo register subcommands (`example_west_command.py` shows the pattern) | adding a verb = editing `justfile` and shipping `just/<thing>.just` | west's plugin model lets boards/modules add verbs without touching the central CLI. |

`west`'s magic: a single command — `west build -b <board> <app>` — picks up `module.yml` from every project in `west.yml`, runs cmake with the right module roots, and is identical for every board. nano-ros's `just` is intentionally narrower and still stitches per-platform recipes together (`just/zephyr.just`, `just/freertos.just`, `just/esp32.just`), each with its own setup/build/test/run conventions.

---

## What `idf.py` does that nano-ros's `just` does not

`idf.py` action surface (from `external/esp-idf/tools/idf_py_actions/`):

- `core_ext.py`: `all build clean fullclean reconfigure menuconfig confserver app bootloader partition-table size size-components size-files set-target docs python-clean show-efuse-table`
- `serial_ext.py`: `flash app-flash bootloader-flash partition-table-flash erase-flash erase-otadata read-otadata monitor merge-bin encrypted-flash secure-encrypt-flash-data`
- `create_ext.py`: `create-project create-component`
- `qemu_ext.py`: `qemu` (run in QEMU)
- `dfu_ext.py`: `dfu dfu-flash`
- `mcp_ext.py`: `mcp` (Model-Context Protocol bridge, recent addition)
- `uf2_ext.py`: `uf2 uf2-app`
- `diag_ext.py`, `debug_ext.py`: `gdb gdbgui openocd coredump-info coredump-debug`

| Intent | `idf.py` | nano-ros `just` | Gap |
|---|---|---|---|
| New project | `idf.py create-project myapp` (copies `esp-idf-template/`) | `cargo nano-ros new <name> --platform zephyr` *exists* but is undocumented | Discoverability: not in book, not surfaced from `just`. |
| Pick a target chip | `idf.py set-target esp32c3` (re-runs cmake with right toolchain, picks `sdkconfig.defaults.esp32c3`) | hard-coded per-example `Cargo.toml` + `.cargo/config.toml` + `boards/<name>.conf` | A user wanting to retarget a working example to a different MCU has to hand-edit ~4 files. |
| Configure | `idf.py menuconfig` (Kconfig TUI on `Kconfig.projbuild` + IDF tree) | none for Rust paths; only Zephyr C examples expose Kconfig | nano-ros `[features]` aren't browsable. |
| Build → flash → run-loop | `idf.py build flash monitor` (chained, single command) | `cargo run --release` works only on ESP32 via espflash; everywhere else: separate commands | The chained verb is the killer feature — one command per save. |
| Component dependency | `idf_component.yml` + `idf.py update-dependencies` (component manager) | `cargo` for Rust deps; for C, manual `find_package(NanoRos CONFIG REQUIRED)` plus `CMAKE_PREFIX_PATH` plumbing | No C-side package manager; users reading `examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt` have to know to run `just install-local` first. |
| Size profiling | `idf.py size-components` lists every component's `.text/.data/.bss` contribution; `size-files` zooms further | `scripts/stack-analysis*.sh` exists but is undocumented and per-platform | An nros user can't easily ask "how many bytes does adding a subscription cost me?" |
| Erase / recover device | `idf.py erase-flash` | none | Real-board recovery story is delegated to `espflash`. |
| Project info | `idf.py reconfigure`, `--info`, `--list-targets` | `just doctor` (per-platform) is closest | No machine-readable project introspection. |

---

## Project bootstrap UX: zero-to-blinking-LED

### ESP-IDF (3 commands, plus `. export.sh` once per shell):

```bash
. $IDF_PATH/export.sh                          # one-time per shell
idf.py create-project --path . blink           # scaffolds CMakeLists.txt + main/
idf.py set-target esp32c3 build flash monitor  # configure + compile + flash + watch UART
```

(With the canonical `esp-idf-template` already cloned, replace step 2 with `cp -r $IDF_PATH/examples/get-started/blink .`.)

### Zephyr (3 commands):

```bash
west init -m https://github.com/zephyrproject-rtos/example-application my-app && cd my-app
west update
west build -b nucleo_f302r8 app -t flash       # one verb: build target picks runner
```

### nano-ros (honest count, Rust / Zephyr path)

```bash
just setup                              # 1 — pulls everything (Rust toolchains, west, SDK, zenohd, MicroXRCEAgent, …)
just zephyr setup                       # 2 — separate from `just setup`; downloads ~1.5 GB SDK, creates sibling workspace
source ../nano-ros-workspace/env.sh     # 3 — separate sibling workspace shape leaks
cp -r examples/zephyr/rust/zenoh/talker my-talker
# 4 — hand-edit my-talker/.cargo/config.toml so [patch.crates-io] paths still resolve from new location
# 5 — hand-edit my-talker/Cargo.toml: package name MUST stay "rustapp" for zephyr-lang-rust
# 6 — hand-edit prj.conf if you want a different RMW backend
cd ../nano-ros-workspace
west build -b native_sim/native/64 nros/my-talker -d build-talker
# 7
./build-talker/zephyr/zephyr.exe        # 8 — manual launch
# To see the talker connect to ROS, separately:
just zenohd::start                      # 9 — in another terminal
```

**Honest count: 7–9 commands, plus 3 manual file edits, plus knowledge of "must be named `rustapp`", plus knowing that `nano-ros-workspace` is a sibling directory of the repo.** This is the gap.

A C example targeting QEMU FreeRTOS is comparable: `just setup`, `just freertos setup`, `just install-local`, `cd examples/qemu-arm-freertos/c/zenoh/talker`, edit `config.toml`, `cmake -B build`, `cmake --build build`, then a hand-rolled `qemu-system-arm` line that's mirrored in `nros_tests/src/qemu.rs` but not exposed.

---

## Configuration UX: Kconfig vs cargo features vs `config.toml`

**Zephyr / IDF: one model.** Every option is in Kconfig; users edit `prj.conf` / `sdkconfig.defaults` for declarative defaults; `menuconfig` is the interactive TUI; the build system writes the final `.config` and emits `autoconf.h`/`Cargo` envs/`CONFIG_*` macros.

**nano-ros: four parallel models, all needed simultaneously.**

| Layer | Mechanism | Edit point | Visibility |
|---|---|---|---|
| Rust workspace selection | Cargo `[features]` mutual-exclusion at compile time | `Cargo.toml` of the example | `cargo build` errors |
| Zephyr-side wiring | Kconfig `CONFIG_NROS_*` (in `zephyr/Kconfig`) | `prj.conf` | `west build -t menuconfig` (Zephyr only) |
| Per-board static config (IPs, MAC, priorities) | TOML parsed by `nano_ros_read_config()` (see `examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:11`) | `config.toml` | none — silent if file missing |
| Build glue | CMake `-D` flags (`NANO_ROS_RMW=…`, `NANO_ROS_PLATFORM=…`) | `justfile` (`install-local-posix:` loop) | `just --list` |
| Cargo patches | `[patch.crates-io]` in `.cargo/config.toml` | every example | invisible until `cargo` complains |

### Honest tradeoff analysis

- **Kconfig is heavyweight.** It assumes a project tree (`Kconfig.zephyr`-rooted), Python tooling, and an `autoconf.h` writer. nano-ros wants to ship to *non-Zephyr* RTOSes (FreeRTOS, NuttX, ThreadX) where there is no Kconfig host.
- **Cargo features are first-class for Rust users.** The mutual-exclusion compile-time checks (RMW × platform × ROS edition) are clean and only cost a one-time learning curve.
- **`config.toml` is the right shape for *deployment* config** (IP, MAC, stack sizes) but is invisible to discovery — a new user reading `examples/qemu-arm-freertos/c/zenoh/talker/` has no signal that `config.toml` exists or that it must be present.
- **Verdict:** the current split between *build-time choice* (cargo features) and *deploy-time tuning* (`config.toml`) is sound, but it deserves a single discoverable entry point — see proposals 4, 5, 6 below.

---

## Module / component shipping model

### Zephyr (`example-application/zephyr/module.yml`):

```yaml
build:
  kconfig: Kconfig
  cmake: .
  settings:
    board_root: .   # provides custom boards
    dts_root: .     # provides custom devicetrees
runners:
  - file: scripts/example_runner.py   # custom flash/debug runner
```

Drop a `module.yml` in any repo, list it in `west.yml`, and it auto-integrates.

### ESP-IDF (`main/idf_component.yml`):

```yaml
dependencies:
  espressif/led_strip: "^3.0.0"
```

ESP-IDF Component Registry (https://components.espressif.com) is a public registry. `idf.py` resolves and downloads components into `managed_components/`. Components are CMake-aware (`idf_component_register(...)`) and ship Kconfig fragments.

### What nano-ros has today

- **Zephyr side: works.** `zephyr/module.yml` + `west.yml` integrate cleanly; `find_package(Zephyr ...)` + `CONFIG_NROS=y` + `nros_generate_interfaces()` is a real win.
- **Awkward bits:**
  - Every Rust example needs `[patch.crates-io]` with relative paths. See `examples/zephyr/rust/zenoh/talker/.cargo/config.toml`. This breaks the moment the user copies the example out of the tree.
  - Cargo package name must be `rustapp` to satisfy `zephyr-lang-rust`'s `rust_cargo_application()`. Documented as a code comment but not in the book.
  - `nano-ros-workspace` is a *sibling* directory of the repo, accessed via `zephyr-workspace -> ../nano-ros-workspace` symlink. Confusing for users who clone into `~/Code/nano-ros/` and end up with `~/Code/nano-ros-workspace/`.
- **Non-Zephyr platforms: no module model.** FreeRTOS, NuttX, ThreadX users get `find_package(NanoRos CONFIG REQUIRED)` after running `just install-local` — and the `_cmake-cargo-stale-guard` recipe in `justfile:142` exists precisely because corrosion's per-build cargo target tree silently serves stale `.rlib`s when source content changes without an mtime bump. Users will hit this.
- **No registry.** A third-party producing a custom platform crate (e.g. `nros-board-foobar`) has nowhere to publish it that gets it discovered by other nros users.

### What nano-ros needs to ship cleanly into both

1. **Zephyr:** publish `nros` and `nros-c`/`nros-cpp` to crates.io / a Zephyr binary blob index so users can drop the `[patch.crates-io]` block.
2. **IDF:** ship a real `idf_component.yml` for `nros-c` so an IDF user can `idf.py add-dependency aeon-iot/nros-c` and have headers/libs land. Today `examples/esp32/` uses bare `esp-hal` (no IDF), which is a strategic choice but means the IDF-using majority is locked out.
3. **Both:** a stable `nros_*` cmake API (already partially done — `nros_generate_interfaces()`, `NanoRos::NanoRos`) becomes the contract.

---

## Flash / debug / monitor: why is `idf.py build flash monitor` so addictive?

Three reasons:

1. **It's chained.** `idf.py` accepts multiple verbs in one invocation; `build flash monitor` is documented as the canonical loop. Save → arrow-up → enter.
2. **`monitor` knows about ESP-IDF panic frames.** It decodes addresses against the ELF, prints stack traces, knows GDB stub, applies colour. Generic `screen /dev/ttyUSB0 115200` is dumb pipes.
3. **The runner is auto-discovered from the build.** Set-target wrote a runner into `sdkconfig`; `idf.py flash` reads it.

`west flash` is similar: `boards/arm/<board>.cmake` declares `board_runner_args(jlink "--device=…")` and `west flash` picks the right tool.

### Can nano-ros do the same?

**Yes, and it should.** All the building blocks already exist:

- `nros_tests/src/qemu.rs` knows the `qemu-system-arm` invocation per board.
- `examples/esp32/*/.cargo/config.toml` already wires `runner = "espflash flash --monitor"`.
- `tests/run-test.sh` collects logs to `test-logs/latest/`.

Missing piece: a single CLI verb (`just run <example>`, or better, a `nros` binary) that:

1. Looks up the example's board (from `package.xml` or a `nros.toml`).
2. Builds it via the right path (cargo / cmake).
3. Picks a runner based on board family (qemu / espflash / openocd / native).
4. Streams output to the terminal with line-rate flushing.
5. On panic, decodes via the ELF.

This is doable as a Rust binary in `packages/codegen/packages/cargo-nano-ros/src/run.rs` reusing existing per-platform logic from `nros-tests`.

---

## Concrete UX improvement proposals

Each entry: **problem → reference SDK approach → proposed change → effort (S/M/L) → risk**.

### P1 (high impact, low risk)

1. **Surface `cargo nano-ros new` from `just` and from the book.**
   *Problem:* `cargo nano-ros new <name> --platform zephyr --lang rust` already works but is invisible. Users currently `cp -r` an example.
   *Reference:* `idf.py create-project`, `west init`.
   *Proposal:* Add `just new <name> --platform … --lang …` wrapper; document in `book/src/getting-started/your-first-project.md` (new page). Make the templates fix `[patch.crates-io]` to absolute paths or to crates.io.
   *Effort:* S. *Risk:* Low — wraps existing tool.

2. **Add `just run <example>` as a single verb.**
   *Problem:* Build + flash + monitor is a 3–4 command sequence that's different per platform.
   *Reference:* `idf.py build flash monitor`, `west flash`, `cargo run`.
   *Proposal:* `just run examples/qemu-arm-freertos/c/zenoh/talker` chooses the right backend (cargo / cmake), the right launcher (QEMU / espflash / openocd / native), and pipes stdout. Reuse `nros-tests` runner code.
   *Effort:* M. *Risk:* Medium — has to handle every platform; phase rollout (POSIX → QEMU → ESP32 → real ARM boards).

3. **Ship a real `module.yml` Kconfig for non-Zephyr RTOSes too — drop the `config.toml` parser.**
   *Problem:* `nano_ros_read_config()` parses a TOML file the user has to know about (`examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:11`). Discovery is zero.
   *Reference:* Zephyr's `prj.conf` is universally understood.
   *Proposal:* Extend `nros_generate_interfaces()` family to emit a `prj.conf`-style key=value file that's read by every platform's CMake. Single discoverable file per example; `just menuconfig <example>` runs Zephyr's `kconfig.py` in standalone mode.
   *Effort:* M. *Risk:* Medium — re-tooling existing examples.

### P2 (high impact, medium risk)

4. **Eliminate `[patch.crates-io]` in user examples by publishing nros core crates to crates.io.**
   *Problem:* Every Zephyr Rust example has 11 lines of `path = "../../../../../packages/core/…"` patches. Copy-out templates immediately break.
   *Reference:* `idf_component.yml` resolves from the registry; Zephyr-lang-rust's `zephyr = "0.1.0"` resolves from crates.io.
   *Proposal:* Publish `nros`, `nros-core`, `nros-serdes`, `nros-node`, `nros-rmw`, `nros-rmw-zenoh`, `zpico-sys` to crates.io with `0.1.x` versions. Examples depend on `nros = "0.1"`; `[patch.crates-io]` becomes opt-in for nros-tree development only (handled by a workspace-level `.cargo/config.toml`, not per-example).
   *Effort:* L. *Risk:* High — public-API commitment, semver pressure. Phase 23 (Arduino lib) probably needs this anyway.

5. **Drop the "Cargo package name must be `rustapp`" requirement.**
   *Problem:* `examples/zephyr/rust/zenoh/talker/Cargo.toml` is named `rustapp` — a leak from `zephyr-lang-rust`'s `rust_cargo_application()`. Users renaming the package will get a cryptic CMake failure.
   *Reference:* IDF / Zephyr-C never have this constraint.
   *Proposal:* Either upstream the relaxation to `zephyr-lang-rust`, or wrap `rust_cargo_application()` in a `nros_rust_application(<name>)` macro that aliases the static lib internally.
   *Effort:* M. *Risk:* Medium — touches an external module.

6. **Provide `just doctor` as a single top-level recipe** that runs every per-platform `doctor` and emits a JSON summary.
   *Problem:* `just doctor` exists, `just zephyr doctor` exists, `just esp32 doctor` exists, but they're separate. New users don't know which to run first.
   *Reference:* `idf.py --info`, `west status`.
   *Proposal:* Top-level `just doctor` calls all platform doctors, prints a colour table, exits non-zero if any P1 platform is broken.
   *Effort:* S. *Risk:* None — current `doctor` recipes already do most of this.

### P3 (UX polish, low effort)

7. **Sample metadata file (`nros.yaml` or `package.xml`-extension) per example.**
   *Problem:* Each example has its own `Cargo.toml` + `prj.conf` + `config.toml` + `package.xml`. There is no single source of truth for "this example targets `qemu_cortex_m3`, RMW=zenoh, runs the integration scenario `pubsub`".
   *Reference:* Zephyr's `sample.yaml` (see `zephyr-workspace/zephyr/samples/hello_world/sample.yaml`) carries `description`, `name`, `tags`, `integration_platforms`, `harness`, `harness_config.regex`. Twister consumes it directly.
   *Proposal:* Adopt `sample.yaml` verbatim for nros examples. `just test-all` reads it to know what regex to match in stdout.
   *Effort:* S–M. *Risk:* Low — additive.

8. **`just monitor <board>` that decodes panics.**
   *Problem:* When a Cortex-M3 panics, the user sees a hex address. They have to manually run `arm-none-eabi-addr2line` against the ELF.
   *Reference:* `idf.py monitor` decodes panic frames against the ELF automatically.
   *Proposal:* Reuse `defmt-print` (defmt) or `probe-rs` for ARM; for QEMU, a small script that pipes serial through `addr2line`.
   *Effort:* M. *Risk:* Low — additive.

9. **Eliminate `nano-ros-workspace` sibling-directory surprise.**
   *Problem:* `book/src/getting-started/zephyr.md:28-37` documents that `nano-ros-workspace` lives *outside* the repo and is symlinked back in. New users hit this immediately.
   *Reference:* IDF lives entirely in `$IDF_PATH`; west supports `--manifest-only` workspaces *inside* the project tree.
   *Proposal:* Default to `west init -l . zephyr-workspace/` so the workspace is a (gitignored) subdirectory of nros, not a sibling. Keep the env override.
   *Effort:* S. *Risk:* Low — `zephyr-workspace/` is already gitignored.

10. **Publish nros-c as an ESP-IDF component (`idf_component.yml`).**
    *Problem:* No ESP-IDF user can adopt nano-ros without leaving IDF for `esp-hal` + Rust.
    *Reference:* `external/esp-idf/examples/get-started/blink/main/idf_component.yml` shows the format. The ESP Component Registry is at https://components.espressif.com.
    *Proposal:* Build `nros-c` as a precompiled static lib for `esp32`, `esp32c3`, `esp32s3`, ship it as `aeon-iot/nros` on the registry; consumers add it to `main/idf_component.yml`. Pairs with proposal 4.
    *Effort:* L. *Risk:* Medium — committing to IDF maintenance burden; requires a precompiled distribution story (Phase 75 partially solves this).

11. **Top-level `just --list` curation: hide internal recipes.**
    *Problem:* `just --list` shows 62 recipes; many are internal (`_cmake-cargo-stale-guard`, `install-local-posix`, `refresh-cmake-cargo`). New users see noise.
    *Reference:* `idf.py --help` shows ~25 user verbs and hides the rest behind `--all`.
    *Proposal:* Adopt `just`'s `[private]` attribute on internal recipes (already done for `_`-prefixed ones; finish the job). Make `just --list` the user-facing curated set; introduce `just --list-all` for maintainers.
    *Effort:* S. *Risk:* None.

12. **Single per-example `README.md` template generated by `cargo nano-ros new`.**
    *Problem:* Some examples have READMEs (`examples/zephyr/rust/zenoh/talker/README.md`), most don't. New users opening `examples/qemu-arm-nuttx/c/zenoh/talker/` see no guidance.
    *Reference:* `external/example-application/README.md`, every IDF example has one.
    *Proposal:* `cargo nano-ros new` always writes a `README.md` with the build/run/expected-output snippet. Backfill existing examples via a script.
    *Effort:* S. *Risk:* None.

---

## Closing observation

nano-ros has done the hard plumbing — multi-RMW, multi-platform, multi-RTOS, with a real Zephyr module that *works*. The deltas to the reference SDKs are almost entirely surface-level: a curated CLI, a discoverable config flow, a cargo-publish, and a `run` verb. None of these change the architecture; they change who can land a "hello world" without reading 300 lines of `justfile`.

The single highest-leverage move is **proposal 4 (publish to crates.io)**: it deletes an entire class of foot-guns (`[patch.crates-io]`, the `rustapp` rename, the sibling-workspace dance) in one stroke, and unblocks proposal 1 (`just new` / `cargo nano-ros new`) actually being copy-out-able.
