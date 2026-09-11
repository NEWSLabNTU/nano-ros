# What `nros build` Produces

`nros build` ends at an **artifact** and stops. It discovers your packages,
resolves the image, checks the toolchain, generates the root build file and the
entry, and then hands off to the native tool — `cargo`, `cmake`, `west` or
`idf.py`. The last stage is an `exec`, so the artifact is exactly the one that
tool would have produced had you run it yourself.

This page answers the question the build does not: **what are you holding, and
where is it?**

## There is no `nros run` and no `nros flash`

That is a decision, not a gap. How an image is flashed or started is a property
of the board and of your bench — a probe, a bootloader, a QEMU command line, a
CI fixture — and nano-ros cannot know it without inventing a device inventory
nobody asked for. So nano-ros delivers binaries, and the board's own tooling
takes them from there.

The commands quoted below are **that tooling's**, reproduced here as a
reference. They are not nano-ros wrappers and nano-ros does not run them for
you.

There is also no install step: `nros build` never runs `cmake --install`, writes
no `install/` prefix and stages nothing. Whatever the native tool wrote in its
own build tree is the deliverable.

## Where it lands

Which tool builds an image is decided by its platform and by whether the
workspace crosses languages:

| platform / shape | driver | the artifact |
| --- | --- | --- |
| Rust single package, any non-Zephyr non-ESP32 board | `cargo` | `<leaf>/build/<image-id>/target/[<triple>/]<profile>/<bin>` |
| Rust-only workspace, the same boards | `cargo` | `<ws>/build/<coordinate>/<image>_entry/target/[<triple>/]<profile>/<image>_entry` |
| any workspace containing C or C++ | `cmake` | `<ws>/build/<coordinate>/cmake/<image>_entry` |
| a `zephyr` board | `west` | `<ws>/build/zephyr/zephyr.{elf,bin,exe}` |
| an `esp32` board | `idf.py` | ESP-IDF's own `build/` in the project directory |

The **coordinate** is the platform and the RMW — `posix-zenoh`,
`freertos-cyclonedds` — plus, for CMake, the board, because CMake pins one
compiler per configure and two boards on one platform would otherwise share a
cache: `posix-zenoh-native`, `freertos-zenoh-mps2-an385-freertos`.

Everything above is under `build/`, and that is the rule rather than a
coincidence: a workspace has **no root `Cargo.toml` and no root
`CMakeLists.txt`** (RFC-0098 D9). It is a directory of packages, like a
colcon workspace, and every generated thing — the entry, the root build
file, the cargo settings, the compiled output — lives in `build/`.

One path the build *does* print is easy to mistake for the binary:

```text
nros build:   entry → …/build/posix-zenoh/native_entry
```

That is the generated **entry package** — source that nano-ros wrote for you,
under `build/` because it is build output. The compiled program is elsewhere;
see below.

## cargo

Every image gets its own cargo settings file, `build/<image-id>/nros-cargo.toml`
for a single package and `build/<coordinate>/<entry>/nros-cargo.toml` for a
workspace, and `nros build` points cargo at it:

```text
cargo build --manifest-path talker/Cargo.toml --config talker/build/native/nros-cargo.toml
```

That file states the image's `target-dir`, so each image compiles into its
own tree rather than sharing one `target/`. It also carries the board's
triple, linker flags and cross compiler, the resolved pool budgets as
`[env]`, the in-repo `[patch.crates-io]` rows, and the `nros-*` build
profiles. It is generated on every sync and build — read it, never edit it.

For a single package the artifact is that package's own binary:

```text
<leaf>/build/<image-id>/target/[<triple>/]<profile-dir>/<bin>
```

`examples/native/rust/talker` lands at `build/native/target/debug/talker`;
its bare-metal sibling `examples/mps2-an385-baremetal/rust/talker` at
`build/mps2-an385-baremetal/target/thumbv7m-none-eabi/debug/qemu-bsp-talker`.
The triple level appears whenever the board pins one — which it does in the
generated `[build] target`, not on your command line.

For a workspace the binary is the generated entry, inside that entry's own
target dir — the Rust scaffold's `native` image is
`build/posix/native_entry/target/debug/native_entry`.

`<profile-dir>` is `debug` unless the image declares
`[image.<id>] profile = …`, in which case it is that profile's directory
(`nros profile dir <name>` prints it).

**Next step:** on a host board, run it. It is an ordinary executable.

```bash
./build/native/target/debug/talker
```

For a QEMU board, the generated settings file also carries that board's
`runner`, so `cargo run` through the same `--config` boots the image —
see [Build Profiles](build-profiles.md) for driving cargo yourself.

A multi-process zenoh example also needs a router; that is ROS's `rmw_zenohd`,
not something nano-ros ships — see
[Deployment Workflow](deployment.md) and
[Install](../getting-started/installation.md#do-i-need-ros-2-installed).

## C, C++ and mixed workspaces (cmake)

CMake is chosen whenever the package graph contains anything that is not Rust,
on the host and when cross-compiling alike. The generated root is `<ws>/build/<coordinate>/CMakeLists.txt`,
it is configured into `<ws>/build/<coordinate>/cmake/`, and each image's
executable lands at the top of that directory:

```text
<ws>/build/<coordinate>/cmake/<image>_entry
```

For example, `examples/workspaces/c` builds `[image.native]` to
`build/posix-zenoh-native/cmake/native_entry` and its FreeRTOS image to
`build/freertos-zenoh-mps2-an385-freertos/cmake/freertos_entry`. (Node packages
also build their own executables, under `cmake/pkg/<pkg>/<pkg>`; the *image* is
the one at the top level.)

The file has no extension in either case. On the host it is a host executable;
for a cross board it is a statically linked ELF for that architecture — the
FreeRTOS one above is `ELF 32-bit LSB executable, ARM, EABI5, statically
linked`.

**Next step:**

* **host** — run it directly.
* **QEMU** — the ELF is what `-kernel` takes, with the machine and CPU your
  board's guide names.
* **hardware** — hand the ELF to your flasher (`probe-rs`, OpenOCD, a
  vendor tool). nano-ros has no opinion about which.

## Zephyr (west)

`nros build` resolves your application and its overlays and runs `west build`
from the workspace root, passing no `-d`, so west's default build directory
applies. Zephyr writes its image under `zephyr/` inside it:

```text
<ws>/build/zephyr/zephyr.elf      # always
<ws>/build/zephyr/zephyr.exe      # native_sim — a host executable
<ws>/build/zephyr/zephyr.bin      # cross targets — the raw image
<ws>/build/zephyr/zephyr.map      # the link map
```

Which of `.exe` and `.bin` appears is Zephyr's choice, not nano-ros's: a
`native_sim` build produces `zephyr.exe`, a Cortex-M build produces
`zephyr.bin`.

**Next step — Zephyr's own verbs:**

```bash
./build/zephyr/zephyr.exe     # native_sim: just run it
west build -t run             # emulated boards: Zephyr's runner
west flash                    # hardware: the runner declared by the board
```

`west flash` picks its runner from the board's own `board.cmake`; nano-ros adds
nothing to it. Note that these are run against the west build directory, so
they work from wherever you would normally run `west`.

## ESP32

Two different shapes, because there are two ways to build for an ESP32:

**ESP-IDF component** (`idf.py` driver) — `nros build` runs `idf.py build` in
the project directory, and the artifact is whatever ESP-IDF writes under that
project's `build/`. The next step is Espressif's:

```bash
idf.py -p /dev/ttyUSB0 flash monitor
```

See [ESP32 (ESP-IDF component)](../getting-started/integration-esp-idf.md).

**Rust bare-metal (esp-hal)** — a single-package image on the cargo driver
like any other, so the ELF is under that image's own target dir:
`build/<image-id>/target/<triple>/<profile-dir>/<name>`. `espflash` turns it
into a flash image or writes it to a board:

```bash
espflash flash --monitor \
    build/esp32-c3-baremetal/target/riscv32imc-unknown-none-elf/release/talker
espflash save-image --chip esp32c3 --flash-size 4mb --merge \
    build/esp32-c3-baremetal/target/riscv32imc-unknown-none-elf/release/talker talker.bin
```

See [ESP32 (esp-hal)](../getting-started/esp32.md).

## Finding it yourself

When in doubt, ask the build what it is about to do. `--dry-run` prints the
stages and the exact command, and performs no I/O:

```bash
nros build <image> --dry-run
```

The tool named in that command owns the output location, and its own
documentation is the authority — which is the whole point of handing off rather
than wrapping.

For the generation side of this — which root file is written, why the entry is
derived rather than hand-written, and what the coordinate directory holds — see
[Images](../getting-started/workspace-entry-pkg.md#what-nros-build-generates).
