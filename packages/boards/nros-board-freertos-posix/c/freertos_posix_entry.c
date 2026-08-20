/*
 * freertos_posix_entry.c — boot path for the FreeRTOS POSIX simulator.
 *
 * phase-370 W1. The C/C++ lane's `main`: create the application task, start the
 * scheduler, and let the task call `app_main` (which `nros-c`'s NROS_APP_MAIN
 * macro emits). Same responsibility as the family's `freertos_c_entry.c`, and
 * deliberately NOT a copy of it — that file is ~150 lines of ARM semihosting
 * (`SYS_OPEN`, `SYS_WRITE`, `bkpt #0xAB`, `ADP_Stopped_ApplicationExit`) plus
 * lwIP bring-up, none of which exists here:
 *
 *   * stdout is the host's, so `printf` needs no `_write` override and no
 *     semihosting handle. Buffering is still disabled, for the same reason the
 *     family file disables it: a test harness reading the process's stdout must
 *     see output as it happens, not when the image exits.
 *   * there is no network to bring up. The host kernel owns the stack, so no
 *     `nros_freertos_init_network`, no netif wait, and no poll task —
 *     `supported_netstacks = []` in `nros-board.toml` is the same statement one
 *     layer up.
 *   * exit is `exit()`, not a semihosting trap.
 *
 * What IS shared with the family is the part that is about FreeRTOS rather than
 * about the chip: `nros_freertos_create_task` / `nros_freertos_start_scheduler`
 * from `freertos_task_glue.c`, split out of `network_glue.c` by this phase
 * precisely so this board could reach them without lwIP.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h> /* getpid, for the RNG seed fallback */

#include "FreeRTOS.h"
#include "task.h"

#include <nros/app_config.h>

/* ---- Provided elsewhere ---- */
/* nros-board-freertos/c/freertos_task_glue.c */
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

/* ---- nros-log writer ----
 *
 * Registered before `app_main` so records raised during session setup land.
 * Writes to stderr rather than stdout: stdout carries the application's own
 * output, which is what an e2e test greps, and interleaving diagnostics into it
 * makes a delivery assertion depend on log level. */
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
        fprintf(stderr, "[%s] %.*s\n", label, (int)msg_len, (const char *)msg_ptr);
    } else {
        fprintf(stderr, "[%s] %.*s: %.*s\n", label, (int)name_len, (const char *)name_ptr,
                (int)msg_len, (const char *)msg_ptr);
    }
}

static void board_log_flush(void) {
    fflush(stderr);
}

/* NROS_APP_CONFIG's scheduling priorities are RAW FreeRTOS values (issue 0623),
 * converted once by `nros_board_common::freertos_build` when it emits the
 * config TU. This is a bounds guard, kept for the same reason the family file
 * keeps its copy: an out-of-tree config still holding a normalized 0–31 value
 * would otherwise trip `configASSERT(uxPriority < configMAX_PRIORITIES)` inside
 * `xTaskCreate`, which names neither the field nor the file. */
static inline UBaseType_t clamp_prio(uint32_t p) {
    if (p >= (uint32_t)configMAX_PRIORITIES) {
        return (UBaseType_t)(configMAX_PRIORITIES - 1);
    }
    return (UBaseType_t)p;
}

/* Seed the platform PRNG with a value unique to this node.
 *
 * The platform's xorshift PRNG has a fixed default state, so two unseeded
 * processes produce identical sequences. On the QEMU boards that collapses
 * zenoh session IDs; here the concern is narrower but the same shape — two
 * simulator processes on one host must not agree by construction. IP octets
 * are the per-node fact available at this point; a config that leaves them
 * zero falls back to the PID, which a host always has and QEMU does not. */
static void seed_platform_rng(const uint8_t ip[4], const uint8_t mac[6]) {
    uint32_t seed = ((uint32_t)ip[0] << 24) | ((uint32_t)ip[1] << 16) | ((uint32_t)ip[2] << 8) |
                    (uint32_t)ip[3];
    seed ^= ((uint32_t)mac[4] << 8) | (uint32_t)mac[5];
    if (seed == 0) {
        seed = (uint32_t)getpid();
    }
    seed = seed * 2654435761u; /* Knuth multiplicative hash */
    if (seed == 0) {
        seed = 1;
    }
    nros_platform_freertos_seed_rng(seed);
}

static void app_task_entry(void *arg) {
    (void)arg;

    setvbuf(stdout, NULL, _IONBF, 0);
    setvbuf(stderr, NULL, _IONBF, 0);

    seed_platform_rng(NROS_APP_CONFIG.network.ip, NROS_APP_CONFIG.network.mac);
    nros_platform_register_log_writer(board_log_writer, board_log_flush);

    app_main();

    /* The image is a host process, so ending is `exit`, and the status is the
     * one a harness reads. `vTaskDelete(NULL)` would leave the scheduler
     * running with nothing to run — the family file's semihosting trap is the
     * equivalent statement for QEMU. */
    fflush(NULL);
    exit(0);
}

int main(void) {
    /* `xTaskCreate` takes a depth in WORDS. On the POSIX port a word is the
     * host's `StackType_t`, so the byte count from NROS_APP_CONFIG is divided
     * by the real word size rather than by a hardcoded 4 — the family file can
     * assume 4 because Cortex-M is 32-bit, and this board is usually not.
     *
     * A zero or absent `app_stack_bytes` falls back to the config's minimum
     * rather than creating a task with no stack; `stack_bytes` is a FLOOR the
     * port raises, never a size the caller can get right (issue 0667). */
    uint32_t app_stack_words = NROS_APP_CONFIG.scheduling.app_stack_bytes / sizeof(StackType_t);
    if (app_stack_words < (uint32_t)configMINIMAL_STACK_SIZE) {
        app_stack_words = (uint32_t)configMINIMAL_STACK_SIZE;
    }

    if (nros_freertos_create_task(app_task_entry, "app", app_stack_words, NULL,
                                  clamp_prio(NROS_APP_CONFIG.scheduling.app_priority)) != 0) {
        fprintf(stderr, "failed to create the app task\n");
        return 1;
    }

    nros_freertos_start_scheduler();

    /* The POSIX port's `vTaskStartScheduler` does not return. */
    fprintf(stderr, "FreeRTOS scheduler exited unexpectedly\n");
    return 1;
}
