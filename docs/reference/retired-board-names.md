# Retired board names — what each one became, and why

Phase-437 applied [RFC-0093](../design/0093-board-naming-states-reach.md):
a board name is a claim about where its artifact runs, and the claim must be
true. Sixteen `[board.*]` index keys became twelve, and nine names retired.

This page exists so that a name you meet in a two-month-old issue, a stale
branch, a shell history or an archived doc leads you to the name that resolves
today. Archived issues and dated measurements deliberately keep the old
spelling — that is what makes this ledger necessary rather than optional.

The rules cited below are RFC-0093's:

| rule | what it says |
| --- | --- |
| R1 | name the narrowest thing the build is true for — **system**, **part** or **machine model** |
| R2 | `<where>-<stack>`, stack always last |
| R3 | never name the emulator |
| R4 | prefer the vendor's own name, and R4 outranks R5 |
| R5 | leave out what the index already carries as a field (`arch`, `platform`) |
| R6 | a fixture `platform =` names a FAMILY, never a board |

## Board names

Each row's *new* name is an `[board.*]` key in `nros-sdk-index.toml`, a
`cmake/board/nano-ros-board-<name>.cmake` overlay, an `examples/<name>/`
directory and the `board=` value in that tree's `package.xml` exports — one
name across all four, which is the point.

| retired | now | rule | why |
| --- | --- | --- | --- |
| `qemu-arm-baremetal` | `mps2-an385-baremetal` | R1 R3 R5 | Promised any ARM under QEMU; the overlay links `mps2-an385.x` for one Cortex-M3 part. R3 drops the emulator, R5 drops `arm` because `arch` is already a field. |
| `mps2-an385` | `mps2-an385-baremetal` | R2 | The same board under its board-crate spelling. The part was right; the stack suffix was missing, and this part also carries FreeRTOS and Zephyr, so the bare name could not say which build it meant. |
| `qemu-arm-freertos` | `mps2-an385-freertos` | R1 R3 R5 | Promised any ARM + QEMU + FreeRTOS; links `mps2_an385.ld`. One part, and `mps2-an385-freertos` was already the other spelling of it. |
| `qemu-arm-nuttx` | `qemu-armv7a-nuttx` | R1 R4 | One machine, not an architecture: `CONFIG_ARCH_BOARD="qemu-armv7a"`, PL011 at `0x9000000` IRQ 33, RAM at `0x40200000`. The word `qemu` survives here **only** because it is NuttX's own board name — R4 outranks R3. |
| `nuttx-qemu-arm` | `qemu-armv7a-nuttx` | R2 R4 | Same board, cmake-overlay spelling, stack first. |
| `qemu-riscv-nuttx` | `rv-virt-nuttx` | R1 R4 | NuttX's own `CONFIG_ARCH_BOARD="rv-virt"`. |
| `nuttx-qemu-riscv` | `rv-virt-nuttx` | R2 R4 | Same board, cmake-overlay spelling, stack first. |
| `qemu-riscv64-threadx` | `rv-virt-threadx` | R1 R3 R4 | One machine. ThreadX ships no board layer, so unlike the NuttX pair there is no vendor BSP name to borrow — `virt` comes from QEMU's machine list instead. |
| `riscv64-qemu` | `rv-virt-threadx` | R1 R2 | The clearest over-promise in the set: it named an ISA and delivered one machine — UART `0x10000000`, CLINT `0x02000000`, virtio-MMIO `0x10001000`, and a write to QEMU's `test-finisher` at `0x100000`, a device no silicon has. |
| `qemu-esp32-baremetal` | `esp32-c3-baremetal` | R1 R3 | `cmake/board/nano-ros-board-esp32-c3-baremetal.cmake` is *"board overlay for ESP32-C3 (real silicon) + ESP32-C3 QEMU"* — one name, both, because the name is the part. |
| `esp32-c3` | `esp32-c3-baremetal` | R2 | The part was right; the stack suffix was missing. |

`qemu-esp32-baremetal` was also a live `deploy=` token in two manifests. It
moved with its board (RFC-0093 §3's one exception): leaving it behind would not
have preserved a separate vocabulary, only a third spelling of one thing.
Deploy values that are **not** board keys — `native`, `freertos`, `nuttx`,
`threadx-linux`, `zephyr`, `qemu-mps2-an385`, `rtic-mps2-an385`,
`threadx-qemu-riscv64`, `esp32-qemu` — were untouched.

## Fixture `platform =` coordinates — a different axis

`examples/fixtures.toml`'s `platform =` is a lane COORDINATE, not a board. Ten
of its twelve values already named a platform family; two named a board. Under
R6 they take family names, and the ESP32 half **removes** a duplicate rather
than renaming one — `PlatformId::Esp32Qemu` mapped to `esp32` *and*
`qemu-esp32-baremetal`, one platform wearing two coordinates.

| retired coordinate | rows | now | why |
| --- | ---: | --- | --- |
| `qemu-arm-baremetal` | 21 | `baremetal` | The only bare-metal family, so the plain word is free. |
| `qemu-esp32-baremetal` | 3 | `esp32` | Collapses into the coordinate that already existed. Renaming it to `esp32-c3-baremetal` would have added a *third* spelling. |

Twelve fixture platforms became eleven. The coordinate is also a directory
name (`build/cargo-fixtures/<platform>`), so **`build/cargo-fixtures/qemu-arm-baremetal`
is now `build/cargo-fixtures/baremetal`** — a stale path there is valid-looking
and empty rather than an error.

## Strings that did NOT retire

Two of the names above look retired and are not. Only the **index key** moved.

**`mps2-an385`** is alive in 659 tracked files and every one of these uses is
correct:

* the QEMU machine argument — `qemu-system-arm -machine mps2-an385`;
* the linker scripts `packages/boards/nros-board-mps2-an385/mps2-an385.x` and
  `packages/testing/qemu-smoltcp-bridge/mps2-an385.x`;
* the peripheral-access crate `mps2-an385-pac`;
* the Zephyr fragments `cmake/zephyr/mps2-an385.conf` and
  `cmake/zephyr/mps2-an385-serial.conf`;
* the deploy tokens `qemu-mps2-an385` and `rtic-mps2-an385`;
* the board crate directory `packages/boards/nros-board-mps2-an385/`.

**`esp32-c3`** is the chip, and stays the chip: 34 tracked files use it outside
the `esp32-c3-baremetal` board name.

**Board crate directories did not move.** `nros-board-mps2-an385`,
`nros-board-nuttx-qemu`, `nros-board-threadx-qemu-riscv64` and
`nros-board-esp32-qemu` keep their names — a crate is reached by
`nano_ros_use_board()` from the overlay, and renaming a crate directory
silently changes an alias `board_descriptor.rs::directory_alias()` honours. So
a `nros-board-*` directory whose name contains `qemu` is not evidence of a
missed rename.

## Checking rather than trusting this page

    nros setup <board> --dry-run      # resolves, or lists every key that does
    just check board-name-reach       # the gate; RFC-0093 §5, empty baseline
    just check board-vocabulary       # every board= export resolves to a key

`check-board-name-reach` is what keeps the rule from decaying back into two
vocabularies. It reads the pins rather than a list — a linker script, a memory
base or a hex MMIO literal in the overlay makes a board a part-or-machine
board, and its name must then not be an arch or an ISA alone.

## See also

* [RFC-0093 — A board name states the reach of its build](../design/0093-board-naming-states-reach.md)
* [phase-437 — rename every board to the reach its build has](../roadmap/archived/phase-437-board-name-reach-rename.md)
* [Supported boards](../../book/src/reference/supported-boards.md) — the
  user-facing table
