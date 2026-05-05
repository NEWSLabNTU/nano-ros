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
extern int tcu_print_msg(const char *msg_buf, int len, int from_isr);

int vsnprintf(char *str, size_t size, const char *fmt, va_list ap)
{
    return vsniprintf(str, size, fmt, ap);
}

/*
 * `printf` override. The BSP's `platform/debug_init.c` calls `printf`
 * directly (not `printf_isr` via `platform/print.c`), which would
 * otherwise pull newlib's full `vfprintf` chain — `_dtoa_r`,
 * `fmaf128`, `__divtf3`, `__addtf3`, `__multf3`, `lgamma_r`, `frexp` —
 * for float-format support that the BSP never uses (every printf call
 * site formats `%d` / `%s` / `%x` / pointer literals). Forwarding to
 * `vsniprintf` against a stack buffer + `tcu_print_msg` cuts the same
 * ~25 KB BTCM as the `vsnprintf` shim above, but for the non-isr
 * printf path the shim alone doesn't catch.
 */
int printf(const char *fmt, ...)
{
    char buf[256];
    va_list ap;
    va_start(ap, fmt);
    int ret = vsniprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (ret > 0) {
        int len = ret < (int)sizeof(buf) ? ret : (int)sizeof(buf) - 1;
        tcu_print_msg(buf, len, 0);
    }
    return ret;
}
