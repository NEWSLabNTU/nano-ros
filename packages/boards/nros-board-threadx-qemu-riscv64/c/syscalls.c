/*
 * syscalls.c — Minimal bare-metal stubs for picolibc
 *
 * picolibc's assert/abort/raise functions reference POSIX symbols
 * that don't exist on bare metal. Provide no-op or minimal stubs.
 */

#include <stdint.h>

/* `stderr` (and `stdout`) is defined once, canonically, in the board's
 * `startup.c` — a real picolibc `FILE` routed to UART, linked into every app.
 * Do NOT redefine it here: a second global `stderr` collides at link time once
 * `syscalls.o` is pulled for its other stubs (issue 0084 — `rust-lld: duplicate
 * symbol: stderr`, after phase-251 dropped `--allow-multiple-definition`).
 * picolibc's `__assert_func` writes to startup.c's UART `stderr`. */

/* _sbrk: refuse, loudly — issue 0657.
 *
 * newlib's malloc pulls `_sbrk`; picolibc's does not, so this stub was never
 * needed while the Ubuntu picolibc toolchain was the only one that built this
 * board. The toolchain `nros setup` provisions (xPack `riscv-none-elf`) bundles
 * NEWLIB, and the image then failed to link on this one symbol after every
 * other libc function resolved.
 *
 * It returns failure rather than carving a heap: allocation on this board
 * belongs to the ThreadX byte pool, and `link.lds` gives `.heap` ZERO bytes on
 * purpose (its `PROVIDE(end = .)` exists only so a libnosys stub can resolve).
 * A stray `malloc` therefore returns NULL — which the caller must handle —
 * instead of silently handing out memory that belongs to something else.
 */
void *_sbrk(int incr)
{
    (void)incr;
    return (void *)-1;
}

/* _exit: halt the processor */
void _exit(int status)
{
    (void)status;
    for (;;) {
        __asm__ volatile("wfi");
    }
}

/* getpid / kill: referenced by picolibc's raise() */
int getpid(void) { return 1; }
int kill(int pid, int sig) { (void)pid; (void)sig; return 0; }

/*
 * rand / srand — Non-TLS replacements for picolibc's TLS-based versions.
 *
 * picolibc uses thread-local storage (via the tp register) for rand() state.
 * On bare-metal ThreadX, tp is 0 → any TLS access is a load from NULL → crash.
 * These simple LCG implementations use a global variable instead.
 */
static unsigned int _rand_seed = 1;

void srand(unsigned int seed)
{
    _rand_seed = seed;
}

int rand(void)
{
    _rand_seed = _rand_seed * 1103515245u + 12345u;
    return (int)((_rand_seed >> 16) & 0x7FFF);
}

long random(void)
{
    return (long)rand();
}
