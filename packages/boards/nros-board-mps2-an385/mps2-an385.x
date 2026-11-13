/* Memory layout for MPS2-AN385 (QEMU Cortex-M3 with LAN9118 Ethernet)
 *
 * This linker script defines the memory map for bare-metal applications
 * running on the MPS2-AN385 machine in QEMU. cortex-m-rt's `link.x` includes
 * it as `memory.x`; a C-rooted image reaches it through `c-runtime.x`, which
 * includes `link.x`.
 *
 * Memory Map:
 *   0x0000_0000 - 0x003F_FFFF : Flash (4MB)
 *   0x2000_0000 - 0x203F_FFFF : SRAM (4MB)
 *
 * SRAM, bottom to top (phase-553 W7, issue 2494):
 *
 *   .data, .bss, .uninit          the statics, including the Rust heap
 *                                 (`nros_platform_mps2_an385::memory::HEAP`)
 *   [__sheap, _nros_stack_guard)  free; in a C-rooted image, newlib's `_sbrk`
 *                                 heap, which grows up and is capped at the
 *                                 guard (the board's `_sbrk`, `c/c_main.c`)
 *   [_nros_stack_guard, _stack_end)
 *                                 the guard: an MPU region with no access,
 *                                 armed at boot (`stack_guard::install`)
 *   [_stack_end, _stack_start)    the main stack, `_stack_size` bytes, which
 *                                 grows down from the top of RAM
 *
 * `_stack_size` is stated, not measured: `nros-stack.x`, which the board's
 * build script writes into its OUT_DIR, sets it from `NROS_BAREMETAL_STACK_KB`
 * (default in `build.rs`). A stack that grows past it writes into the guard,
 * faults, and the board's HardFault handler reports the overflow and stops the
 * image. Before this, the stack had no size and no guard, so an overflow wrote
 * over the statics with no diagnostic.
 */

MEMORY
{
    /* Flash: 4MB at 0x00000000 */
    FLASH : ORIGIN = 0x00000000, LENGTH = 4M

    /* SRAM: 4MB at 0x20000000 */
    RAM : ORIGIN = 0x20000000, LENGTH = 4M
}

/* `_stack_size`, from the board's build script. */
INCLUDE nros-stack.x

/* The guard below the stack. An MPU region must be a power of two in size
 * and aligned to its size; `_stack_start` is the top of RAM and `_stack_size`
 * is a whole number of KiB, so a 1 KiB guard below the stack is aligned. A
 * frame larger than the guard can still step over it: the guard catches a
 * stack that GROWS past its limit, not a single allocation bigger than 1 KiB.
 */
_nros_stack_guard_size = 1K;

_stack_start = ORIGIN(RAM) + LENGTH(RAM);
/* cortex-m-rt PROVIDEs `_stack_end` as the end of the statics; the stack's
 * real lower bound is here. */
_stack_end = _stack_start - _stack_size;
_nros_stack_guard = _stack_end - _nros_stack_guard_size;

ASSERT(_stack_size % 1024 == 0, "
nros: the main stack size must be a whole number of KiB, so that the guard
below it is aligned for the MPU. Set NROS_BAREMETAL_STACK_KB.");

ASSERT(__sheap <= _nros_stack_guard, "
nros: the image's statics run into the main stack and its guard. Lower
NROS_BAREMETAL_STACK_KB, or shrink the statics (the Rust heap is
NROS_HEAP_SIZE).");
