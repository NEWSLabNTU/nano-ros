---
id: 2177
title: "The mps2-an385-zephyr image's network is the reader's to state, and nobody has measured the chapter's advice"
status: resolved
type: test-gap
severity: low
area: [zephyr, docs, testing]
related: [phase-517]
found: 2026-10-31
---

**Home: [phase-517](../../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W9's **Left**.

## What

`integration-zephyr.md` "Run on QEMU (mps2-an385)" has the reader build an
`[image.*] board = "mps2-an385-zephyr"` and boot it with
`west build -t run`. The chapter then says to state the image's address
(`10.0.2.15`, gateway `10.0.2.2`) and `locator = "tcp/10.0.2.2:<port>"`
themselves. The in-tree fixture lanes get the same network from
`packages/boards/nros-board-mps2-an385-zephyr/prj.conf`, which an installed
user's image does not read. The advice is written, not measured: no probe
track builds or boots this board, and the block carries no probe tags.

## Fix direction

* Have the board package supply the image's QEMU network the way it does for
  the fixture lanes (an RFC-0109 board network, like the NuttX and FreeRTOS
  QEMU boards), so the chapter has nothing for the reader to state.
* Then make the chapter's block a probe variant on the `zephyr` track, and
  boot it.

## Resolution (2026-10-31)

The board package now supplies the network, and the chapter's block is a probe
variant (`native-sim-rust`, probe 80 build, 90 boot) that builds the image with
`nros new entry zephyr_m3 --platform zephyr --board mps2-an385-zephyr` and boots
it with `west build -t run`. Measured locally before the probe ran: the image
boots under the Zephyr SDK's QEMU and prints `Publishing: N` / `I heard: N`
through a host `rmw_zenohd` on `10.0.2.2:7447`.

Getting there took nine defects on one road, each hidden behind the one before:

* `nros build` passed a board package's Zephyr files only to a GENERATED west
  application, on the assumption that a hand-written one calls
  `nano_ros_use_board()`. The application `nros new entry` scaffolds does not,
  so its image had no network driver, address or locator. It now gets them
  unless its own `CMakeLists.txt` calls that function; a standalone leaf is
  still excluded, because the module hook gives it the files.
* The board's `prj.conf` enabled `ETH_SMSC911X` but not the
  `NET_L2_ETHERNET` it depends on, so an application whose own conf did not
  turn it on got "No Qemu ethernet driver configured!".
* The scaffold's `prj-zenoh.conf` set `CONFIG_NROS_ZENOH_LOCATOR` to the
  Kconfig default and merged after the board's file, overriding its
  `tcp/10.0.2.2:7447`. The line is gone.
* The same file set `CONFIG_MAX_PTHREAD_MUTEX_COUNT=16`, below the floor the
  build derives for one subscriber (27); it is 64, like every in-tree leaf.
* `nros::main!` did not know `mps2-an385-zephyr`, `qemu-cortex-a53` or
  `fvp-aemv8r-smp` (`BOARD_PATHS` and `framework_for_board_key`), so a Rust
  image for any of them failed with "unknown board".
* The Zephyr Rust lane got no C cross compiler for the crates that build C
  (`zpico-sys`: `failed to find tool "arm-none-eabi-gcc"`). The C lane's
  `CC_<triple>` derivation is now one helper, `nros_cross_cc_env`, used by both.

The board's registry row moved from `scaffold` to the descriptor-only
directories (`infra`) in `packages/boards/board-support.toml`. `scaffold`
said "unfinished, do not build on these" about a board the chapter tells a
reader to build. `infra` carries no promise, so this is not a promotion: the
promise for this hardware stays the tier-2 `ZephyrQemuCortexM` row on
`nros-board-zephyr`, whose lanes build with this directory's `prj.conf`. That
is the same split as the `zephyr` descriptor directory and the tier-1
`ZephyrNativeSim` row.

* The first probe run still failed at `zpico-sys` (`failed to find tool
  "arm-none-eabi-gcc"`), because the carrier of the previous fix — and of every
  Rust-lane fact, `NROS_BOARD_FACTS_ENV` — is a line nano-ros's patch set adds
  to zephyr-lang-rust's cargo command, and the chapter's workspace (plain
  `west init` / `west update`) was never patched. Locally it built because the
  developer workspace was. The block now runs `nros setup board
  mps2-an385-zephyr --zephyr-workspace "$NROS_ZEPHYR_WORKSPACE"` first, and the
  board descriptor gained the `[board.provisioning]` block that command needs
  (`zephyr_line = "3.7"`, `rust_targets = ["thumbv7m-none-eabi"]`); before, it
  refused the board as "not a Zephyr consumer board".
* And that command could not work on an installed SDK at all: `patches/3.7.sh`
  runs five cyclonedds steps against nano-ros's own `third-party/dds/cyclonedds`,
  which `<prefix>/share/nano-ros` does not ship, so the first one exited 1 and
  `set -e` aborted the set before it reached anything else. Measured on a staged
  SDK root. Those steps now skip, saying so, when the tree is absent; the pinned
  fork already carries each change, and the 4.4 set runs none of them.

* With the set applied, the `zephyr` crate itself failed: `GpioPin::new`
  "takes 6 arguments but 5 arguments were supplied", issue 0432's gpio
  devicetree arity. Its fix, `zephyr-lang-rust-gpio-patch.sh`, was run by every
  `just` lane beside the AArch64 and Cortex-R patches but was never in
  `patches/3.7.sh`, the set `nros setup board` applies. It is now; a sweep of
  the lanes' patch calls against the set finds no other one missing.

  The native_sim road has the same unpatched workspace and passes anyway,
  silently building with each crate's defaults: issue 2185.

Two fixes reach a generated west application (`nros new --image … --board
mps2-an385-zephyr`, no bringup `boards/<board>/` directory): it now carries a
`prj.conf` stating `CONFIG_NROS` (and `CONFIG_RUST` for a Rust one), which
Zephyr requires, and the three Zephyr board descriptors gained the
`[board.entry]` crate-root lines the generic `zephyr` board has. That road
still needs the RMW's Kconfig (`POSIX_API`, the pthread pools) from somewhere
when the bringup has no board directory; the chapter does not use it.

Observed, not ours: zephyr-lang-rust's `zephyr-build` prints
`cargo-rerun-if-changed=<.config>` (no colon), so cargo never re-runs the
`zephyr` crate's build script when `.config` changes, and a build first
configured without `CONFIG_RUST` keeps failing "CONFIG_RUST must be set" after
the conf is fixed.
