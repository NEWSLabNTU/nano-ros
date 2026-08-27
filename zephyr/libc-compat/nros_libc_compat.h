/*
 * nros — libc gap shim for Zephyr targets, force-included by the module.
 *
 * Zephyr's MINIMAL libc (the default on native_sim for the 3.7 line) ships a
 * `<stdio.h>` with no `setvbuf` and no `_IO*BF` constants at all. Its stdout is
 * a per-character hook (`lib/libc/minimal/source/stdout/stdout_console.c`), so
 * it is ALREADY unbuffered and `setvbuf(stdout, NULL, _IONBF, 0)` — which the
 * examples call so their prints appear promptly — is a no-op there by
 * construction. Without this shim that call is a hard error and every C and C++
 * example fails to build on minimal libc.
 *
 * Keyed on `_IONBF` rather than on a libc Kconfig: picolibc and newlib both
 * define it along with a real `setvbuf`, and on those this header does nothing.
 */

#ifndef NROS_LIBC_COMPAT_H
#define NROS_LIBC_COMPAT_H

#include <stdio.h>

#ifndef _IONBF
#include <stddef.h>

#define _IOFBF 0
#define _IOLBF 1
#define _IONBF 2

#ifdef __cplusplus
extern "C" {
#endif

/* Accepts and ignores, which matches the behaviour: the stream is unbuffered
 * whatever is asked for. Returning 0 is "success" per POSIX. */
static inline int setvbuf(FILE *stream, char *buf, int mode, size_t size) {
    (void)stream;
    (void)buf;
    (void)mode;
    (void)size;
    return 0;
}

#ifdef __cplusplus
}
#endif

#endif /* _IONBF */

#endif /* NROS_LIBC_COMPAT_H */
