# Adding a Board to the Registry

<!-- type: how-to · level: 4 -->

Every support claim the book makes about a board (its status, breadth,
emulator and run line) is generated from one row of the board-support
registry and the board's own descriptor. A board you add out of tree needs no
row: it runs from its descriptor's `[board.run]`, `[board.flash]` and
`[board.console]` tables. A board contributed to nano-ros gets a row, so its
status appears on [Supported Boards](../reference/supported-boards.md).

## How to add a new board

1. **Pick the matching RTOS path.** Cortex-M3, M4 or M7 with an RTOS: start
   from the FreeRTOS or Zephyr starter. Cortex-M0+: the bare-metal starter,
   which is limited (see the Cortex-M0+ caveat on
   [Supported Boards](../reference/supported-boards.md#caveats-by-chip-family)).
   Cortex-A or RISC-V64: NuttX or Zephyr. Xtensa has no supported path.
2. **Start from an existing board package, or scaffold one.** The board
   packages nano-ros ships cover the QEMU machines and the reference
   development kits; `nros board list` names them and `nros board info <board>`
   prints one's resolved descriptor. For a board of your own, scaffold a
   package outside nano-ros:

   ```sh
   nros board new my-board --platform freertos --out-of-tree ./boards
   ```

   It prints the `NROS_EXTRA_BOARD_PATH` value through which nano-ros finds
   the package. A real-hardware board needs a thin package that supplies
   startup code, a linker script and `BoardIdle::wfi()` (bare-metal), or that
   wraps the RTOS's BSP (FreeRTOS, Zephyr). The
   [Custom Board Package](custom-board.md) page describes its parts, and the
   [STM32F4 out-of-tree worked example](stm32f4-out-of-tree.md) is a complete
   one.
3. **Say how the board runs.** The descriptor, `nros-board.toml`, states it
   once: `[board.run]` for a host process or an emulator, `[board.flash]` and
   `[board.console]` for real silicon. `nros run` and the generated board
   pages read those tables; nothing else repeats them.
4. **Build and run the talker and listener.** Use your platform chapter's
   example, and check the pair against a stock ROS 2 peer (for zenoh,
   `RMW_IMPLEMENTATION=rmw_zenoh_cpp`).
5. **Contribute it, if you want a status.** A board that stays out of tree
   works without a registry row; it simply has no status on the generated
   pages. To have nano-ros carry a status for it, open an issue or a pull
   request on GitHub with the board package and the commands that build,
   flash and run it. The maintainers add its registry row, and the status
   word is derived from the evidence the row records.

## What a registry row records

A row records where the board's image runs in the evidence nano-ros holds
(nowhere, on the host, in an emulator, or on hardware), how often that run
happens (on every merge, daily, weekly, or by hand, with the date of the last
manual run), and, for a board that inherits its breadth from its platform
family, which family's evidence it inherits. The status word is generated from those facts and is
never typed by hand. [Board Support Levels](../reference/board-support-levels.md)
lists the words and what each means.

## Design background

- [RFC-0114: board support registry primitives](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0114-board-support-registry-primitives.md)
- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md) — the `[board.run]`, `[board.flash]` and `[board.console]` tables
