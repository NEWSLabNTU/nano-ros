/*
 * startup.c — ThreadX RISC-V QEMU virt entry point for C/C++ examples
 *
 * Provides the C-level setup that calls nros_threadx_set_config() with
 * compile-time APP_* macros, then enters the ThreadX kernel.
 * tx_kernel_enter() calls tx_application_define() from app_define.c.
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "tx_api.h"
#include <nros/app_config.h>

/* ---- Override memset/memcpy from compiler_builtins ---- */
/* Rust's compiler_builtins provides weak memset/memcpy that can crash on
 * RISC-V due to TLS issues. Provide simple byte-loop implementations. */
void *memset(void *s, int c, __SIZE_TYPE__ n) {
    unsigned char *p = (unsigned char *)s;
    while (n--) *p++ = (unsigned char)c;
    return s;
}
void *memcpy(void *d, const void *s, __SIZE_TYPE__ n) {
    unsigned char *dp = (unsigned char *)d;
    const unsigned char *sp = (const unsigned char *)s;
    while (n--) *dp++ = *sp++;
    return d;
}
void *memmove(void *d, const void *s, __SIZE_TYPE__ n) {
    unsigned char *dp = (unsigned char *)d;
    const unsigned char *sp = (const unsigned char *)s;
    if (dp < sp) { while (n--) *dp++ = *sp++; }
    else { dp += n; sp += n; while (n--) *--dp = *--sp; }
    return d;
}

/* phase-243 — the legacy nros-c platform stubs (time_ns/sleep_ns + atomic-bool)
 * this TU carried are retired. nros-c's no_std path now uses the canonical ABI:
 * nros_platform_clock_us()/sleep_us() (provided by the linked nros-platform-threadx
 * port) + core::sync::atomic. No example references the old symbols anymore. */

/* ---- UART output for printf ---- */
extern int uart_putc(int ch);

/* picolibc stdio: provide stdout as a UART stream.
 * picolibc declares stdout as an undefined extern — we define it here. */
static int _uart_put(char c, FILE *f) { (void)f; uart_putc((int)c); return 0; }
static FILE _uart_file = FDEV_SETUP_STREAM(_uart_put, NULL, NULL, _FDEV_SETUP_WRITE);
/* picolibc declares `extern FILE *const stdout` but leaves it undefined.
 * We provide the definition. The 'const' qualifier is on the pointer,
 * not the FILE — so the FILE itself is mutable. */
FILE *const stdout = &_uart_file;
FILE *const stderr = &_uart_file;

/* picolibc _write syscall for other output (fprintf to fd, etc.) */
int _write(int fd, const char *buf, int len) {
    (void)fd;
    for (int i = 0; i < len; i++) uart_putc(buf[i]);
    return len;
}

/* ---- FFI: set config and C entry point in app_define.c ----
 *
 * Phase 214.A.1 — `nros_threadx_set_config` is `void` by design.
 * The impl is a pure memcpy of the IP/MAC/netmask/gateway bytes into
 * a static cache (see `c/board_threadx_qemu_riscv64.c`); it has no
 * failure mode. The call below therefore returns no error value to
 * capture. */
extern void nros_threadx_set_app_main(void (*entry)(void));
extern void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name);

/*
 * nros_threadx_startup() — called from the board crate's Rust main or
 * from the C entry path. Sets network config and enters ThreadX.
 *
 * On RISC-V QEMU virt, there is no separate main() — entry.s jumps
 * directly into the ThreadX low-level init which calls
 * tx_application_define(). The startup config is set via a global
 * constructor or by being linked before tx_kernel_enter.
 */

/* UART init — must be called before any printf */
extern int uart_init(void);

/* picolibc TLS initialization — must be called before any picolibc function.
 * picolibc uses TLS (via the tp register) for errno, rand state, etc.
 * Our entry.s leaves tp=0 (no picolibc crt0), causing null-pointer access
 * when any TLS variable (errno, etc.) is accessed.
 * Provide a zero-initialized TLS block and point tp to it. */
static char tls_block[512] __attribute__((aligned(16)));

static void init_tls(void) {
    __asm__ volatile("mv tp, %0" : : "r"(tls_block));
}

/* entry.s calls main() after BSS init and stack setup.
 * We init TLS, UART, set network config, then enter the ThreadX kernel. */
int main(void) {
    init_tls();
    uart_init();
    /* Direct UART test before tx_kernel_enter */
    {
        const char *m = "startup: entering ThreadX\n";
        for (int i = 0; m[i]; i++) uart_putc(m[i]);
    }
    nros_threadx_set_config(
        NROS_APP_CONFIG.network.ip,
        NROS_APP_CONFIG.network.netmask,
        NROS_APP_CONFIG.network.gateway,
        NROS_APP_CONFIG.network.mac,
        "");

    /* Register C app_main as the entry point for the ThreadX app thread */
    extern void app_main(void);
    nros_threadx_set_app_main(app_main);

    /* Enter ThreadX scheduler — never returns.
     * tx_application_define() is called from within tx_kernel_enter(). */
    tx_kernel_enter();

    return 0;
}
