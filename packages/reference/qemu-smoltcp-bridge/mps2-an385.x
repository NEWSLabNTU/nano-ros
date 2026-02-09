/* Memory layout for MPS2-AN385 (QEMU Cortex-M3 with LAN9118 Ethernet)
 *
 * This linker script defines the memory map for bare-metal applications
 * running on the MPS2-AN385 machine in QEMU.
 *
 * Memory Map:
 *   0x0000_0000 - 0x003F_FFFF : Flash (4MB)
 *   0x2000_0000 - 0x203F_FFFF : SRAM (4MB)
 *
 * Allocations (within SRAM):
 *   Stack: 8KB at end of RAM
 *   Heap: 64KB for embedded-alloc
 *   Ethernet buffers: 16KB for smoltcp
 */

MEMORY
{
    /* Flash: 4MB at 0x00000000 */
    FLASH : ORIGIN = 0x00000000, LENGTH = 4M

    /* SRAM: 4MB at 0x20000000 */
    RAM : ORIGIN = 0x20000000, LENGTH = 4M
}

/* Stack configuration */
_stack_size = 8K;

/* Heap configuration for embedded-alloc */
_heap_size = 64K;

/* Export symbols for runtime */
_heap_start = ORIGIN(RAM) + LENGTH(RAM) - _stack_size - _heap_size;
_heap_end = ORIGIN(RAM) + LENGTH(RAM) - _stack_size;
