/*
 * syscalls.c — Minimal bare-metal stubs for picolibc
 *
 * picolibc's assert/abort/raise functions reference POSIX symbols
 * that don't exist on bare metal. Provide no-op or minimal stubs.
 */

#include <stdint.h>

/* Stub FILE for stderr — picolibc's __assert_func writes to stderr */
struct __sFILE {
    int _unused;
};
static struct __sFILE _stderr_file;
struct __sFILE *const stderr = &_stderr_file;

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
