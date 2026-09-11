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
ip      = "10.0.2.10"
```

Everything that choice implies — the target triple, the linker flags,
the QEMU runner, the cross compiler for the `cc` crate, the pool sizes
derived from the components, the `[patch.crates-io]` table that resolves
nano-ros's registry-style names into your checkout — is **generated**,
never written by hand. Retargeting is editing the `board` line: in a
workspace that is the whole edit, and in a single-package Rust leaf the
board crate in `[dependencies]` moves with it, because such a leaf is
its own entry
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md)).

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

You need it **once per checkout location**, not once per build. Re-run
it after editing a `.msg`, `.srv`, or `.action` file, after changing
`system.toml`, and after moving the checkout.

## Which builder your cell uses

Each cell is the builder that nano-ros's own CI uses for that pair, read
from `examples/fixtures.toml` — the manifest the fixture builds and the
staleness probe both consume. A dash means the pair has no in-tree
coverage today, not that it is forbidden.

The row names are the manifest's, which are shorter than the ones you
type: `freertos` here is the platform whose board is
`mps2-an385-freertos` and whose examples live in
`examples/mps2-an385-freertos/`. The per-platform table further down maps
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
nros build                      # every [image.*]; or: nros build <image-id>
```

The artifact lands under `build/`, keyed on the image:
`<leaf>/build/<image-id>/target/[<triple>/]<profile>/<bin>`. **No
`--target` on the command line, on any platform** — the board's triple
is in the generated settings file, which is also where its linker flags
and its QEMU runner live.

If you would rather drive cargo yourself — an IDE, a CI step,
`--release` — run `nros sync` and then point cargo at that file. Run it
from the directory *above* the package, so the package's own `.cargo/`
is not read a second time; phase-445 W6 deletes that directory, and then
the working directory stops mattering:

```bash
cd <leaf-parent>
cargo build --manifest-path <leaf>/Cargo.toml \
            --config <leaf>/build/<image-id>/nros-cargo.toml
```

`cargo run` through the same file works for a QEMU board — the runner is
in it.

### cmake — C and C++

```bash
./scripts/bootstrap.sh
source ./activate.sh
nros setup <board> --rmw zenoh

cd <leaf>
cmake -B build -DCMAKE_TOOLCHAIN_FILE=<toolchain> -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
```

No `nros sync`, and no `-DNANO_ROS_BOARD` / `-DNROS_RMW` either:
`find_package(nano_ros)` reads the leaf's `system.toml` and derives the
platform from the board it names. The toolchain file is still per
platform — see the starter page. `-D_NANO_ROS_CODEGEN_TOOL=` is not
needed when `nros` is on PATH; CMake resolves it.

`nros build` does not yet work in a *single-package* C or C++ leaf
([issue 1296](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1296-nros-build-c-leaf-bringup-name-mismatch.md));
it is the verb for a C/C++ **workspace**, where it drives exactly this
cmake pair for you.

### west — Zephyr single-node examples

Zephyr owns the build. See
[Zephyr (west module)](../getting-started/integration-zephyr.md) for the
module wiring; the Rust leaves under `examples/zephyr/rust/` still need
`nros sync` first, because west drives cargo and cargo needs the
generated message crates either way.

### In the checkout, or copied out?

The commands above are written as `cd <leaf>` inside the nano-ros
checkout, which is the fastest way to see something run. For anything
beyond that, copy the example directory out — examples are standalone
copy-out projects with no workspace walk-up, so a copied one builds on
its own.

Either way the build output goes to the same place — `build/<image>/`,
under the leaf, because the generated settings file names that
`target-dir` — so an in-tree build leaves no `target/` beside the
sources for the repo's own gate to reject
(`check-example-leaf-target-dirs`). A copied-out example carries no path
into the checkout at all: `NROS_REPO_DIR` is the only thing tying it to
one, and it is on the command line rather than in a file.

So: exploring in the checkout, `cd <leaf> && nros sync && nros build`.
Building your own thing, copy the example out and run the same two
commands there.

## Per platform

| platform | grid row | `nros setup <board>` | examples under | recipes (contributors) | starter page |
|---|---|---|---|---|---|
| Linux host | `linux` | `native` | `examples/native/` | `just native …` | [Native host build](../platform-guides/native-host.md) |
| FreeRTOS (QEMU MPS2-AN385) | `freertos` | `mps2-an385-freertos` | `examples/mps2-an385-freertos/` | `just freertos …` | [FreeRTOS](../getting-started/freertos.md) |
| NuttX (Arm) | `nuttx` | `qemu-armv7a-nuttx` | `examples/qemu-armv7a-nuttx/` | `just nuttx …` | [NuttX](../getting-started/integration-nuttx.md) |
| NuttX (RISC-V) | `nuttx-riscv` | `rv-virt-nuttx` | `examples/rv-virt-nuttx/` | `just nuttx …` | [NuttX](../getting-started/integration-nuttx.md) |
| ThreadX (Linux sim) | `threadx-linux` | `threadx-linux` | `examples/threadx-linux/` | `just threadx_linux …` | [ThreadX](../getting-started/threadx.md) |
| ThreadX (QEMU RISC-V 64) | `threadx-riscv64` | `rv-virt-threadx` | `examples/rv-virt-threadx/` | `just threadx_riscv64 …` | [ThreadX](../getting-started/threadx.md) |
| Zephyr | `zephyr`, `zephyr-cortex-m` | `zephyr` | `examples/zephyr/` | `just zephyr …` | [Zephyr](../getting-started/integration-zephyr.md) |
| ESP32 | `esp32` | `esp32-c3-baremetal` | `examples/esp32-c3-baremetal/` | `just esp32 …` | [ESP32](../getting-started/esp32.md) |
| Bare-metal Cortex-M3 | `baremetal` | `mps2-an385-baremetal` | `examples/mps2-an385-baremetal/` | `just qemu …` | [Bare-metal](../getting-started/bare-metal.md) |
| Arm FVP (Cortex-A SMP) | — | `zephyr` + a license-gated FVP binary | — | — | [ARM FVP](../getting-started/arm-fvp.md) |

Multi-node workspace examples do not follow that directory rule: they
all live under `examples/workspaces/`, selected by fixture row rather
than by directory. So ESP32's single-node and workspace examples share
one grid row, `esp32`: a grid row names a platform FAMILY, never a board
(RFC-0093 R6), and `esp32` carries both. Zephyr is the case that does
split — `zephyr` and `zephyr-cortex-m` build the SAME examples for
different boards.

**Contributors (in-tree checkout):** each module's recipes are
discoverable rather than memorized:

```bash
just --list freertos          # every FreeRTOS recipe, grouped
just --list zephyr
```

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
