# Workflow by Platform and Language

You have a target and a language. This page is the sequence of commands
that pair implies, and the one step whose absence is the most common
first failure.

The rest of the book is organized by *where you are going* — a starter
page per platform. This page is organized by *what you type*, because
the commands vary along a different axis than the pages do: the builder
follows from your **language**, and the toolchain follows from your
**platform**. Neither table below is a summary of the other.

## Where the target comes from

**Every leaf, in every language, states what it deploys to in one file:
`system.toml`, beside its `Cargo.toml` or `CMakeLists.txt`** (RFC-0098).
`[system]` carries the RMW and the domain, `[[component]]` carries what
runs, and one `[image.<id>]` per target carries its `board` and, on an
embedded image, its network identity:

```toml
[system]
rmw       = "zenoh"
domain_id = 0

[image.mps2]
board   = "qemu-mps2-an385"
locator = "tcp/10.0.2.2:10500"
net     = { eth0 = { network = "qemu", host = 10 } }
```

Everything that choice implies — the target triple, the linker flags,
the QEMU runner, the cross compiler for the `cc` crate, the pool sizes
derived from the components, the `[patch.crates-io]` table that resolves
nano-ros's registry-style names into the SDK — is **generated**,
never written by hand. Retargeting is editing the `board` line: in a
workspace that is the whole edit, and in a single-package Rust leaf too
once it depends on its generated `<pkg>_nros_selection` package instead of
a board crate, as every in-tree single-package example on a supported board
does
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)).

## The step that is easy to miss

**Rust leaves need `nros sync` before their first build. C and C++
leaves do not.**

That split is a property of the two languages, not an inconsistency.
CMake generates a C/C++ leaf's message bindings *during configure*, so
there is nothing to prepare first. Cargo has no such stage: the message
crates a Rust leaf path-depends on, and the settings file that carries
its board facts, both have to exist before cargo parses the manifest.
`nros sync` produces them:

```text
<leaf>/generated/<pkg>/                  # message crates
<leaf>/build/<image>/nros-cargo.toml     # every cargo setting the board implies
```

**Skipping it fails in three ways, and they read very differently.**
`nros build` says so in one line and names the remedy:

```text
Error: <leaf> has not been synced — missing the resolved model under
`build/nros/models/`, the generated message crate `std_msgs`
(generated/std_msgs).
  Run `nros sync` in <leaf> (RFC-0098 D2), then build again.
```

A plain `cargo build` gets as far as the manifest and stops on the
missing message crate, without mentioning nano-ros at all:

```text
error: failed to load manifest for dependency `std_msgs`
Caused by:
  failed to read `<leaf>/generated/std_msgs/Cargo.toml`
Caused by:
  No such file or directory (os error 2)
```

And a `cargo build --config <leaf>/build/<image>/nros-cargo.toml` whose
settings file does not exist yet produces the one that does not look
like a missing file at all — cargo falls back to reading the argument as
an inline setting:

```text
error: failed to parse value from --config argument
`<leaf>/build/<image>/nros-cargo.toml` as a dotted key expression
Caused by:
  TOML parse error at line 1, column 39
  key with no value, expected `=`
```

If you see any of the three, run `nros sync`.

**Contributors (in-tree checkout):** the `just` recipes —
`just <module> build-fixtures` and friends — run `nros sync` for you, so
they work from a fresh clone. It is the hand-run build in a leaf that
needs you to run it yourself.

You need it **once per project location**, not once per build. Re-run
it after editing a `.msg`, `.srv`, or `.action` file, after changing
`system.toml`, after moving the project, and after installing another
release.

## Which builder your cell uses

Each cell is the builder that nano-ros's own CI uses for that pair, read
from [`examples/fixtures.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/fixtures.toml) — the manifest the fixture builds and the
staleness probe both consume. A dash means the pair has no in-tree
coverage today, not that it is forbidden.

The row names are the manifest's, which are shorter than the ones you
type: `freertos` here is the platform whose board is
`mps2-an385-freertos` and whose examples live in
[`examples/mps2-an385-freertos`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos). The per-platform table further down maps
all three spellings.

| platform | rust | c | cpp | mixed |
|---|---|---|---|---|
| `linux` | cargo | cmake | cmake | cmake |
| `freertos` | cargo | cmake | cmake | cmake |
| `nuttx` | cargo | cmake | cmake | — |
| `nuttx-riscv` | cargo | cmake | cmake | — |
| `threadx-linux` | cargo | cmake | cmake | cmake |
| `threadx-riscv64` | cargo, cmake | cmake | cmake | — |
| `zephyr` | cargo, west | cmake, west | cmake, west | cargo |
| `zephyr-cortex-m` | west | west | west | — |
| `esp32` | cargo | — | — | — |
| `baremetal` | cargo | — | — | — |

Where a row lists two builders, both are real and they build different
things: on Zephyr, `west` builds the single-node examples under
`examples/zephyr/<lang>/`, and `cargo`/`cmake` build the workspace
examples under [`examples/workspaces`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces).

## The three command shapes

Every cell above is one of these three, plus platform-specific flags
that live on the platform's own starter page.

### cargo — Rust, every platform

```bash
nros setup <board> --rmw zenoh  # toolchain + SDK for the target

cd <leaf>
nros sync                       # ← the step above; once per location
nros build                      # every [image.*]; or: nros build <image-id>
```

(`nros` is the installed CLI — [Installation](../getting-started/installation.md).)

The artifact lands under `build/`, keyed on the image:
`<leaf>/build/<image-id>/target/[<triple>/]<profile>/<bin>`. **No
`--target` on the command line, on any platform** — the board's triple
is in the generated settings file, which is also where its linker flags
and its QEMU runner live.

If you would rather drive cargo yourself — an IDE, a CI step,
`--release` — run `nros sync`; then a plain `cargo build` inside the
package works, because sync wires that file into the package's
gitignored `.cargo/config.toml`. To name the file explicitly instead,
run from the directory *above* the package — inside it cargo would read
the settings twice and join the board's rustflags onto themselves:

```bash
cd <leaf-parent>
cargo build --manifest-path <leaf>/Cargo.toml \
            --config <leaf>/build/<image-id>/nros-cargo.toml
```

`cargo run` through the same file works for a QEMU board — the runner is
in it.

### cmake — C and C++

```bash
nros setup <board> --rmw zenoh

cd <leaf>
nros build                      # configures + builds into build/<image>/cmake
```

The executable lands at `<leaf>/build/<image-id>/cmake/<name>`, where
`<name>` is what the leaf's `nano_ros_add_executable(<name> …)` calls it;
`nros build` prints that path and the command that runs it. No `nros sync`,
and no `-DNROS_BOARD` / `-DNROS_RMW` either: `find_package(nano_ros)` reads
the leaf's `system.toml` and derives the platform from the board it names,
and `nros build` passes the board's cross-toolchain file and the SDK root
([issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1296-nros-build-c-leaf-bringup-name-mismatch.md)).
It drives the same cmake pair in a C/C++ **workspace**.

**Escape hatch — driving CMake yourself.** `nros build --dry-run` prints the
exact configure line; its core is the SDK root, the image and, for a cross
board, the toolchain file the board's starter page names:

```bash
cmake -S . -B build/<image-id>/cmake -DNROS_IMAGE=<image-id> \
    -Dnano_ros_ROOT="$(nros sdk-root)" \
    -DCMAKE_TOOLCHAIN_FILE="$(nros sdk-root)/cmake/toolchain/<toolchain>.cmake"
cmake --build build/<image-id>/cmake --parallel
```

`-D_NANO_ROS_CODEGEN_TOOL=` is not needed when `nros` is on PATH; CMake
resolves it. A CMake tree with no `system.toml` at all is
[Build as a CMake subdirectory](../getting-started/build-as-subdirectory.md).

### west — Zephyr

```bash
nros setup zephyr --rmw zenoh

cd <leaf-or-workspace>
nros sync
nros build <image>              # runs `west build -d build/<coordinate>/<bringup>__<image>_west`
```

`nros build` resolves the west application and the image's overlays, runs
`west build` with that image's own build directory, and prints Zephyr's next
command for it — `west build -d <dir> -t run` for an emulated board,
`west flash -d <dir>` for one with a flash runner. `nros sync` comes first on
Zephyr whatever the language, because the build stops at its preflight on a
workspace that was never synced. Driving `west build` yourself is the escape
hatch; see [Zephyr (west module)](../getting-started/integration-zephyr.md).

### Where a leaf comes from

`<leaf>` above is a package of yours: `nros new <name> --platform
<platform> --lang <lang>` scaffolds one, and the examples on
[GitHub](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples) are
standalone copy-out projects with no workspace walk-up, so the files of
one copied into a directory of yours build there too.

The build output goes to `build/<image>/`, under the leaf, because the
generated settings file names that `target-dir`. A leaf carries no path
into the SDK at all: `nros` ties it to the SDK the binary belongs to, and
`--nano-ros-path` names another on the command line rather than in a
file.

## Per platform

| platform | grid row | `nros setup <board>` | examples under | recipes (contributors) | starter page |
|---|---|---|---|---|---|
| Linux host | `linux` | `linux` | [`examples/linux`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/linux) | `just linux …` | [Native host build](../platform-guides/native-host.md) |
| FreeRTOS (QEMU MPS2-AN385) | `freertos` | `mps2-an385-freertos` | [`examples/mps2-an385-freertos`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos) | `just freertos …` | [FreeRTOS](../getting-started/freertos.md) |
| NuttX (Arm) | `nuttx` | `qemu-armv7a-nuttx` | [`examples/qemu-armv7a-nuttx`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx) | `just nuttx …` | [NuttX](../getting-started/integration-nuttx.md) |
| NuttX (RISC-V) | `nuttx-riscv` | `rv-virt-nuttx` | [`examples/rv-virt-nuttx`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/rv-virt-nuttx) | `just nuttx …` | [NuttX](../getting-started/integration-nuttx.md) |
| ThreadX (Linux sim) | `threadx-linux` | `threadx-linux` | [`examples/threadx-linux`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux) | `just threadx_linux …` | [ThreadX](../getting-started/threadx.md) |
| ThreadX (QEMU RISC-V 64) | `threadx-riscv64` | `rv-virt-threadx` | [`examples/rv-virt-threadx`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/rv-virt-threadx) | `just threadx_riscv64 …` | [ThreadX](../getting-started/threadx.md) |
| Zephyr | `zephyr`, `zephyr-cortex-m` | `zephyr` | [`examples/zephyr`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr) | `just zephyr …` | [Zephyr](../getting-started/integration-zephyr.md) |
| ESP32 (dormant, issue 1525) | `esp32` | `esp32-c3-baremetal` | [`examples/esp32-c3-baremetal`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/esp32-c3-baremetal) | — (module not mounted) | [ESP32](../getting-started/esp32.md) |
| Bare-metal Cortex-M3 | `baremetal` | `mps2-an385-baremetal` | [`examples/mps2-an385-baremetal`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-baremetal) | `just qemu …` | [Bare-metal](../getting-started/bare-metal.md) |
| Arm FVP (Cortex-A SMP) | — | `zephyr` + the Arm FVP (`nros setup --tool arm-fvp`, x86_64 only) | — | — | [ARM FVP](../getting-started/arm-fvp.md) |

Multi-node workspace examples do not follow that directory rule: they
all live under [`examples/workspaces`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces), selected by fixture row rather
than by directory. So ESP32's single-node and workspace examples share
one grid row, `esp32`: a grid row names a platform FAMILY, never a board
(RFC-0093 R6), and `esp32` carries both. Zephyr is the case that does
split — `zephyr` and `zephyr-cortex-m` build the SAME examples for
different boards.

**Contributors (in-tree checkout):** each module's recipes are listed in
[Per-Platform Contributor Lanes](../internals/platform-lanes.md).

## More than one node

The sequence above builds one leaf. A project with several nodes adds a
Bringup package carrying the same `system.toml` — and nothing else: no
root build file, and no entry package to write, because the entry is
generated per `[image.*]`. The commands stay `nros sync` and
`nros build`, which then drive one of the three shapes above for you.
See [Project layout](../getting-started/workspace-from-app-node.md).

## If it does not work

- [Troubleshooting — First 10 Minutes](../getting-started/troubleshooting-first-10-min.md)
- [Troubleshooting](troubleshooting.md)
