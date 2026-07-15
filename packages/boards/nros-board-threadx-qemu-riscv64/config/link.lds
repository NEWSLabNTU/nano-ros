OUTPUT_ARCH( "riscv" )
ENTRY( _start )

SECTIONS
{
  /*
   * ThreadX runs in M-mode (no OpenSBI). QEMU -bios none loads -kernel at
   * 0x80000000 and starts executing there. entry.s / _start must be first.
   */
  . = 0x80000000;

  .init : {
    KEEP(*(.text.init))
  }

  .text : {
    KEEP(*(.init))
    *(.text .text.*)
    . = ALIGN(0x1000);
    PROVIDE(etext = .);
  }

  .rodata : {
    . = ALIGN(16);
    *(.srodata .srodata.*)
    . = ALIGN(16);
    *(.rodata .rodata.*)
  }

  /* Issue #195 — static-constructor table. The Cyclone message descriptors
   * register via `__attribute__((constructor))` TUs (register_std_msgs_*);
   * on this flat bare-metal image nothing walks .init_array unless the board
   * does it explicitly (board_threadx_qemu_riscv64.c), and the walk needs
   * these bounds. Previously the section landed as an ORPHAN (present in the
   * ELF, no symbols, never run) — descriptors stayed unregistered and
   * `nros_executor_register_subscription` failed -1 at reader creation. */
  .init_array : {
    . = ALIGN(8);
    PROVIDE(__init_array_start = .);
    KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
    KEEP(*(.init_array))
    PROVIDE(__init_array_end = .);
  }

  .data : {
    . = ALIGN(16);
    *(.sdata .sdata.*)
    . = ALIGN(16);
    *(.data .data.*)
  }

  .bss : {
    . = ALIGN(16);
    _bss_start = .;
    *(.sbss .sbss.*)
    . = ALIGN(16);
    *(.bss .bss.*)
    _bss_end = .;
  }

  .stack : {
    . = ALIGN(4096);
    _sysstack_start = .;
    . += 0x1000;
    _sysstack_end = .;
  }

  .heap (NOLOAD) : {
    . = ALIGN(16);
    __heap_start = .;
    __heap_end = .;
  }

  PROVIDE(_end = .);
  /* Issue #195 — newlib's canonical heap-base symbol. The Cyclone fixtures
   * link the SDK (newlib-built) libstdc++ whose libnosys `_sbrk` stub
   * references `end`; the stub never runs (picolibc + the ThreadX byte pool
   * own allocation), but the symbol must resolve. */
  PROVIDE(end = .);
}
