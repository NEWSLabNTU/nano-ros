/* The C runtime's init arrays, for a C-rooted image (RFC-0130 D2, phase-553 W2).
 *
 * cortex-m-rt's link.x places no `.preinit_array`/`.init_array`, so C++
 * constructors and `__attribute__((constructor))` functions would land in an
 * orphan section with no bounds, and nothing would run them. A C-rooted image
 * links with THIS file as its script (`-Tc-runtime.x`, not `-Tlink.x`): it
 * includes link.x, then inserts the init arrays after `.rodata` (measured: an
 * INSERT in a second `-T` script, or in a plain input script, finds no
 * `.rodata`; cortex-m-rt honours INSERTs only from a script read inside its
 * own). The board's C start (`nros_board_mps2_an385_start`) walks the two
 * arrays before the application runs. A Rust-rooted image never names this
 * file.
 */
INCLUDE link.x

SECTIONS
{
  .init_array : ALIGN(4)
  {
    __preinit_array_start = .;
    KEEP(*(.preinit_array))
    __preinit_array_end = .;
    __init_array_start = .;
    KEEP(*(SORT(.init_array.*)))
    KEEP(*(.init_array))
    __init_array_end = .;
    . = ALIGN(4);
  } > FLASH
}
INSERT AFTER .rodata;
