# Cross-RTOS SDK UX: PlatformIO, Arduino, Mbed CLI 2 — Lessons for nano-ros

Status: Research note (2026-05-04)
Author: Claude (research dispatch)
Audience: nano-ros maintainers planning Phase 23 (Arduino lib), Phase 88
(`nros-log`), and any future "developer onboarding" phase.

This note studies how three established cross-RTOS embedded SDKs hide
toolchain, board, and library complexity from end users, and proposes
concrete UX improvements for nano-ros. The reference SDKs were chosen because
each solves part of the same problem nano-ros is solving today: one workflow
across many RTOSes/boards/languages.

Repos studied (cloned to `/home/aeon/repos/nano-ros/external/`, `--depth=1`):

- `platformio/platformio-core`, `platformio/platform-espressif32`,
  `platformio/platformio-examples`
- `arduino/arduino-cli`, `arduino/ArduinoCore-API`
- `ARMmbed/mbed-tools`

nano-ros baselines reviewed:

- `/home/aeon/repos/nano-ros/CLAUDE.md`
- `/home/aeon/repos/nano-ros/justfile` (1123 lines, 60+ recipes)
- `/home/aeon/repos/nano-ros/examples/qemu-arm-freertos/rust/zenoh/talker/`
  (a full FreeRTOS Rust example: 4 config files for one binary)
- `/home/aeon/repos/nano-ros/examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt`
- `/home/aeon/repos/nano-ros/templates/` (4 Cargo.toml stubs, no scaffolding tool)
- `/home/aeon/repos/nano-ros/book/src/getting-started/{freertos,zephyr,nuttx,esp32}.md`
  (576 + 291 + 133 + 305 = **1305 lines** of platform-specific onboarding text)
- `/home/aeon/repos/nano-ros/docs/roadmap/phase-23-arduino-precompiled.md`

## 1. Executive Summary — Five UX Patterns These SDKs Got Right

1. **One declarative file replaces a tree of toolchain artifacts.**
   `platformio.ini`, `library.properties`, `mbed_app.json` collapse what would
   otherwise be a Makefile + linker script + toolchain config + library list +
   board metadata into a single human-readable file the user actually edits.
   nano-ros today asks the user to maintain four (`Cargo.toml`,
   `.cargo/config.toml`, `config.toml`, `CMakeLists.txt`) plus
   `cmake/<plat>-support.cmake`.

2. **Board selection is a string, not a crate dependency.**
   `board = nodemcuv2`, FQBN `arduino:samd:mkr1000`, `mbed-tools compile -m K64F`
   all dispatch to a registry of board JSON descriptors. nano-ros forces a
   `nros-board-<plat>` crate dep + custom feature flags + per-board cmake
   modules; porting to a new STM32 part means writing a new crate.

3. **Project bootstrap is one CLI call, not a copy-paste recipe.**
   `pio project init -b nodemcuv2`, `arduino-cli sketch new MySketch`,
   `mbed-tools new my-app` create a working project. nano-ros's templates/
   directory documents a 4-step manual copy procedure
   (`templates/README.md` lines 22-31).

4. **Libraries are pip/cargo-style first-class entities.** PlatformIO
   `lib_deps = bblanchon/ArduinoJson@^7.0.0`, Arduino
   `arduino-cli lib install "ArduinoJson"`, Mbed `mbed-tools deploy` over a
   manifest. There is a registry, version constraints, and transitive
   resolution. nano-ros has no story for "I want to use nros-vision-msgs from
   user-foo's repo" beyond `find_package(NanoRos)` + manual cmake plumbing.

5. **The user-facing entry point hides language and toolchain.** PlatformIO
   wraps SCons + arm-none-eabi + esp-idf behind `pio run`. Arduino wraps
   gcc-avr + avrdude behind `arduino-cli compile`. nano-ros currently exposes
   `just`, `cargo`, `cmake`, `west`, and `qemu` directly — five user-visible
   tools with different idioms.

## 2. The `platformio.ini` Model

A canonical platformio.ini, reproduced from
`/home/aeon/repos/nano-ros/external/platformio-examples/wiring-blink/platformio.ini`:

```ini
[env:uno]
platform = platformio/atmelavr
framework = arduino
board = uno

[env:featheresp32]
platform = platformio/espressif32
framework = arduino
board = featheresp32

[env:teensy31]
platform = platformio/teensy
framework = arduino
board = teensy31
```

Three build environments, three boards, three architectures, three
toolchains, **one** file. `pio run` builds all three. `pio run -e uno`
builds one. `pio run -e uno -t upload` flashes. The file is what the user
sees, edits, commits, and onboards a colleague with.

Compare to nano-ros's per-binary configuration surface for
`examples/qemu-arm-freertos/rust/zenoh/talker/`:

| File | Lines | Owns |
|---|---|---|
| `Cargo.toml` | 22 | crate name, nros features (`rmw-zenoh,platform-freertos,link-tcp,link-udp-unicast,ros-humble`), dep paths |
| `.cargo/config.toml` | 21 | target triple, runner, linker flags, `[patch.crates-io]` for codegen messages |
| `config.toml` | 25 | network IP/MAC/gateway, zenoh locator, scheduling priorities |
| `package.xml` | — | ROS interfaces declaration for `cargo nano-ros generate-rust` |
| `cmake/freertos-support.cmake` (shared) | shared | toolchain, FreeRTOS sources, Corrosion bridge |

For the C variant of the same talker, add `CMakeLists.txt` (30 lines) which
re-reads `config.toml` via `nano_ros_read_config()` and re-injects the same
values as `target_compile_definitions`. Five locations re-state board,
platform, and config — they cannot drift safely.

### Could nano-ros adopt a `nano-ros.toml` analogue?

Yes, and it would supersede 80% of the per-example sprawl. Sketch:

```toml
# nano-ros.toml
[project]
name = "talker"
language = "rust"             # or "c", "cpp"
ros_edition = "humble"

[[env]]
name = "freertos-mps2"
board = "mps2-an385-freertos"  # resolves to nros-board-mps2-an385-freertos
rmw   = "zenoh"
transport = ["tcp", "udp-unicast"]

[env.network]
ip = "10.0.2.20"
mac = "02:00:00:00:00:00"
gateway = "10.0.2.2"
netmask = "255.255.255.0"

[env.zenoh]
locator = "tcp/10.0.2.2:7451"
domain_id = 0

[[env]]
name = "zephyr-native-sim"
board = "native-sim"
rmw   = "zenoh"
transport = ["tcp"]
zenoh = { locator = "tcp/127.0.0.1:7456" }

[interfaces]
generate = ["std_msgs", "geometry_msgs"]

[dependencies]
# C/C++ user libraries (see section 4)
"acme/nros-vision-msgs" = "^0.3"
```

A `cargo nano-ros build` driver reads this, **emits** the right
`Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` + transient
`config.toml` into a `target/<env-name>/` working directory, runs cargo or
cmake, and returns. The user never edits the generated files.

The same envs match PlatformIO semantics: `nano-ros build -e freertos-mps2`,
`nano-ros run -e freertos-mps2`, `nano-ros monitor -e freertos-mps2`. And
multi-env CI becomes a single command per project, not 60+ recipes in
`justfile` (recipes counted: `freertos`, `nuttx`, `threadx_linux`,
`threadx_riscv64`, `esp32`, `zephyr`, `qemu`, ...).

Effort: **L** (multi-month), but high leverage. Risk: medium — the layered
RTOS tree (`third-party/`, sibling `nano-ros-workspace/` for Zephyr) means
not every project tree is self-contained. Solvable by keeping
`just <plat> setup` orthogonal: nano-ros.toml never owns SDK paths.

## 3. The Arduino Library Distribution Model

Arduino libraries are a directory with one metadata file
(`library.properties`) and a `src/` tree:

```
NanoROS/
├── library.properties
├── keywords.txt
├── src/
│   ├── NanoROS.h               # umbrella header
│   ├── nros_pub_sub.h
│   └── cortex-m4/
│       └── libnanoros.a        # precompiled per arch (precompiled=true)
├── examples/
│   └── Talker/Talker.ino
```

`library.properties` keys (from
`/home/aeon/repos/nano-ros/external/arduino-cli/docs/library-specification.md`
sibling spec): `name, version, author, maintainer, sentence, paragraph,
category, url, architectures, depends, includes, precompiled, ldflags`.

`micro-ROS Arduino` (already cloned at
`/home/aeon/repos/nano-ros/external/micro_ros_arduino`) is the proof of
shape: ship `libmicroros.a` per arch under `src/<arch>/`, ship transport
helpers as ~70-line Arduino-specific `.cpp` files, ship the rest as headers.
nano-ros Phase 23 adopts the same model.

### Concrete .ino UX target

This is what the user-facing Arduino sketch should look like — write it now
so Phase 23 has a star to steer by.

```cpp
// examples/Talker/Talker.ino
#include <NanoROS.h>
#include <std_msgs/Int32.h>

nros::Executor executor;
nros::Publisher<std_msgs::Int32> pub;

void setup() {
  Serial.begin(115200);
  while (!Serial) {}

  // Transport: pick one. Arduino-specific glue lives here, all 70 lines of it.
  nros::set_serial_transport(&Serial);
  // or: nros::set_wifi_udp_transport("ssid", "pass", "192.168.1.10", 7447);

  nros::Config cfg;
  cfg.locator   = "serial//dev/ttyUSB0";
  cfg.domain_id = 0;
  cfg.node_name = "arduino_talker";

  if (!executor.open(cfg)) {
    Serial.println("nros: open failed");
    while (true) {}
  }

  auto node = executor.create_node("talker");
  pub = node.create_publisher<std_msgs::Int32>("/chatter");
}

void loop() {
  static int32_t count = 0;
  std_msgs::Int32 msg;
  msg.data = count++;
  pub.publish(msg);

  executor.spin_once(10);  // ms
  delay(100);
}
```

Key design rules (validated against nano-ros's
`docs/roadmap/phase-23-arduino-precompiled.md`):

- **No `main()`** — Arduino owns it. nros-c entry point is `nros::set_*_transport`.
- **No `<vector>`, no `String`** — heapless on AVR/SAMD. nros-c is already
  alloc-free.
- **One umbrella include** — `<NanoROS.h>` re-exports executor + node + pub +
  sub + service + action.
- **Serial-first** — Phase 23 must default to serial transport because
  zenoh-pico over UDP needs WiFi/Ethernet which not all boards have. The
  transport setup helpers are the only board-specific surface.
- **Generate, don't hand-write, message types** — pre-generate `std_msgs/`,
  `geometry_msgs/`, `sensor_msgs/` headers and ship them in `src/msg/`.
  Custom messages need a host-side `cargo nano-ros generate-c --arduino-lib`
  step that emits a sibling library.

Effort: **M** for Phase 23 v1 (one arch, serial-only). **L** for full
multi-arch + WiFi UDP variant.

## 4. Library / Component Manager — The Big Gap

PlatformIO `lib_deps` accepts:

```ini
lib_deps =
    bblanchon/ArduinoJson@^7.0.0           ; registry + semver
    https://github.com/me/foo.git#main     ; VCS
    file:///abs/path/to/local-lib          ; local
    name=https://example.com/zip-archive   ; archive
```

Arduino: `arduino-cli lib install "ArduinoJson"` hits the Arduino Library
Registry (~7000 libs). ESP-IDF Component Registry, Zephyr modules, and
`west` manifests all converge on the same shape: declarative manifest, named
versions, transitive resolution.

**nano-ros has nothing here for C/C++ users.** Today, the only way to consume
a third-party `nros-vision-msgs` package as a C user is:

1. Fork/clone it next to `packages/`.
2. Edit `packages/codegen/interfaces/` to bundle its `.msg` files.
3. Add a path entry to a workspace `Cargo.toml`.
4. Write a `find_package` shim or add it to the example's `CMakeLists.txt`.

Steps 1–4 are not user-facing. They require fork-level access to the
nano-ros tree.

For Rust users, cargo handles deps adequately, but **C/C++ examples are
described as "standalone projects, copy-out templates"** (CLAUDE.md, lines
under "Examples = Standalone Projects"). Once the user copies one out, they
have no way to add a dependency without re-deriving the cmake plumbing.

### Proposal: `nano-ros add <pkg>`

A registry-aware helper that:

1. Resolves a name → URL + version (against a curated index, initially a
   single GitHub repo `nano-ros/registry` with TOML descriptors).
2. Updates `[dependencies]` in `nano-ros.toml`.
3. For Rust, runs `cargo add` with the right path/git/version.
4. For C/C++, fetches via FetchContent, adds `find_package(<lib> CONFIG REQUIRED)`
   and the right `target_link_libraries` line into the generated cmake.

Effort: **L** (registry infra). Risk: low — a curated index of ~10
official packages (`nros-rcl-interfaces`, `nros-lifecycle-msgs`, etc.) is
viable for v1.

## 5. Board Abstraction

PlatformIO board JSON
(`/home/aeon/repos/nano-ros/external/platform-espressif32/boards/esp32dev.json`,
abridged):

```json
{
  "build": {
    "core": "esp32",
    "extra_flags": "-DARDUINO_ESP32_DEV",
    "f_cpu": "240000000L",
    "mcu": "esp32",
    "variant": "esp32"
  },
  "frameworks": ["arduino", "espidf"],
  "name": "Espressif ESP32 Dev Module",
  "upload": { "flash_size": "4MB", "maximum_ram_size": 327680 }
}
```

That's the entirety of board support: a JSON file. To port to a new board,
a user (or vendor) writes one JSON file, drops it under `boards/`, and
`board = my_new_board` works.

nano-ros has 9 board crates under `packages/boards/`: `mps2-an385`,
`mps2-an385-freertos`, `stm32f4`, `esp32`, `esp32-qemu`, `nuttx-qemu-arm`,
`threadx-linux`, `threadx-qemu-riscv64`, `orin-spe`. Each is a full Rust
crate with `build.rs`, `config/` directory (linker scripts, FreeRTOSConfig.h,
lwipopts.h), `src/lib.rs` with `init_hardware()`, etc.

**To port to a new STM32 part the user must:**

1. Create `packages/boards/nros-board-stm32f7/` with `Cargo.toml`, `build.rs`,
   `src/lib.rs` (~300+ lines based on existing crates).
2. Write `memory.x` and a HAL initializer.
3. Update `nros-platform-*` if the platform layer needs new traits.
4. Hand-craft an example tree mirroring `examples/stm32f4/`.

Cf. PlatformIO: drop a `stm32f7nucleo.json`, add `board = stm32f7nucleo`
to platformio.ini, done. Frameworks (Arduino/ESP-IDF/Mbed) handle
chip init.

This is a real divergence, not a minor one. The Rust side will always need
*some* per-chip code (PAC selection, memory.x, peripheral init), but the
**user-facing contract** can be flattened.

### Proposal: split `boards/<name>/board.toml` from runtime crate

```toml
# packages/boards/registry/stm32f429zi-nucleo.toml
display_name = "STM32 NUCLEO-F429ZI"
chip = "stm32f429zi"
flash_kb = 2048
ram_kb   = 256
runtime_crate = "nros-board-stm32f4"
runtime_features = ["stm32f429zi", "ethernet"]
memory_x = "memory/stm32f429zi.x"
default_priorities = { app = 12, zenoh_read = 16 }
```

A new STM32F7 part with the same HAL family becomes one TOML file +
`memory.x`, **not** a new crate. Effort: **M**. Risk: medium — bare-metal
init varies more than PlatformIO's framework world.

## 6. Project Bootstrap — Counting Steps

Empirical step counts from each SDK's "from zero to running":

**PlatformIO** (3 commands):
```bash
pio project init -b nodemcuv2
# (edit src/main.cpp)
pio run -t upload
```

**Arduino** (4 commands):
```bash
arduino-cli core install arduino:samd
arduino-cli sketch new MyFirstSketch
# (edit MyFirstSketch.ino)
arduino-cli compile --fqbn arduino:samd:mkr1000 MyFirstSketch
arduino-cli upload -p /dev/ttyACM0 --fqbn arduino:samd:mkr1000 MyFirstSketch
```

**Mbed CLI 2** (3 commands):
```bash
mbed-tools new my-app
# (edit main.cpp)
mbed-tools compile -m K64F -t GCC_ARM
```

**nano-ros, FreeRTOS QEMU example, today** (counted from
`book/src/getting-started/freertos.md` lines 27-115):

```bash
just setup                                # 1: install everything (~20 min)
just freertos setup                       # 2: download FreeRTOS + lwIP
direnv allow                              # 3: load .env
just build-zenoh-pico-arm                 # 4: prereq for build (CLAUDE.md)
just qemu setup-network                   # 5: TAP bridge for QEMU
# Pick example or copy from templates/
cp -r examples/qemu-arm-freertos/rust/zenoh/talker my-talker
# Edit Cargo.toml, .cargo/config.toml, config.toml dep paths
cd my-talker && cargo build --release    # 6
qemu-system-arm -M mps2-an385 ...        # 7 (or just freertos run)
```

**Seven steps, four files to edit.** Plus the prerequisite knowledge of
which features to set in Cargo.toml (`rmw-zenoh,platform-freertos,...`),
what `config.toml` keys exist, and how to handle the
`[patch.crates-io]` block.

`book/src/getting-started/freertos.md` is **576 lines**. PlatformIO's
Quickstart is ~300 lines and covers more boards. The ratio is the
proxy metric for onboarding ceremony.

## 7. UX Improvement Proposals (Prioritized)

Each proposal: **problem → reference SDK approach → proposed change →
effort (S/M/L) → risk**.

### P1. Ship a `cargo nano-ros init` scaffolder (PRIORITY 1)

- **Problem:** Templates exist (`/home/aeon/repos/nano-ros/templates/`) but
  require manual copy + edit; new users never find them.
- **Ref:** `pio project init -b nodemcuv2`, `arduino-cli sketch new`.
- **Change:** Extend the existing `cargo-nano-ros` codegen crate
  (`packages/codegen/cargo-nano-ros/`) with an `init` subcommand:
  `cargo nano-ros init --board mps2-an385-freertos --rmw zenoh --lang rust talker`.
  Emits a working tree from `templates/` filled with chosen
  features and minimal main. Runs `cargo build` to verify.
- **Effort:** S (a week, mostly templating).
- **Risk:** Low. The hardest decision is the board name registry — start
  with the 9 existing board crates as hardcoded enum.

### P2. Introduce `nano-ros.toml` as the single user-edited config

- **Problem:** Four files per Rust example, five per C example; high drift
  surface, hostile to newcomers (CLAUDE.md "Examples = Standalone Projects"
  section captures the cost).
- **Ref:** `platformio.ini` env matrix.
- **Change:** Section 2 above. `cargo nano-ros build` reads
  `nano-ros.toml`, emits transient cargo + cmake configs into
  `target/<env>/`. Existing files remain valid for power users.
- **Effort:** L (multi-month). Phased: **(a)** read-only — emits
  generated cargo/cmake, user keeps their hand-written ones initially;
  **(b)** opt-in switch in nano-ros.toml turns hand-written into
  generated; **(c)** docs and templates flip to nano-ros.toml as canonical.
- **Risk:** Medium. Shared `cmake/<plat>-support.cmake` modules already
  centralize the duplicated logic — generation just promotes that pattern
  one level up.

### P3. Phase 23 Arduino library with the umbrella `<NanoROS.h>` API

- **Problem:** Phase 23 listed as Not Started despite being a "drive
  adoption" lever; user-facing UX shape is undefined.
- **Ref:** Arduino `library.properties` + `precompiled=true` + sketch
  with `setup()/loop()`.
- **Change:** Implement Phase 23 with the `.ino` sketch in section 3 as
  the acceptance criterion. Ship `NanoROS-1.0.0.zip` to the Arduino
  Library Registry.
- **Effort:** M for v1 (one arch + serial). L for full WiFi UDP + multi-arch.
- **Risk:** Low — micro-ROS Arduino is a working precedent vendored at
  `/home/aeon/repos/nano-ros/external/micro_ros_arduino/`.

### P4. `nano-ros add <pkg>` + minimal package registry

- **Problem:** No 3rd-party C/C++ package story (section 4).
- **Ref:** `pio pkg install`, `arduino-cli lib install`,
  ESP-IDF Component Registry.
- **Change:** A `nano-ros/registry` GitHub repo with TOML index files
  pointing at git-tagged releases. `cargo nano-ros add nros-vision-msgs`
  resolves, vendors via cargo dep / cmake FetchContent, updates
  `nano-ros.toml`. Bootstrap with the existing in-tree packages
  (`nros-rcl-interfaces`, `nros-lifecycle-msgs`, ROS msg families).
- **Effort:** L (registry + tooling). M if scoped to "trusted index" v1.
- **Risk:** Low (additive — no impact on current users).

### P5. Board descriptor TOML to decouple board names from crates

- **Problem:** Adding an STM32F7 NUCLEO board today = new Rust crate +
  cmake + per-platform doc page. The user-facing knob is hidden behind
  workspace surgery.
- **Ref:** PlatformIO `boards/*.json`, Arduino FQBN `vendor:arch:board`.
- **Change:** `packages/boards/registry/<name>.toml` (section 5 sketch).
  `nano-ros.toml` `board = "stm32f429zi-nucleo"` resolves to a runtime
  crate + features + memory layout. Existing crates stay; the registry
  is a thin index above them.
- **Effort:** M.
- **Risk:** Medium — bare-metal initialization has chip-specific quirks
  that don't fit a uniform schema (clock trees, peripheral mapping). Mitigate
  by limiting v1 to "same-family" derivations (STM32F4 family, ESP32-S3 vs
  ESP32, etc.).

### P6. Replace `just <plat> ...` with `nano-ros run -e <env>`

- **Problem:** 1123-line justfile is the canonical entry point; impenetrable
  for a new user; ties UX to a niche tool. CLAUDE.md instructs "Always `just
  ci` after task" — institutional knowledge load.
- **Ref:** `pio run`, `mbed-tools compile`, `arduino-cli compile`.
- **Change:** A thin Rust CLI `nano-ros` (alias for
  `cargo nano-ros`) that wraps `cargo`, `cmake`, `qemu`, `west`. The
  `justfile` survives as the **CI orchestration layer** (parallelism,
  matrix, network setup) but stops being the user entry point.
  Documentation in `book/src/getting-started/` flips to `nano-ros run`.
- **Effort:** M.
- **Risk:** Medium — a fork of the user-facing API; needs careful migration
  notes. The existing `just` recipes can call into `nano-ros` for
  consistency.

### P7. Unified `nano-ros monitor` (serial / log / udp)

- **Problem:** Logs come out of QEMU semihosting, RTT, UART, or stdout
  depending on platform. The user has to know each. Phase 88 (`nros-log`)
  is "Not Started".
- **Ref:** `pio device monitor`, `arduino-cli monitor`.
- **Change:** `nano-ros monitor -e <env>` reads from the env's transport
  (semihosting log, `tap0`, serial port) and pretty-prints. Pairs with
  Phase 88.
- **Effort:** M.
- **Risk:** Low.

### P8. `nano-ros doctor` consolidating `just doctor`

- **Problem:** `just doctor` exists and is read-only, but only checks
  workspace. There is no single "is my Phase 23 / Phase 92 / ESP32
  install ready?" view.
- **Ref:** ESP-IDF `idf_tools.py check`, mbed `mbed-tools detect`.
- **Change:** Promote `just doctor` to `nano-ros doctor`. Per-RTOS plug-ins
  (`nano-ros doctor freertos`, `nano-ros doctor zephyr`) probe SDK paths,
  toolchains, QEMU versions, and emit a single fixit-style report. Hook
  into board descriptors (P5) so `nano-ros doctor -e freertos-mps2`
  validates one env end to end.
- **Effort:** S (incremental on existing doctor recipe).
- **Risk:** None.

### P9. Concrete board JSON for QEMU (`mps2-an385-detect`)

- **Problem:** `mbed-tools detect` and `arduino-cli board list` instantly
  enumerate available targets. nano-ros users have to read CLAUDE.md or
  grep `packages/boards/`.
- **Ref:** `mbed-tools detect`, `arduino-cli board listall`.
- **Change:** `nano-ros board list` lists every board in
  `packages/boards/registry/` with chip, RAM, flash, supported RMWs.
  Trivial once P5 lands.
- **Effort:** S.
- **Risk:** None.

### P10. Doc consolidation: collapse 1305 lines of getting-started

- **Problem:**
  `book/src/getting-started/{freertos,zephyr,nuttx,esp32}.md` total 1305
  lines, much duplicating the same setup pattern.
- **Ref:** PlatformIO has one Quickstart + per-board pages; Arduino has one
  Getting Started + FQBN reference.
- **Change:** Once P1–P3 land, replace per-RTOS pages with a single
  "Getting Started" page (5 commands) plus per-RTOS appendices for
  hardware-specific notes (TAP networking, SDK download paths).
- **Effort:** S.
- **Risk:** Low — pure docs work, gated on tooling.

## 8. Recommendation

The single highest-leverage move is **P1 (`cargo nano-ros init`)** because
it ships *now* on top of existing infrastructure and immediately collapses
the four-files-to-edit problem for new projects. It also de-risks P2 and
P5 (the scaffolder learns the schema first; promotion to general
build-driver follows).

P3 (Arduino lib) is the highest-leverage *external* move — it opens a user
base that will not adopt the existing Rust toolchain regardless of CLI UX
improvements.

P2 (`nano-ros.toml`) is the highest-leverage *long-term* move and the one
that aligns nano-ros with PlatformIO's UX outright. It's also the one that
most heavily reshapes `examples/` and `templates/`. Schedule it after
Phase 100 stabilizes the AGX Orin board pattern, so the descriptor schema
covers IVC-only platforms from the start.

Rust at the core is preserved in every proposal: the user-facing CLI is a
thin wrapper, not a replacement. PlatformIO's strength is precisely this
layering — Python on top, gcc/clang/SCons underneath. nano-ros's
equivalent layering is `cargo nano-ros` on top, `cargo` + `cmake` + `qemu`
underneath. The work is to make that top layer real.
