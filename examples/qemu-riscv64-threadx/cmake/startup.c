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

/* ---- Configuration from CMake (via config.toml) ---- */
#ifndef APP_IP
#define APP_IP {10, 0, 2, 40}
#endif
#ifndef APP_MAC
#define APP_MAC {0x52, 0x54, 0x00, 0x12, 0x34, 0x56}
#endif
#ifndef APP_GATEWAY
#define APP_GATEWAY {10, 0, 2, 2}
#endif
#ifndef APP_NETMASK
#define APP_NETMASK {255, 255, 255, 0}
#endif

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

/* ---- nros-c platform stubs (NROS_PLATFORM_BAREMETAL) ----
 *
 * libnros_c_zenoh_threadx_riscv64.a is built no_std, so its Rust code
 * calls the four `nros_platform_*` symbols via FFI instead of using
 * `std::time::Instant` / `std::thread::sleep`. ThreadX supplies
 * tx_time_get() (ticks since startup, 100 Hz by default — see
 * `TX_TIMER_TICKS_PER_SECOND` in tx_user.h) and tx_thread_sleep(ticks).
 * GCC __atomic_* builtins cover the atomic-bool pair. The C/C++
 * examples link this TU, so these strong definitions satisfy the
 * archive's undefined references at link time. Rust ThreadX RISC-V
 * examples don't pull in nros-c, so they don't need these. */

uint64_t nros_platform_time_ns(void) {
    /* tx_time_get() returns ULONG ticks; one tick = 10ms at 100 Hz. */
    return (uint64_t)tx_time_get() * (1000000000ULL / TX_TIMER_TICKS_PER_SECOND);
}

void nros_platform_sleep_ns(uint64_t ns) {
    uint64_t ticks = ns / (1000000000ULL / TX_TIMER_TICKS_PER_SECOND);
    if (ticks == 0 && ns > 0) ticks = 1;
    tx_thread_sleep((ULONG)ticks);
}

void nros_platform_atomic_store_bool(_Bool *ptr, _Bool value) {
    __atomic_store_n(ptr, value, __ATOMIC_RELEASE);
}

_Bool nros_platform_atomic_load_bool(const _Bool *ptr) {
    return __atomic_load_n(ptr, __ATOMIC_ACQUIRE);
}

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

/* picolibc _write syscall for other output (fprintf to fd, etc.) */
int _write(int fd, const char *buf, int len) {
    (void)fd;
    for (int i = 0; i < len; i++) uart_putc(buf[i]);
    return len;
}

/* ---- FFI: set config and C entry point in app_define.c ---- */
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
    uint8_t ip[]      = APP_IP;
    uint8_t netmask[] = APP_NETMASK;
    uint8_t gateway[] = APP_GATEWAY;
    uint8_t mac[]     = APP_MAC;

    nros_threadx_set_config(ip, netmask, gateway, mac, "");

    /* Register C app_main as the entry point for the ThreadX app thread */
    extern void app_main(void);
    nros_threadx_set_app_main(app_main);

    /* Enter ThreadX scheduler — never returns.
     * tx_application_define() is called from within tx_kernel_enter(). */
    tx_kernel_enter();

    return 0;
}
