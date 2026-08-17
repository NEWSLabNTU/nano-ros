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
   * `nros_executor_add_subscription` failed -1 at reader creation. */
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

  /* issue 0664 — a REAL heap, because the C library's malloc has a caller the
   * ThreadX byte pool cannot serve.
   *
   * This region was zero-sized on purpose: allocation on this board belongs to
   * the byte pool, and picolibc's malloc was never reached. The toolchain
   * `nros setup` provisions bundles NEWLIB, and CycloneDDS's `thread_states_init`
   * goes through libgcc's EMULATED TLS — `__emutls_get_address` calls plain
   * `malloc` and `abort()`s when it returns NULL. So every Cyclone image aborted
   * inside `dds_create_domain` before printing a line (issue 0664), which read
   * as a hang.
   *
   * emutls allocates one small block per TLS variable per thread, so this is
   * sized for that and not for application data — an app that wants a heap
   * still uses the byte pool. 64 KiB against 256 MiB of QEMU RAM. */
  .heap (NOLOAD) : {
    . = ALIGN(16);
    __heap_start = .;
    . += 0x10000;
    __heap_end = .;
  }

  PROVIDE(_end = .);
  /* Issue #195 — newlib's canonical heap-base symbol. The Cyclone fixtures
   * link the SDK (newlib-built) libstdc++ whose libnosys `_sbrk` stub
   * references `end`; the stub never runs (picolibc + the ThreadX byte pool
   * own allocation), but the symbol must resolve. */
  PROVIDE(end = .);
}
