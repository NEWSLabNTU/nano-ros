# Add a board

<!-- type: how-to · level: 4 -->

**Needs first:** [How a build becomes an image](../background/how-a-build-becomes-an-image.md)

You have a board that nano-ros does not ship, on a platform it already
supports. By the end of this page, `nros board info` resolves your board from
your own tree, and an image built for it runs. If nano-ros has no platform for
your RTOS yet, [add the platform](add-a-platform.md) first.

## Before you start

- nano-ros is [installed](../start/install.md), and you have built and run an
  image on one of its own boards.
- You know which platform the board runs: `zephyr`, `freertos`, `nuttx`,
  `threadx`, `bare-metal` or `posix`.
- You know who owns your build. That decides what you write:

  | Your build | What you write |
  | --- | --- |
  | Zephyr, with `west` | A board package: a descriptor, a `package.xml`, Kconfig and devicetree fragments. No Rust crate. |
  | FreeRTOS, ThreadX or NuttX, driven by `nros build` | A board package and a small board crate on the family crate (`nros-board-freertos`, `nros-board-threadx`, `nros-board-nuttx`). |
  | Bare metal | A board package and a board crate of your own. There is no family crate; `nros-board-mps2-an385` is the reference. |
  | A vendor IDE or CMake tree | No board crate: [build inside your tree](../guide/build/existing-cmake-or-vendor-tree.md). |

## Steps

1. Scaffold the board package outside nano-ros:

   ```sh
   nros board new my-board --platform freertos --out-of-tree ./boards --workspace "$(nros locate nano-ros)"
   ```

   ```text
   wrote ./boards/nros-board-my-board

   next:
     export NROS_EXTRA_BOARD_PATH=./boards   # how this board is reached
   ```

   The command writes two files: the descriptor `nros-board.toml`, which says
   what the board lowers to, and a `package.xml` whose
   `<nano_ros_provides kind="board" name="my-board"/>` announces that the
   board exists. nano-ros needs both. For a Zephyr board, add
   `--west-board <board>/<soc>/<variant>`; the command then also writes an
   empty `prj.conf`. Outside a nano-ros source tree, `--workspace` is required
   even with `--out-of-tree`.

   The scaffold's last line names a check for nano-ros's own tree. It does not
   apply to an out-of-tree board.

2. Make the board reachable, and check that nano-ros finds it:

   ```sh
   export NROS_EXTRA_BOARD_PATH="$PWD/boards"
   nros board info my-board
   ```

   ```text
   {
     "name": "my-board",
     "board_dir": ".../boards/nros-board-my-board",
     "descriptor": ".../boards/nros-board-my-board/nros-board.toml",
     "cmake_vars": null
   }
   ```

   `NROS_EXTRA_BOARD_PATH` takes several directories, separated by `:`. Each
   has the layout of nano-ros's own board directory: one package directory per
   board. For a Zephyr board, `cmake_vars` holds the variables
   `nano_ros_use_board()` sets, so you can read what CMake will see without
   configuring anything.

3. Fill in the descriptor. The scaffold sets `names`, `platform`, `toolchain`,
   `platform_feature`, `entry_kind` and an empty `supported_netstacks`. Add
   what your board needs:

   - `board_crate`, the crate that holds the board's code, and
     `cargo_config`, the target triple and link flags its images build with.
   - `chip`, and `supported_netstacks`: who brings up the network. An empty
     list is a statement, not an omission: an image that names a network
     stack the board does not list is refused.
   - `[board.entry]`, when `entry_kind = "board-run"`: the crate and the
     `main` signature the generated entry calls. Copy the block from the
     in-tree board nearest yours.

   Then say how the board runs. A board states this once, and `nros run`,
   `nros flash` and the generated board pages read it:

   - `[board.run]` holds `emulator = "<kind>:<machine>"` (`qemu`, `fvp`,
     `renode` or `wokwi`) or `host = true`. A QEMU board adds
     `[board.run.qemu]`. Real silicon with no emulator twin leaves
     `[board.run]` out.
   - `[board.flash]` names the flasher `method` (`west`, `probe-rs`,
     `openocd`, `picotool`, …), the `chip` in that flasher's spelling, and the
     `prepare` steps a person does first. Each step is `say`, `wait_for` (an
     event such as `usb:<vid>:<pid>`) or `run`.
   - `[board.console]` gives the console `kind` (`stdio`, `serial`, `rtt` or
     `semihosting`), and for serial the adapter's `usb` id and `baud`.

   Rerun `nros board info my-board` after each edit. An unknown key fails the
   load and names the key, the file and every key that is allowed.

4. Write the board crate (every platform but Zephyr). A board is a marker type
   that implements four traits from `nros_platform`:

   | Trait | What your board does in it |
   | --- | --- |
   | `BoardInit` | `init_hardware()`: clocks, pin mux and peripherals, once, before the executor opens. |
   | `BoardPrint` | `println(args)`: write one line wherever the board's console is. |
   | `BoardExit` | `exit_success()` and `exit_failure()`: halt, reset or end the emulator. |
   | `BoardEntry` | `run(setup)`: bring the link up, open the executor, call `setup`, spin. On FreeRTOS, ThreadX and NuttX, delegate to the family crate's `run_entry`. |

   `BoardEntry::run_with_deploy` takes the image's network settings from its
   `system.toml`; implement it as well, so the board dials the locator the
   image names. The members and their contracts are in the
   [`nros_platform` rustdoc](../api/rust/nros_platform/index.html).

   On FreeRTOS, the family crate brings up lwIP and calls two C hooks for the
   network interface, `nros_board_register_netif` and
   `nros_board_poll_netif`. Its defaults are weak and report no Ethernet. Give
   strong definitions that attach your MAC driver to lwIP, or leave them out
   and reach the router over a serial locator.

5. On Zephyr, consume the board from the application's `CMakeLists.txt`, with
   one call before `find_package(Zephyr)`:

   ```cmake
   nano_ros_use_board(my-board)
   find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
   project(my_app LANGUAGES CXX)
   ```

   The call sets `BOARD` and the default RMW, and appends the board's
   `prj.conf`, Kconfig fragments and devicetree overlay, ahead of your own. It
   comes from nano-ros's Zephyr module, which `nros build` passes for you; a
   `west build` you run yourself names it with
   `ZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"`.

6. Build and run an image on the board:

   ```sh
   nros new sensor_ws --board my-board --template talker
   cd sensor_ws
   nros run sensor_node
   ```

   On hardware, use `nros flash --monitor` instead of `nros run`. Check the
   talker against a stock ROS 2 peer, as in
   [Talk to ROS 2](../tutorial/talk-to-ros2.md).

## Example

An STM32F429 with Ethernet, on bare metal, kept in its own repository. No
nano-ros lane boots this chip, so its verification is yours: the board stays
out of tree, and you upgrade nano-ros when you choose.

```text
my-stm32f4-board/
  Cargo.toml        # depends on nros and the bare-metal platform
  nros-board.toml   # the descriptor
  package.xml       # announces kind="board" name="my-stm32f4"
  build.rs          # emits the memory map
  memory.x          # FLASH and RAM origin and length for the chip
  src/lib.rs        # the board marker type, its trait impls and its entry
```

The descriptor. nano-ros supplies both the MAC driver and the IP stack here
(`smoltcp`), so nothing outside the crate brings up the network:

```toml
[[board]]
names               = ["my-stm32f4"]
platform            = "bare-metal"
toolchain           = "stable"
platform_feature    = "platform-bare-metal"
link_kind           = "none"
entry_kind          = "board-run"
supported_netstacks = ["smoltcp"]
chip                = "stm32f429"
board_crate         = "my-stm32f4-board"
cargo_config = '''
[build]
target = "thumbv7em-none-eabihf"

[target.thumbv7em-none-eabihf]
rustflags = ["-C", "link-arg=-Tlink.x"]
'''

[board.entry]
crate_name = "my_stm32f4_board"
signature  = "#[my_stm32f4_board::entry]\nfn main() -> !"

[board.flash]
method  = "probe-rs"
chip    = "STM32F429ZITx"
prepare = [
    { say = "Set BOOT0 to 1, then press RESET", wait_for = "usb:0483:df11" },
]

[board.console]
kind = "serial"
usb  = "0483:374b"
baud = 115200
```

The board's `Config` composes the shared network fields rather than declaring
them again. `BaseConfig` carries the MAC, IP, netmask, gateway, locator and
domain, and applies the image's settings, so your board stays in step with the
others when those fields change:

```rust
pub struct Config {
    pub base: nros_board_common::BaseConfig,
    pub phy_addr: u8,
}
```

The cost of this board is the network driver. On an RTOS board, the RTOS
brings up the NIC and nano-ros asks it for a socket. Here nothing else does,
so the MAC driver is part of your board. Budget for it first.

## Get your board into this book

A board kept out of tree needs nothing more: it runs from its descriptor. It
has no support status on the generated pages, because nano-ros holds no
evidence for it.

For nano-ros to carry a status for your board, contribute the board package
and the commands that build, flash and run it, by an issue or a pull request
on GitHub. The maintainers add the board's row to the board-support registry.
The row records where the board's image runs in evidence nano-ros holds
(nowhere, on the host, in an emulator or on hardware) and how often that run
happens. The status word on [Supported boards](../reference/supported-boards.md)
is derived from those facts and never typed. A board gets a page in this book
only when it has an emulator twin or a probe track that replays its chapter;
[Board support levels](../reference/board-support-levels.md) explains each
word.

## If it doesn't work

- **`has no sibling package.xml announcing a board`.** The descriptor has no
  announcement beside it. Write the `package.xml` the error prints, naming the
  descriptor's `names`, canonical first. `nros board new` writes it for you.
- **`no board named my-board`.** `NROS_EXTRA_BOARD_PATH` is unset in this
  shell, or names the package directory itself instead of the directory that
  holds it.
- **The Zephyr build ignores the board.** `nano_ros_use_board()` ran after
  `find_package(Zephyr)`. Zephyr reads `BOARD` and the fragments during that
  call, so move it above.
- **`could not auto-detect nano-ros workspace root`.** `nros board new` ran
  outside a nano-ros source tree without `--workspace`. Pass
  `--workspace "$(nros locate nano-ros)"`.

## See also

- [Add a platform](add-a-platform.md), for an RTOS nano-ros has no platform
  for.
- [Add a transport](custom-transport.md), for a link that is not Ethernet,
  serial or WiFi.
- [Supported boards](../reference/supported-boards.md) and the generated board
  pages.
- The in-tree boards, as working code to copy:
  [`nros-board-mps2-an385-freertos`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-mps2-an385-freertos)
  (FreeRTOS with its own Ethernet driver) and
  [`nros-board-mps2-an385`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-mps2-an385)
  (bare metal).

## Design background

- [RFC-0064: board support organization](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0064-board-support-organization.md): board packages, family crates and why an unverified board stays out of tree
- [RFC-0114: board support registry primitives](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0114-board-support-registry-primitives.md): the evidence a registry row records and the derived status word
- [RFC-0115: emulator-first board policy](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0115-emulator-first-board-policy.md): which boards the user book admits (D4)
- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md): `[board.run]`, `[board.flash]` and `[board.console]` (D4)
