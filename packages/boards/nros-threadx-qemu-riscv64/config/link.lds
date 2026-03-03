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

  PROVIDE(_end = .);
}
