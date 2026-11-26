/* phase-578 W2 -- the ELF `fault_lines`' unit tests look addresses up in.
 *
 * Committed with its build output, `fixture.elf`, so the tests need no
 * cross toolchain and do not depend on the host build's debug info (CI
 * builds the test binary with `debug = 0`). Rebuild from this directory
 * with the SDK store's arm-none-eabi-gcc 13.2:
 *
 *   arm-none-eabi-gcc -mcpu=cortex-m3 -mthumb -g -O1 -nostdlib \
 *       -ffreestanding -fdebug-prefix-map="$PWD"=. -Wl,-e,_start \
 *       -o fixture.elf fixture.c
 *
 * The tests read the line numbers below from this file, by the marker
 * comments, so an edit that moves them stays consistent. */

__attribute__((noinline)) void fixture_inner(volatile int *p) {
    *p = 1; /* fault-lines: inner store */
}

static inline __attribute__((always_inline)) void fixture_middle(volatile int *p) {
    fixture_inner(p); /* fault-lines: inlined call */
}

void _start(void) {
    volatile int x = 0;
    fixture_middle(&x);
    for (;;) {
    }
}
