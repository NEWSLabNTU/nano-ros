/*
 * freertos_run_tiers.c — Phase 274.W3: C++ embedded multi-tier entry for FreeRTOS.
 *
 * Implements `nros_board_freertos_run_tiers`, the FreeRTOS analog of the native
 * `nros_board_native_run_tiers`: opens ONE RMW session on the calling task (the
 * startup.c `app_task`), spawns one FreeRTOS task per non-boot tier (each with a
 * borrowed executor sharing the session), then runs the boot tier on the caller.
 *
 * Called from `FreertosBoard::run_tiers` (main.hpp) via the generated
 * `nros_app_main` (RFC-0015 Model 1 embedded, RFC-0043 typed path).
 *
 * Phase 274.W3 / RFC-0015 §5.
 */

#include <stdint.h>
#include <stddef.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"

/* --- nros-cpp C FFI forward declarations ---
 *
 * These symbols are defined in nros-cpp (Rust) and linked into the final binary.
 * No need to include nros_cpp_ffi.h here — the linker resolves by name. Signatures
 * must match nros_cpp_ffi.h exactly. */

extern int nros_cpp_init(const char* locator, uint8_t domain_id, const char* node_name,
                         const char* namespace_, void* storage);

extern int nros_cpp_fini(void* storage);

extern void* nros_cpp_executor_session_handle(void* executor);

extern int nros_cpp_executor_open_over_session(void* session_handle, const char* node_name,
                                               uint32_t domain_id, void* out_storage);

extern int nros_cpp_executor_set_active_groups(void* executor, const char* const* groups, size_t n);

extern int nros_cpp_spin_once(void* handle, int32_t timeout_ms);

/* nros_board_network_wait: weak no-op in main.hpp; strong override on boards that
 * need an extra poll-wait after network bring-up. On MPS2-AN385 the startup.c
 * already waits 2 s and polls lwIP, so this is usually a no-op. */
extern void nros_board_network_wait(void);

/* RFC-0034 — the sole sanctioned allocation seam (wraps the FreeRTOS heap); a
 * single counter sees all traffic. Direct pvPortMalloc/vPortFree are forbidden
 * (check-no-direct-kernel-alloc). */
extern void* nros_platform_alloc(size_t size);
extern void nros_platform_dealloc(void* ptr);

/* --- Executor storage sizing ---
 *
 * nros_cpp_init / nros_cpp_executor_open_over_session both need storage of
 * CPP_EXECUTOR_OPAQUE_U64S * 8 bytes, 8-byte aligned. The cmake build generates
 * nros_cpp_config_generated.h with the exact value; since this file is compiled
 * by build.rs (before cmake runs), we use the NuttX/FreeRTOS ARM fallback from
 * nros_cpp_config_generated_nuttx.h (79304 bytes), rounded up to 80 KiB for
 * headroom. nros_platform_alloc on FreeRTOS heap_4 returns 8-byte aligned memory. */
#define NROS_FREERTOS_EXECUTOR_STORAGE_BYTES 81920u

/* --- Local tier-spec type ---
 *
 * Mirror of nros_native_tier_spec_t (nros/main.h). Layout MUST match: same
 * field order, same types, same ABI (verified by the C++ caller casting
 * NativeTierSpec* → nros_native_tier_spec_t* → this type). On 32-bit ARM:
 *   offset 0:  name (ptr, 4 B)
 *   offset 4:  groups (ptr, 4 B)
 *   offset 8:  n_groups (size_t, 4 B)
 *   offset 12: [pad 4 B]
 *   offset 16: priority (int64_t, 8 B)
 *   offset 24: stack_bytes (size_t, 4 B)
 *   offset 28: [pad 4 B]
 *   offset 32: spin_period_us (uint64_t, 8 B)
 *   offset 40: setup (fn ptr, 4 B)
 *   total: 44 B (struct-padded to 48 B) */
typedef int32_t (*nros_tier_setup_fn_t)(void* executor);

typedef struct {
    const char* name;
    const char* const* groups;
    size_t n_groups;
    int64_t priority;
    size_t stack_bytes;
    uint64_t spin_period_us;
    nros_tier_setup_fn_t setup;
} nros_tier_spec_t;

/* --- Per-tier task context ---
 *
 * Heap-allocated by the boot task before xTaskCreate; lives for the firmware
 * lifetime (the spawned task never returns). executor_storage is a separate
 * heap-allocated block passed to nros_cpp_executor_open_over_session. */
typedef struct {
    void* session_handle;
    uint32_t domain_id;
    const char* const* groups;
    size_t n_groups;
    uint64_t spin_period_us;
    nros_tier_setup_fn_t setup;
    void* executor_storage;
} nros_freertos_tier_ctx_t;

/* Minimum spin delay: 1 ms (FreeRTOS tick resolution on MPS2-AN385). */
#define SPIN_PERIOD_FLOOR_MS 1u

/* freertos_tier_task — body of each non-boot tier task.
 *
 * Opens a borrowed executor over the shared session, gates it to the tier's
 * callback groups, calls the tier's setup function, then spins forever at
 * the tier's declared period. On failure: idles (never deletes itself — the
 * boot task is the session owner and must outlive all borrowed executors). */
static void freertos_tier_task(void* arg) {
    nros_freertos_tier_ctx_t* ctx = (nros_freertos_tier_ctx_t*)arg;

    /* Open a borrowed executor that shares the primary session. The primary
     * executor (boot task) must outlive this task — the startup sequence
     * enforces this (the boot task spins forever). */
    int rc = nros_cpp_executor_open_over_session(ctx->session_handle, "tier_node", ctx->domain_id,
                                                 ctx->executor_storage);
    if (rc != 0) {
        /* Cannot open the borrowed executor; idle forever (boot task continues). */
        for (;;) {
            vTaskDelay(pdMS_TO_TICKS(1000u));
        }
    }

    /* Gate this executor to its tier's callback groups. */
    if (ctx->n_groups > 0 && ctx->groups != NULL) {
        nros_cpp_executor_set_active_groups(ctx->executor_storage, ctx->groups, ctx->n_groups);
    }

    /* Run the tier's node-setup function. */
    if (ctx->setup != NULL) {
        rc = ctx->setup(ctx->executor_storage);
        if (rc != 0) {
            /* Setup failed; close the borrowed executor and idle. */
            nros_cpp_fini(ctx->executor_storage);
            for (;;) {
                vTaskDelay(pdMS_TO_TICKS(1000u));
            }
        }
    }

    /* Spin loop. Pass the tier period as the spin_once timeout — a BLOCKING
     * read (issue #126 defect B): timeout 0 returns immediately and never drives
     * the zenoh-pico session's TX/handshake from the spin path, so the shared
     * session never connects. `run_components` (`component_spin_loop`) and the
     * Rust `run_tiers_entry` both spin with a real timeout; mirror that. */
    uint32_t period_ms = (uint32_t)(ctx->spin_period_us / 1000u);
    if (period_ms < SPIN_PERIOD_FLOOR_MS) {
        period_ms = SPIN_PERIOD_FLOOR_MS;
    }
    for (;;) {
        nros_cpp_spin_once(ctx->executor_storage, (int32_t)period_ms);
        vTaskDelay(1);
    }
}

/* nros_board_freertos_run_tiers — Phase 274.W3 (RFC-0015 Model 1 embedded).
 *
 * Called from FreertosBoard::run_tiers (main.hpp) which is called from the
 * generated nros_app_main. By this point: the FreeRTOS kernel is running, the
 * network is up (startup.c app_task_entry brought up LAN9118 + lwIP + zenoh
 * read task), and we are executing inside the app task.
 *
 * The `tiers` array is laid out identically to nros_native_tier_spec_t (the
 * caller casts NativeTierSpec* → nros_native_tier_spec_t* → nros_tier_spec_t*;
 * all three structs have the same ABI on 32-bit ARM).
 *
 * `locator`      — zenoh connect endpoint (baked by cmake, e.g. tcp/192.0.3.1:PORT)
 * `domain_id`    — ROS domain ID (compile-time NROS_ENTRY_DOMAIN_ID)
 * `session_name` — primary session / node name; NULL or empty → "node"
 * `tiers`        — tier-spec array, highest-priority-first (codegen order)
 * `n_tiers`      — number of tiers (>= 1)
 *
 * Returns: this function normally never returns (the boot tier spins forever).
 * Returns a non-zero error code only if the primary session open or a task
 * creation fails. */
int32_t nros_board_freertos_run_tiers(const char* locator, uint8_t domain_id,
                                      const char* session_name, const nros_tier_spec_t* tiers,
                                      size_t n_tiers) {
    if (tiers == NULL || n_tiers == 0) {
        return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
    }

    /* Belt-and-suspenders network wait (startup.c already brought the network
     * up; this calls the weak no-op on MPS2-AN385 or a board's strong override). */
    nros_board_network_wait();

    /* --- Open the primary (owning) executor on the boot task --- */
    const char* sn = (session_name != NULL && session_name[0] != '\0') ? session_name : "node";

    /* Allocate executor storage from the FreeRTOS heap (8-byte aligned on heap_4). */
    void* boot_storage = nros_platform_alloc(NROS_FREERTOS_EXECUTOR_STORAGE_BYTES);
    if (boot_storage == NULL) {
        return -1; /* NROS_CPP_RET_ERROR */
    }
    memset(boot_storage, 0, NROS_FREERTOS_EXECUTOR_STORAGE_BYTES);

    int rc = nros_cpp_init(locator, domain_id, sn, NULL, boot_storage);
    if (rc != 0) {
        nros_platform_dealloc(boot_storage);
        return (int32_t)rc;
    }

    /* Retrieve the session handle so non-boot tiers can borrow it. The handle
     * remains valid as long as boot_storage lives (it lives forever — the boot
     * spin loop never returns). */
    void* session_handle = nros_cpp_executor_session_handle(boot_storage);

    /* --- Spawn non-boot tier tasks (tiers[1..n_tiers)) --- */
    size_t ti;
    for (ti = 1; ti < n_tiers; ti++) {
        const nros_tier_spec_t* t = &tiers[ti];

        /* Allocate executor storage for this tier. */
        void* tier_exec = nros_platform_alloc(NROS_FREERTOS_EXECUTOR_STORAGE_BYTES);
        if (tier_exec == NULL) {
            /* Heap exhausted; shut down the primary executor and return error.
             * Already-spawned tier tasks will idle after open_over_session fails. */
            nros_cpp_fini(boot_storage);
            nros_platform_dealloc(boot_storage);
            return -1; /* NROS_CPP_RET_ERROR */
        }
        memset(tier_exec, 0, NROS_FREERTOS_EXECUTOR_STORAGE_BYTES);

        /* Allocate the tier task context (lives for firmware lifetime). */
        nros_freertos_tier_ctx_t* ctx =
            (nros_freertos_tier_ctx_t*)nros_platform_alloc(sizeof(nros_freertos_tier_ctx_t));
        if (ctx == NULL) {
            nros_platform_dealloc(tier_exec);
            nros_cpp_fini(boot_storage);
            nros_platform_dealloc(boot_storage);
            return -1;
        }

        ctx->session_handle = session_handle;
        ctx->domain_id = (uint32_t)domain_id;
        ctx->groups = t->groups;
        ctx->n_groups = t->n_groups;
        ctx->spin_period_us = t->spin_period_us;
        ctx->setup = t->setup;
        ctx->executor_storage = tier_exec;

        /* Stack size: use the tier spec's stack_bytes if set; else 256 KiB
         * (issue #126 defect A — VERIFIED). A spawned tier task opens a borrowed
         * executor and runs its spin/dispatch; that overflows 64 KiB (HardFault:
         * Prefetch Abort at tskSTACK_FILL_BYTE right after a context switch). At
         * 256 KiB the firmware runs the full run_tiers path (boot init → tier
         * spawn → boot spin) with no fault under QEMU mps2-an385. The boot tier
         * keeps the 512 KiB app_task stack.
         * NOTE: `[tiers.*.freertos].stack_bytes` does NOT yet propagate through
         * emit_cpp into NativeTierSpec (t->stack_bytes is 0 today), so this
         * default is the live value; the config-driven path needs an emitter fix. */
        uint32_t stack_words =
            (t->stack_bytes > 0u) ? (uint32_t)(t->stack_bytes / 4u) : (262144u / 4u);

        /* Raw FreeRTOS priority from the tier spec (the system.toml [tiers.*.freertos]
         * priority is the numeric FreeRTOS value; clamp to configMAX_PRIORITIES-1). */
        UBaseType_t prio = (t->priority > 0)
                               ? (UBaseType_t)(t->priority < (int64_t)(configMAX_PRIORITIES)
                                                   ? t->priority
                                                   : (int64_t)(configMAX_PRIORITIES - 1u))
                               : (UBaseType_t)1u;

        BaseType_t ret = xTaskCreate(freertos_tier_task, (t->name != NULL) ? t->name : "nros_tier",
                                     stack_words, ctx, prio, NULL);
        if (ret != pdPASS) {
            nros_platform_dealloc(ctx);
            nros_platform_dealloc(tier_exec);
            nros_cpp_fini(boot_storage);
            nros_platform_dealloc(boot_storage);
            return -1;
        }
    }

    /* --- Run boot tier (tiers[0]) on THIS task --- */
    const nros_tier_spec_t* boot = &tiers[0];

    /* Gate the boot executor to its tier's callback groups. */
    if (boot->n_groups > 0 && boot->groups != NULL) {
        nros_cpp_executor_set_active_groups(boot_storage, boot->groups, boot->n_groups);
    }

    /* Boot tier node setup. */
    if (boot->setup != NULL) {
        rc = boot->setup(boot_storage);
        if (rc != 0) {
            nros_cpp_fini(boot_storage);
            nros_platform_dealloc(boot_storage);
            return (int32_t)rc;
        }
    }

    /* Boot tier spin loop — runs forever on embedded firmware. Blocking-read
     * spin_once (period as timeout) so the boot session's zenoh handshake is
     * driven from the spin path (issue #126 defect B); timeout 0 did not
     * connect. Mirrors run_components / the Rust run_tiers_entry. */
    uint32_t period_ms = (uint32_t)(boot->spin_period_us / 1000u);
    if (period_ms < SPIN_PERIOD_FLOOR_MS) {
        period_ms = SPIN_PERIOD_FLOOR_MS;
    }
    for (;;) {
        nros_cpp_spin_once(boot_storage, (int32_t)period_ms);
        vTaskDelay(1);
    }

    /* Unreachable — satisfies the compiler. */
    nros_cpp_fini(boot_storage);
    nros_platform_dealloc(boot_storage);
    return 0;
}
