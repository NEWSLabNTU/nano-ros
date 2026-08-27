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

/*
 * issue 0845 — INERT when the libc itself is being compiled.
 *
 * `zephyr_compile_options()` reaches EVERY target, which includes the picolibc
 * MODULE's own sources, not just application code. Force-including this header
 * there is not the no-op the comment above assumes: the `#include <stdio.h>`
 * below runs BEFORE picolibc's own TU sets its feature-test macros, so GNU
 * extensions never get declared and `newlib/libc/ssp/mempcpy_chk.c` dies with
 *
 *     error: implicit declaration of function `mempcpy'
 *
 * under picolibc's `-Werror=implicit-function-declaration`. Verified by running
 * the build's own command line with and without this `-include`: without it the
 * TU compiles clean (rc=0), with it that error appears.
 *
 * `_LIBC` is picolibc's own marker for "I am building the C library" (it is on
 * that command line as `-D_LIBC`), so it is the exact discriminator. The 12
 * callers this shim exists for are all application sources under the zephyr
 * examples' `src` directories; none of them is the libc.
 *
 * This was masked until now by a SECOND bug in the same feature: the `-include`
 * flag was being dropped by CMake de-duplication (see zephyr/CMakeLists.txt),
 * so the header never reached picolibc at all. Fixing that exposed this.
 */
#ifdef _LIBC
#define NROS_LIBC_COMPAT_SKIPPED 1
#else

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

#endif /* _LIBC */

#endif /* NROS_LIBC_COMPAT_H */
