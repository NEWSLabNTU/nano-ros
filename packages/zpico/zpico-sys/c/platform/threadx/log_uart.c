/* Phase 120.3 diag: zenoh-pico log sink for bare-metal ThreadX targets.
 *
 * zenoh-pico's `_Z_LOG` family expands to `ZENOH_LOG_PRINT(fmt, ...)`,
 * which defaults to `printf`. Bare-metal builds don't link a stdio
 * implementation, so the default `ZENOH_DEBUG=3` build fails with
 * `undefined symbol: stdout`. Override `ZENOH_LOG_PRINT` to
 * `zpico_log_print` (-D on the build line) and back it with this
 * thin `vsnprintf` + UART wrapper.
 *
 * Only compiled into the ThreadX RV64 cc-build. */

#if defined(ZENOH_THREADX)

#include <stdarg.h>
#include <stdio.h>

extern void uart_putc(unsigned char c);

int zpico_log_print(const char *fmt, ...) {
    char buf[256];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    int len = (n < 0) ? 0 : (n >= (int)sizeof(buf) ? (int)sizeof(buf) - 1 : n);
    for (int i = 0; i < len; i++) {
        uart_putc((unsigned char)buf[i]);
    }
    return n;
}

#endif
