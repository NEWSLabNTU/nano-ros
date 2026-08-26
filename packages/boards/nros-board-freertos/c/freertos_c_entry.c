/*
 * freertos_c_entry.c — boot path for the C/C++ application lane.
 *
 * phase-337 W5.b. Until this file existed, the CMake (C/C++) lane compiled a
 * per-board `startup.c` of 727 lines, ~575 of which were a LIVE DUPLICATE of
 * `network_glue.c` + `freertos_hooks.c` + the board's own `board_mps2.c`. The
 * duplicate survived only because the two lanes compiled different files —
 * CMake took `startup.c`, cargo took the others — so the copies could drift
 * without either lane noticing, and they HAD: the shadow copy seeded the
 * platform PRNG where the shared glue seeds `srand()`, and its
 * malloc-failed hook printed heap numbers the shared hook does not.
 *
 * Both lanes now compile ONE set of sources. What is left here is the part
 * that was never duplicated: the C-lane equivalent of the Rust lane's
 * `nros_board_freertos::run_entry` — semihosting stdio for `printf`, the
 * nros-log writer, the app/poll task creation, and `main`.
 *
 * Compiled by the CMake lane only (`FREERTOS_STARTUP_SOURCE`), because the
 * Rust lane's `main` is the firmware's Rust entry point. The board's
 * `Reset_Handler` calls `main()` either way.
 */

#include <stdint.h>
#include <stdio.h>

#include "FreeRTOS.h"
#include "task.h"

#include <nros/app_config.h>

/* ---- Semihosting trap instruction ----
 *
 * The instruction that enters the debugger is INSTRUCTION-SET dependent, and
 * this family is no longer Thumb-only: `bkpt #0xAB` is the T32 encoding, while
 * an A/R-profile board running in ARM state (Cortex-R52 — phase-385) must use
 * `svc #0x123456`. Compiled for ARM state, a `bkpt #0xAB` is not recognised as
 * a semihosting call at all: QEMU takes a real prefetch/data abort and the
 * image ends up spinning in the abort vector with no output — which reads as a
 * dead image rather than a wrong console.
 *
 * `__thumb__` is defined by the compiler for the TU's own instruction set, so
 * this selects correctly per board without a board-specific #define.
 */
#if defined(__thumb__)
#define NROS_SEMIHOST_TRAP "bkpt #0xAB\n"
#else
#define NROS_SEMIHOST_TRAP "svc #0x123456\n"
#endif

/* ---- Provided elsewhere ---- */
/* freertos_hooks.c */
extern void semihosting_write0(const char *s);
extern void nros_board_freertos_run_init_array(void);
/* network_glue.c */
extern int nros_freertos_init_network(const uint8_t mac[6], const uint8_t ip[4],
                                      const uint8_t netmask[4], const uint8_t gw[4]);
extern void nros_freertos_poll_network(void);
extern void nros_freertos_start_scheduler(void);
extern int nros_freertos_create_task(void (*entry)(void *), const char *name,
                                     uint32_t stack_words, void *arg, uint32_t priority);
/* nros-platform-freertos */
extern void nros_platform_freertos_seed_rng(uint32_t value);
extern void nros_platform_register_log_writer(void (*writer)(uint8_t, const uint8_t *, uintptr_t,
                                                             const uint8_t *, uintptr_t),
                                              void (*flusher)(void));
/* the user application (nros-c's NROS_APP_MAIN macro emits it) */
extern void app_main(void);

/* ---- Semihosting stdio ----
 * Newlib's printf calls _write(fd, buf, len). With -nostartfiles, the
 * semihosting file handles for stdin/stdout/stderr aren't opened by crt0.
 * Open them via SYS_OPEN at startup and map fd 0/1/2 to the returned
 * semihosting handles. */
static int sh_stdout_handle = -1;

static int semihosting_open(const char *path, int mode) {
    uint32_t args[3] = {(uint32_t)path, (uint32_t)mode, (uint32_t)__builtin_strlen(path)};
    int result;
    __asm__ volatile("mov r0, #0x01\n" /* SYS_OPEN */
                     "mov r1, %1\n"
                     NROS_SEMIHOST_TRAP
                     "mov %0, r0\n"
                     : "=r"(result)
                     : "r"(args)
                     : "r0", "r1", "memory");
    return result;
}

/* Called from app_task_entry before app_main to initialise semihosting I/O. */
static void semihosting_stdio_init(void) {
    /* Open ":tt" in write mode (mode=4) for stdout */
    sh_stdout_handle = semihosting_open(":tt", 4);
}

/* Provides printf() output on QEMU via ARM semihosting SYS_WRITE (0x05).
 * This overrides the stub in libnosys (which returns -1). */
int _write(int fd, const char *buf, int count) {
    int sh_fd = sh_stdout_handle;
    if (sh_fd < 0) {
        /* Fallback before init: use SYS_WRITE0 (goes to stderr/debug) */
        char tmp[256];
        int rem = count;
        const char *p = buf;
        while (rem > 0) {
            int chunk = rem < (int)(sizeof(tmp) - 1) ? rem : (int)(sizeof(tmp) - 1);
            for (int i = 0; i < chunk; i++) tmp[i] = p[i];
            tmp[chunk] = '\0';
            semihosting_write0(tmp);
            p += chunk;
            rem -= chunk;
        }
        return count;
    }
    (void)fd;
    uint32_t args[3] = {(uint32_t)sh_fd, (uint32_t)buf, (uint32_t)count};
    uint32_t result;
    __asm__ volatile("mov r0, #0x05\n"
                     "mov r1, %1\n"
                     NROS_SEMIHOST_TRAP
                     "mov %0, r0\n"
                     : "=r"(result)
                     : "r"(args)
                     : "r0", "r1", "memory");
    return count - (int)result;
}

/* ---- nros-log writer ----
 * Phase 88.16.H — printf-backed writer registered with the platform fn-ptr
 * slot before app_main. Same shape as the Rust `run()` path's hstderr writer,
 * expressed in C so the C/C++ path that bypasses Rust's `run()` still logs. */
static void board_log_writer(uint8_t severity, const uint8_t *name_ptr, uintptr_t name_len,
                             const uint8_t *msg_ptr, uintptr_t msg_len) {
    const char *label;
    switch (severity) {
        case 0: label = "TRACE"; break;
        case 1: label = "DEBUG"; break;
        case 2: label = "INFO"; break;
        case 3: label = "WARN"; break;
        case 4: label = "ERROR"; break;
        case 5: label = "FATAL"; break;
        default: label = "?"; break;
    }
    if (name_len == 0 || name_ptr == NULL) {
        printf("[%s] %.*s\n", label, (int)msg_len, (const char *)msg_ptr);
    } else {
        printf("[%s] %.*s: %.*s\n", label, (int)name_len, (const char *)name_ptr, (int)msg_len,
               (const char *)msg_ptr);
    }
}

/* zenoh-pico's read/lease task tuning. Weak so a DDS-only image links without
 * the zenoh backend; the real definition comes from `zpico.c`. */
__attribute__((weak, used)) void zpico_set_task_config(uint32_t read_priority,
                                                       uint32_t read_stack_bytes,
                                                       uint32_t lease_priority,
                                                       uint32_t lease_stack_bytes) {
    (void)read_priority;
    (void)read_stack_bytes;
    (void)lease_priority;
    (void)lease_stack_bytes;
}

/* NROS_APP_CONFIG's scheduling priorities are RAW FreeRTOS values as of issue
 * 0623 — `nros_board_common::freertos_build` converts the normalized 0–31 band
 * ONCE, when it emits this TU. This is now only a bounds guard.
 *
 * It used to be the C half of the conversion, and it disagreed with the Rust
 * half: this clamp SATURATES while `to_freertos_priority` scales
 * proportionally, so normalized 16 became 7 here and 4 there. Every default was
 * >= configMAX_PRIORITIES (app 12, zenoh read/lease 16, poll 16), so on this
 * path all four saturated to 7 — one priority for the app, the transport tasks
 * and the net poll, with the ordering they were written to express gone.
 *
 * Kept, rather than deleted, because an out-of-tree board that still writes a
 * normalized value into the struct would otherwise trip
 * `configASSERT(uxPriority < configMAX_PRIORITIES)` inside `xTaskCreate`, which
 * names neither the field nor the file. A clamp that never fires for in-tree
 * configs is cheap; the assert it replaces is not. */
static inline UBaseType_t clamp_prio(uint32_t p) {
    if (p >= (uint32_t)configMAX_PRIORITIES) {
        return (UBaseType_t)(configMAX_PRIORITIES - 1);
    }
    return (UBaseType_t)p;
}

static void poll_task_entry(void *arg) {
    (void)arg;
    const uint32_t poll_ms = NROS_APP_CONFIG.scheduling.poll_interval_ms;
    for (;;) {
        vTaskDelay(pdMS_TO_TICKS(poll_ms));
        nros_freertos_poll_network();
    }
}

/* Seed the platform PRNG with a value unique to this node.
 *
 * zenoh-pico's z_random_* delegate to nros-platform-freertos's xorshift PRNG,
 * whose static state defaults to a fixed value. Unseeded, two QEMU instances
 * produce identical 16-byte zenoh session IDs → zenohd treats them as the same
 * peer (max_links=1) and rejects the second connection. `srand()` (which
 * network_glue.c does, for lwIP's LWIP_RAND) does NOT cover this: the platform
 * PRNG never reads libc's rand state. The Rust lane seeds the same way in
 * `nros_board_freertos::entry::freertos_boot_bringup`.
 *
 * Use IP octets directly — each node has a unique IP. Multiply to spread bits
 * and avoid XOR cancellation between MAC and IP. */
static void seed_platform_rng(const uint8_t ip[4], const uint8_t mac[6]) {
    uint32_t seed = ((uint32_t)ip[0] << 24) | ((uint32_t)ip[1] << 16) | ((uint32_t)ip[2] << 8) |
                    (uint32_t)ip[3];
    seed = seed * 2654435761u; /* Knuth multiplicative hash */
    seed ^= ((uint32_t)mac[4] << 8) | (uint32_t)mac[5];
    if (seed == 0) seed = 1;
    nros_platform_freertos_seed_rng(seed);
}

static void app_task_entry(void *arg) {
    (void)arg;

    /* phase-370 W4 (issue 0733) — run the static constructors FIRST. This flat
     * bare-metal image has no crt0, so nothing else walks `.init_array`, and
     * the Cyclone message-descriptor registration TUs are constructors. Before
     * `app_main`, before any session: a descriptor looked up before this runs
     * is a miss, and the miss surfaces as a bare `-1` from publisher/subscriber
     * create. Same placement as the threadx board's #195 walk. */
    nros_board_freertos_run_init_array();

    seed_platform_rng(NROS_APP_CONFIG.network.ip, NROS_APP_CONFIG.network.mac);

    if (nros_freertos_init_network(NROS_APP_CONFIG.network.mac, NROS_APP_CONFIG.network.ip,
                                   NROS_APP_CONFIG.network.netmask,
                                   NROS_APP_CONFIG.network.gateway) != 0) {
        semihosting_write0("Network init failed\n");
        for (;;) {}
    }

    /* Wait for tcpip_thread to run and netif to come up */
    vTaskDelay(pdMS_TO_TICKS(2000));

    semihosting_write0("Network ready\n");

    /* Create poll task. 256 words = 1 KB stack is enough for the
     * `nros_freertos_poll_network` busy loop. */
    nros_freertos_create_task(poll_task_entry, "poll", 256, 0,
                              clamp_prio(NROS_APP_CONFIG.scheduling.poll_priority));

    /* Initialise semihosting stdio so printf() routes to QEMU stdout.
     * Disable buffering so output is visible immediately (important for
     * test harnesses that capture stdout from QEMU processes). */
    semihosting_stdio_init();
    setvbuf(stdout, NULL, _IONBF, 0);

    nros_platform_register_log_writer(board_log_writer, NULL);

    /* Configure zenoh-pico read+lease task priorities BEFORE app_main (which
     * calls zpico_init -> zp_start_read_task). Without this, the read task
     * spawns at priority 0 (idle) and never delivers subscription messages on
     * a system with higher-priority tasks. */
    zpico_set_task_config(clamp_prio(NROS_APP_CONFIG.scheduling.zenoh_read_priority),
                          NROS_APP_CONFIG.scheduling.zenoh_read_stack_bytes,
                          clamp_prio(NROS_APP_CONFIG.scheduling.zenoh_lease_priority),
                          NROS_APP_CONFIG.scheduling.zenoh_lease_stack_bytes);

    /* Run user application */
    app_main();

    /* Semihosting exit */
    {
        uint32_t exit_args[2] = {0x20026, 0}; /* ADP_Stopped_ApplicationExit */
        __asm__ volatile("mov r0, #0x18\nmov r1, %0\n" NROS_SEMIHOST_TRAP
                         :
                         : "r"(exit_args)
                         : "r0", "r1", "memory");
    }
    for (;;) {}
}

/* The board's `Reset_Handler` jumps here (the Rust lane's `main` is the Rust
 * entry point instead — same symbol, one board file). */
int main(void) {
    /* App stack is in WORDS (4 bytes on Cortex-M). The byte count comes from
     * NROS_APP_CONFIG, tuned per use case. */
    const uint32_t app_stack_words = NROS_APP_CONFIG.scheduling.app_stack_bytes / 4;
    nros_freertos_create_task(app_task_entry, "app", app_stack_words, 0,
                              clamp_prio(NROS_APP_CONFIG.scheduling.app_priority));
    nros_freertos_start_scheduler();
    for (;;) {}
}
