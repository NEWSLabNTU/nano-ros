# Entry packages

An **Entry pkg** is the binary that boots a topology on a specific board.
Where a Node pkg is a library (no `fn main`) and a Bringup pkg is purely
declarative, the Entry pkg is the one thing that actually runs: it names a
board, wires the runtime, and — for multi-node setups — points at a Bringup
pkg that describes which nodes should be launched.

You have one Entry pkg per deploy target. A workspace targeting both a native
workstation and an STM32F4 board has two Entry pkgs that reference the same
Node pkgs; only the board and (optionally) the launch target differ.

## Prereqs

Pick one path from a fresh checkout — `just` is NOT a prereq.

**A. Bare machine** (no Rust, no `just`, no cargo):
```sh
./scripts/bootstrap.sh base
```
Installs rustup, just, builds the in-tree `nros` CLI at
`packages/cli/target/release/nros`, leaves the binary on PATH for
this shell.

**B. Already have cargo** (most contributors):
```sh
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
export PATH="$PWD/packages/cli/target/release:$PATH"
```

**C. Tagged release, no Rust at all**:
```sh
./scripts/install-nros-prebuilt.sh
```
Downloads the matching `nros-<triple>.tar.gz` from the GitHub release,
sha256-verifies, installs to `packages/cli/target/release/nros`.

Every subsequent shell sources the workspace env via one of:
```sh
direnv allow                  # if you use direnv
source ./activate.sh          # bash / zsh
source ./activate.fish        # fish
```

Then provision the native host (the canonical first Entry pkg target;
for STM32F4 / Zephyr / ESP32 swap in the matching `nros setup` board):
```sh
nros setup native --rmw zenoh
```

## Package layout

```
src/native_entry/
├── package.xml
├── Cargo.toml           # [[bin]] + deps on node pkgs + board crate
│                        # + [package.metadata.nros.entry]
└── src/main.rs          # nros::main!(launch = "demo_bringup");
```

No library code lives here. The Entry pkg links the Node pkg rlibs and hands
them to the runtime that `nros::main!()` generates.

## `Cargo.toml` metadata

The `[package.metadata.nros.entry]` table tells the CLI which deploy target
this binary is built for. The embedded example
`examples/stm32f4/rust/talker-embassy/` uses:

```toml
[package.metadata.nros.entry]
deploy = "embassy-stm32f4"
```

A native Entry pkg that references a Bringup pkg looks like:

```toml
[package.metadata.nros.entry]
deploy = "native"

[package.metadata.nros.deploy.native]
board     = "posix"
rmw       = "zenoh"
domain_id = 0
```

`deploy` is the key that `nros check` and the Entry macro use to
find the board crate and verify the topology. Keep it short and descriptive —
it becomes the identifier in `nros plan` output and in `system.toml`'s
`[deploy.<name>]` table when you later add a Bringup pkg.

## `nros::main!()` — four forms

```rust
// 1. Single-node self-bringup: reads [package.metadata.nros.entry] deploy
//    from Cargo.toml and boots the Node pkg that is the only member of
//    this workspace (or the one marked default).
nros::main!();

// 2. Single-node, explicit board type.
nros::main!(board = NativeBoard);

// 3. Multi-node: reference a Bringup pkg; boot its default launch file
//    (the one listed under [system] in system.toml).
nros::main!(launch = "demo_bringup");

// 4. Multi-node, explicit launch file.
nros::main!(launch = "demo_bringup:sim.launch.xml");

// 5. Full form: board + launch file + runtime arg overrides.
nros::main!(board = NativeBoard, launch = "demo_bringup:sim.launch.xml", args = [("use_sim","true")]);
```

The macro reads `[package.metadata.nros.entry]` at compile time to select the
right board and executor backend. On Embassy / RTIC targets it emits the
framework-specific `#[embassy_executor::main]` or `#[rtic::app]` body so your
`src/main.rs` stays a single line.

The real `examples/stm32f4/rust/talker-embassy/src/main.rs` collapses to
exactly this:

```rust
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
```

## Escape hatch

If you need more control than the macro provides — custom startup ordering,
hardware init before the runtime, or a fully manual executor loop — you can
bypass `nros::main!()`:

```rust
// Option A: delegate init to the board crate, supply your own closure.
<NativeBoard as BoardEntry>::run(|runtime| {
    let node = runtime.create_node("talker", "/", &Default::default())?;
    // ...
    Ok(())
});

// Option B: fully manual — no board crate.
let executor = nros::Executor::open(&ExecutorConfig::default())?;
// wire nodes, spin manually ...
```

Option A is the right choice when you need to run something before the first
spin (e.g. DMA setup, flash unlock). Option B is there for board-bringup
authors adding a new platform.

## Running a native Entry pkg

The verified path for the canonical Rust workspace is `cargo run -p native_entry`.
Start a Zenoh router first, then boot the Entry binary from the workspace root:

```bash
# in another shell:
zenohd --listen tcp/127.0.0.1:7447 &

cargo run -p native_entry
```

`native_entry` opens the executor against the router, registers `talker` +
`listener` (composed into a single process), and runs the topology.

The canonical Rust workspace is at `examples/workspaces/rust/`.
For Zephyr, QEMU, ESP-IDF, and other non-native targets, use the platform's
native build/run tool or the focused `just <plat> run` recipe.

## Running on Zephyr

On Zephyr the RTOS framework *is* the workflow: `west build` is the build
verb, Kconfig selects the RMW, and the Entry is an ordinary Zephyr
application. There is no `nros build` / `nros launch` build path here, and you
do not type the RMW as a Cargo `--features` flag or bake the board into the
package.

```sh
source ./activate.sh

# Provision message bindings once. This is platform-agnostic workspace
# provisioning (sibling to `west update` / `rosdep`), NOT a compile step —
# the same `nros ws sync` output feeds every board and every RMW.
nros ws sync

# west is the build verb. Choose the board with `-b`, and select the RMW with
# the matching Kconfig overlay via -DCONF_FILE. The Entry source never changes.
west build -b native_sim/native/64 src/zephyr_entry \
    -- -DCONF_FILE="prj.conf;prj-zenoh.conf"

west build -t run            # native_sim; `west flash` for hardware
```

Swap `-b native_sim/native/64` for any other Zephyr board (`-b nrf52840dk/nrf52840`,
`-b stm32f4_disco`, …) and `prj-zenoh.conf` for `prj-xrce.conf` /
`prj-cyclonedds.conf` to pick a different RMW — nothing else changes. **One
Zephyr Entry pkg (`src/zephyr_entry/`) covers every Zephyr board**: unlike the
board-specific FreeRTOS / ThreadX Entries, Zephyr owns its board abstraction,
so the board is chosen at `west build -b` time rather than baked into the
package (see [One Entry pkg per board](#one-entry-pkg-per-board)).

The Entry source is identical to the native, FreeRTOS, and ThreadX Entries —
the same one-line launch macro, with the launch file as the single source of
truth for the node set:

```rust
// examples/workspaces/rust/src/zephyr_entry/src/lib.rs
nros::main!(launch = "demo_bringup:system.launch.xml");
```

## One Entry pkg per board

Each deploy target gets its own Entry pkg. A workspace that runs on both
`native` and `embassy-stm32f4` would have two Entry pkgs that share the same
Node pkg library:

| Entry pkg | `deploy` key | Board crate |
|---|---|---|
| `native_entry` | `"native"` | `nros-board-posix` |
| `stm32f4_entry` | `"embassy-stm32f4"` | `nros-board-embassy-stm32f4` |

Both reference the same `talker_pkg` and `listener_pkg` Node pkg rlibs. The
board crate provides the `BoardEntry` impl and any hardware-specific
initialisation; the Node pkgs are board-agnostic.

**Zephyr is the exception — one Entry per *RTOS*, not per board.** Zephyr
already owns its board abstraction, so a single `zephyr_entry` covers
`native_sim`, `nrf52`, `stm32`, `aemv8r`, … with the board chosen at
`west build -b <board>` time. Contrast FreeRTOS / ThreadX, whose board crates
are board-specific, so each of those Entries bakes one board. See
[Running on Zephyr](#running-on-zephyr).

The `examples/stm32f4/rust/talker-embassy/` example demonstrates the
embedded shape: `deploy = "embassy-stm32f4"` + `nros::main!();` on a
`no_std / no_main` binary that delegates everything to the `EmbassyStm32F4`
board crate.

## C / C++ Entry packages

C and C++ Entry packages use the same role split through CMake. See
[C / C++ multi-node workspaces](./workspace-cpp.md).

## Where to go next

- [Role reference](../user-guide/component-and-entry-pkg.md) — full reference for all three roles.
- [Bringup packages](./workspace-bringup.md) — the `system.toml` + launch XML that an Entry pkg points at.
- [Node packages](./workspace-node-pkgs.md) — the Node pkgs your Entry pkg links.
- [Project layout](./workspace-from-app-node.md) — the full 3-role picture and when to use it.
