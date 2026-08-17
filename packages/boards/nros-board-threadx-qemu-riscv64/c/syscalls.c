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

/* _sbrk: carve the linker's heap region — issues 0657, 0664.
 *
 * It began as a REFUSAL (return -1). That was right for what was known then:
 * newlib's malloc pulls `_sbrk`, picolibc's does not, and allocation on this
 * board belongs to the ThreadX byte pool, so a libc heap looked like a way to
 * hand out memory that belongs to something else.
 *
 * It was wrong, and the way it was wrong is worth keeping: `malloc` here has a
 * caller the byte pool cannot serve. CycloneDDS's `thread_states_init` reaches
 * libgcc's EMULATED TLS, and `__emutls_get_address` calls plain `malloc` and
 * `abort()`s if it returns NULL. Refusing therefore turned every Cyclone image
 * into an abort inside `dds_create_domain`, before a single line of output —
 * which read as a hang and cost issue 0664 to diagnose (the backtrace, once
 * taken, named it in six frames).
 *
 * So: a real bump allocator over the `.heap` region `link.lds` now reserves.
 * No free — `_sbrk` has no shape for it, emutls never frees, and a bump
 * pointer cannot fragment. Out of memory returns `(void *)-1`, which is what
 * newlib expects and what makes the caller's failure ITS decision.
 */
extern char __heap_start[];
extern char __heap_end[];

void *_sbrk(int incr)
{
    static char *brk = 0;
    if (brk == 0) {
        brk = __heap_start;
    }
    if (incr < 0) {
        /* newlib only ever grows through this stub; a shrink would need the
         * bookkeeping a bump allocator deliberately does not have. */
        return (void *)-1;
    }
    if (brk + incr > __heap_end) {
        return (void *)-1;
    }
    char *prev = brk;
    brk += incr;
    return prev;
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
