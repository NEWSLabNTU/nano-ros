/*
 * BTCM-frugal vsnprintf override.
 *
 * NVIDIA's BSP `platform/print.c` calls `vsnprintf` to format printf
 * messages before pushing them to the TCU. The default `vsnprintf` in
 * the toolchain's newlib pulls `_svfprintf_r`, which references
 * `_dtoa_r` (long-double conversion) and the 128-bit float intrinsics
 * (`fmaf128`, `__divtf3`, `__addtf3`, `__multf3`, `lgamma_r`, ...) for
 * `%f` / `%e` / `%g` support. Together they cost ~25 KB BTCM on the
 * Cortex-R5F's 256 KB budget — even when no caller ever prints a float.
 *
 * `vsniprintf` is newlib's integer-only formatter (no float
 * conversions). Forwarding `vsnprintf` to it drops the entire dtoa /
 * f128 chain. Any BSP code that prints floats degrades to printing
 * literal `%f` characters — autoware-sentinel's BSP only formats
 * pointers / ints / strings (`printf("foo: %d\r\n", x)`), so this is
 * a safe trade.
 *
 * Cargo `staticlib` archives are placed before newlib on the link line,
 * so the linker prefers this `vsnprintf` over the float-aware one.
 */

#include <stdarg.h>
#include <stddef.h>

extern int vsniprintf(char *str, size_t size, const char *fmt, va_list ap);

int vsnprintf(char *str, size_t size, const char *fmt, va_list ap)
{
    return vsniprintf(str, size, fmt, ap);
}
