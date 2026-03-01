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
