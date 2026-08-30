# Workflow by Platform and Language

You have a target and a language. This page is the sequence of commands
that pair implies, and the one step whose absence is the most common
first failure.

The rest of the book is organized by *where you are going* — a starter
page per platform. This page is organized by *what you type*, because
the commands vary along a different axis than the pages do: the builder
follows from your **language**, and the toolchain follows from your
**platform**. Neither table below is a summary of the other.

## The step that is easy to miss

**Rust leaves need `nros sync` before their first build. C and C++
leaves do not.**

A Rust leaf resolves its nano-ros dependencies through a
`[patch.crates-io]` table in its own `.cargo/config.toml`, and that
table is generated — `.gitignore` excludes it, so a fresh clone does not
have it. Sixty-seven Rust example leaves have such a file on a synced
checkout; fifty of them are committed shells that `include` the
generated central table:

```toml
include = [ "../../../../../nros-patch.toml", "nros-board.toml"]
```

**Skipping the sync fails in two different ways, and the quieter one is
worse.** Where the config is committed, cargo treats the missing
`include` as a hard error while *parsing the manifest* — before it
builds anything, and before any message about nano-ros could appear:

```
error: failed to parse manifest at `<leaf>/Cargo.toml`

Caused by:
  could not load Cargo configuration

Caused by:
  failed to load config include `../../../../../nros-patch.toml` from `<leaf>/.cargo/config.toml`

Caused by:
  failed to read configuration file `<...>/nros-patch.toml`

Caused by:
  No such file or directory (os error 2)
```

Nothing in those five frames says `nros sync`. If you see it, this is
what it means.

Where the whole config is generated — the native leaves, for instance —
there is no file at all in a fresh clone, so there is no error to read.
The patch table simply is not there, and the nano-ros crates your leaf
names go looking for themselves on crates.io instead of in your
checkout. That one does not announce itself.

**Contributors (in-tree checkout):** the `just` recipes —
`just <module> build-fixtures` and friends — run
`nros sync` for you, so they work from a fresh clone. It is the
hand-run `cd <leaf> && cargo build` that needs you to run it yourself.

C and C++ leaves have no `.cargo/config.toml` at all — not committed,
not generated. Their message bindings are produced inside CMake by
`nros_find_interfaces()`, and the cargo builds CMake drives resolve
against the repo-root config, which carries no `include`. Running
`nros sync` for a C/C++ build is harmless but buys nothing.

You need it **once per checkout location**, not once per build. Re-run
it after editing a `.msg`, `.srv`, or `.action` file, and — because the
central table it writes holds absolute paths — after moving the
checkout, or after one of the patched crates moves *within* it.

## Which builder your cell uses

Each cell is the builder that nano-ros's own CI uses for that pair, read
from `examples/fixtures.toml` — the manifest the fixture builds and the
staleness probe both consume. A dash means the pair has no in-tree
coverage today, not that it is forbidden.

The row names are the manifest's, which are shorter than the ones you
type: `freertos` here is the platform whose board is
`qemu-arm-freertos` and whose examples live in
`examples/qemu-arm-freertos/`. The per-platform table further down maps
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
| `qemu-arm-baremetal` | cargo | — | — | — |
| `qemu-esp32-baremetal` | cargo | — | — | — |

Where a row lists two builders, both are real and they build different
things: on Zephyr, `west` builds the single-node examples under
`examples/zephyr/<lang>/`, and `cargo`/`cmake` build the workspace
examples under `examples/workspaces/`.

## The three command shapes

Every cell above is one of these three, plus platform-specific flags
that live on the platform's own starter page.

### cargo — Rust, every platform

```bash
./scripts/bootstrap.sh          # builds the in-tree nros CLI
source ./activate.sh            # OR: direnv allow / source ./activate.fish
nros setup <board> --rmw zenoh  # toolchain + SDK for the target

cd <leaf>
nros sync                       # ← the step above; once per checkout
cargo build --release
```

Some leaves also pin their cross target in that same
`.cargo/config.toml` (`[build] target = "thumbv7m-none-eabi"` on the
Cortex-M ones), so no `--target` on the command line. Others get it from
the platform's recipe instead — **contributors:** in an in-tree
checkout, `just --list <module>` shows which
recipe builds what, and using the recipe avoids having to know.

### cmake — C and C++

```bash
./scripts/bootstrap.sh
source ./activate.sh
nros setup <board> --rmw zenoh

cd <leaf>
cmake -B build -DCMAKE_TOOLCHAIN_FILE=<toolchain> -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
```

No `nros sync`. The toolchain file is per platform — see the starter
page. `-D_NANO_ROS_CODEGEN_TOOL=` is not needed when `nros` is on PATH;
CMake resolves it.

### west — Zephyr single-node examples

Zephyr owns the build. See
[Zephyr (west module)](../getting-started/integration-zephyr.md) for the
module wiring; the Rust leaves under `examples/zephyr/rust/` still need
`nros sync` first, because west drives cargo and cargo reads the leaf
config either way.

### In the checkout, or copied out?

The commands above are written as `cd <leaf>` inside the nano-ros
checkout, which is the fastest way to see something run. For anything
beyond that, copy the example directory out — examples are standalone
copy-out projects with no workspace walk-up, so a copied one builds on
its own.

The distinction matters for one reason: a bare `cargo build` writes
`target/` next to the leaf. In *your* project that is exactly right. In
the nano-ros checkout it is residue the repo's own gate rejects
(`check-example-leaf-target-dirs`) — in-tree builds are expected to go
through the contributor recipes (`just <module> …`), which write into a
shared build directory
instead. One in-repo `cargo build --release` of a Cortex-M example
leaves 269 MB behind.

So: exploring in the checkout, prefer the recipe. Building your own
thing, copy the example out and `cargo build` normally.

## Per platform

| platform | grid row | `nros setup <board>` | examples under | recipes (contributors) | starter page |
|---|---|---|---|---|---|
| Linux host | `linux` | `native` | `examples/native/` | `just native …` | [Native host build](../platform-guides/native-posix.md) |
| FreeRTOS (QEMU MPS2-AN385) | `freertos` | `qemu-arm-freertos` | `examples/qemu-arm-freertos/` | `just freertos …` | [FreeRTOS](../getting-started/freertos.md) |
| NuttX (Arm) | `nuttx` | `qemu-arm-nuttx` | `examples/qemu-arm-nuttx/` | `just nuttx …` | [NuttX](../getting-started/integration-nuttx.md) |
| NuttX (RISC-V) | `nuttx-riscv` | `qemu-riscv-nuttx` | `examples/qemu-riscv-nuttx/` | `just nuttx …` | [NuttX](../getting-started/integration-nuttx.md) |
| ThreadX (Linux sim) | `threadx-linux` | `threadx-linux` | `examples/threadx-linux/` | `just threadx_linux …` | [ThreadX](../getting-started/threadx.md) |
| ThreadX (QEMU RISC-V 64) | `threadx-riscv64` | `qemu-riscv64-threadx` | `examples/qemu-riscv64-threadx/` | `just threadx_riscv64 …` | [ThreadX](../getting-started/threadx.md) |
| Zephyr | `zephyr`, `zephyr-cortex-m` | `zephyr` | `examples/zephyr/` | `just zephyr …` | [Zephyr](../getting-started/integration-zephyr.md) |
| ESP32 | `qemu-esp32-baremetal` (single-node), `esp32` (workspace) | `qemu-esp32-baremetal` | `examples/qemu-esp32-baremetal/` | `just esp32 …` | [ESP32](../getting-started/esp32.md) |
| Bare-metal Cortex-M3 | `qemu-arm-baremetal` | `qemu-arm-baremetal` | `examples/qemu-arm-baremetal/` | `just qemu …` | [Bare-metal](../getting-started/bare-metal.md) |
| Arm FVP (Cortex-A SMP) | — | `zephyr` + a license-gated FVP binary | — | — | [ARM FVP](../getting-started/arm-fvp.md) |

Multi-node workspace examples do not follow that directory rule: they
all live under `examples/workspaces/`, selected by fixture row rather
than by directory. That is why ESP32 has two grid rows —
`qemu-esp32-baremetal` for its single-node examples and `esp32` for its
share of the workspace ones. Zephyr's two rows split differently:
`zephyr` and `zephyr-cortex-m` build the SAME examples for different
boards.

**Contributors (in-tree checkout):** each module's recipes are
discoverable rather than memorized:

```bash
just --list freertos          # every FreeRTOS recipe, grouped
just --list zephyr
```

## More than one node

The sequence above builds one leaf. A project with several nodes adds a
Bringup package and an Entry package on top of it, and the build is
still one of the three shapes above — see
[Project layout](../getting-started/workspace-from-app-node.md).

## If it does not work

- [Troubleshooting — First 10 Minutes](../getting-started/troubleshooting-first-10-min.md)
- [Troubleshooting](troubleshooting.md)
