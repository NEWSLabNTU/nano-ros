/* Memory layout for STM32F429ZI (Nucleo-F429ZI)
 *
 * This linker script defines the memory map for bare-metal applications
 * running on STM32F4 MCUs with Ethernet.
 *
 * Memory Map (STM32F429ZI):
 *   0x0800_0000 - 0x081F_FFFF : Flash (2MB)
 *   0x2000_0000 - 0x2002_FFFF : SRAM1 (192KB)
 *   0x1000_0000 - 0x1000_FFFF : CCM RAM (64KB, not DMA-accessible)
 *
 * Allocations (within SRAM):
 *   Stack: 8KB at end of RAM
 *   Heap: 64KB for zenoh-pico allocator
 */

MEMORY
{
    /* Flash: 2MB at 0x08000000 */
    FLASH : ORIGIN = 0x08000000, LENGTH = 2M

    /* SRAM1: 192KB at 0x20000000 (DMA-accessible) */
    RAM : ORIGIN = 0x20000000, LENGTH = 192K
}

/* Stack configuration */
_stack_size = 8K;

/* Heap configuration for zenoh-pico allocator */
_heap_size = 64K;

/* Export symbols for runtime */
_heap_start = ORIGIN(RAM) + LENGTH(RAM) - _stack_size - _heap_size;
_heap_end = ORIGIN(RAM) + LENGTH(RAM) - _stack_size;
