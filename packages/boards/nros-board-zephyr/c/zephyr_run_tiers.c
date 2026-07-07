/*
 * zephyr_run_tiers.c — phase-281 W3a: C++ embedded multi-tier entry for Zephyr.
 *
 * Implements `nros_board_zephyr_run_tiers`, the Zephyr analog of the FreeRTOS
 * `nros_board_freertos_run_tiers` (and the native `nros_board_native_run_tiers`):
 * opens ONE RMW session on the calling thread (the Zephyr `main()` thread — an
 * already-running post-init thread), runs the boot tier's setup, then
 * CHAIN-spawns the remaining tiers (each with a borrowed executor sharing the
 * one session) so their setups serialize (issue #144), and runs the boot tier
 * on the caller.
 *
 * Zephyr deltas vs the FreeRTOS mirror:
 *   - non-boot tiers spawn via the `nros_zephyr_tier_task_create` k_thread shim
 *     (`zephyr/nros_platform_zephyr_shims.c`), NOT xTaskCreate; the shim owns a
 *     static tier stack pool + RAW Zephyr priorities;
 *   - the boot tier adopts its declared RAW priority on the caller thread via
 *     `nros_zephyr_set_current_priority` (the spawned tiers get theirs at
 *     `k_thread_create`);
 *   - Zephyr owns boot + networking (`CONFIG_NET_CONFIG_AUTO_INIT`), so there is
 *     no lwIP bring-up here — just the weak `nros_board_network_wait()` gate
 *     (mirrors `ZephyrBoard::run_components` in main.hpp), not the FreeRTOS
 *     network path.
 *
 * Called from `ZephyrBoard::run_tiers` (main.hpp) via the generated `int
 * main(void)` (the Zephyr kernel calls `main` directly — no `nros_app_main`).
 *
 * phase-281 W3a / RFC-0015 §5.
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* --- Zephyr k_thread tier shim (zephyr/nros_platform_zephyr_shims.c) ---
 *
 * `nros_zephyr_tier_task_create`: spawn one tier task on a static pool thread
 * at the RAW Zephyr priority (cooperative if negative). Returns 0 on success,
 * -1 when the pool (NROS_ZEPHYR_MAX_TIERS) is exhausted. The `entry(arg)`
 * signature is `void* (*)(void*)`.
 *
 * `nros_zephyr_set_current_priority`: adopt a RAW Zephyr priority on the CALLING
 * thread (the boot thread runs tiers[0] itself).
 *
 * `nros_zephyr_msleep`: real-symbol wrapper around `k_msleep` for the idle /
 * park loops (the k_msleep inline has no exported symbol). */
extern int nros_zephyr_tier_task_create(void* (*entry)(void*), void* arg, int32_t priority,
                                        const char* name);
extern void nros_zephyr_set_current_priority(int32_t priority);
extern int32_t nros_zephyr_msleep(int32_t ms);

/* --- nros-cpp C FFI forward declarations ---
 *
 * These symbols are defined in nros-cpp (Rust) and linked into the final binary.
 * No need to include nros_cpp_ffi.h here — the linker resolves by name.
 * Signatures must match nros_cpp_ffi.h exactly. */

extern int nros_cpp_init(const char* locator, uint8_t domain_id, const char* node_name,
                         const char* namespace_, void* storage);

extern int nros_cpp_fini(void* storage);

extern void* nros_cpp_executor_session_handle(void* executor);

extern int nros_cpp_executor_open_over_session(void* session_handle, const char* node_name,
                                               uint32_t domain_id, void* out_storage);

extern int nros_cpp_executor_set_active_groups(void* executor, const char* const* groups, size_t n);

extern int nros_cpp_spin_once(void* handle, int32_t timeout_ms);

/* nros_board_network_wait: weak no-op in main.hpp; strong override on boards
 * that must block for DHCP / link-up. On the canonical Zephyr path
 * (CONFIG_NET_CONFIG_AUTO_INIT) this is a no-op — the kernel brings up
 * networking before main() runs. */
extern void nros_board_network_wait(void);

/* RFC-0034 — the sole sanctioned allocation seam (wraps the Zephyr heap); a
 * single counter sees all traffic. Direct k_malloc/k_free are forbidden. */
extern void* nros_platform_alloc(size_t size);
extern void nros_platform_dealloc(void* ptr);

/* --- Executor storage sizing ---
 *
 * nros_cpp_init / nros_cpp_executor_open_over_session both need storage of
 * CPP_EXECUTOR_OPAQUE_U64S * 8 bytes, 8-byte aligned. The exact per-build value
 * is in the cmake-generated nros_cpp_config_generated.h; to keep this seam
 * standalone-compilable (mirroring freertos_run_tiers.c, which cannot include
 * the generated header) we use the NuttX/embedded ARM fallback (79304 bytes,
 * nros_cpp_config_generated_nuttx.h) rounded up to 80 KiB for headroom.
 * nros_platform_alloc on the Zephyr heap returns 8-byte aligned memory. */
#define NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES 81920u

/* --- Local tier-spec type ---
 *
 * Mirror of nros_native_tier_spec_t (nros/main.h). Layout MUST match: same
 * field order, same types, same ABI (verified by the C++ caller casting
 * NativeTierSpec* → nros_native_tier_spec_t* → this type). */
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
 * Heap-allocated by the spawning thread before nros_zephyr_tier_task_create;
 * lives for the firmware lifetime (the spawned task never returns).
 * executor_storage is a separate heap block passed to
 * nros_cpp_executor_open_over_session. */
typedef struct {
    void* session_handle;
    uint32_t domain_id;
    const char* const* groups;
    size_t n_groups;
    uint64_t spin_period_us;
    nros_tier_setup_fn_t setup;
    void* executor_storage;
    /* issue #144 — chained spawn tail: the tiers still to bring up AFTER this
     * one. This task spawns rest[0] (carrying rest[1..]) only after its own
     * setup returns, so no two setups overlap on the shared session. */
    const nros_tier_spec_t* rest;
    size_t n_rest;
} nros_zephyr_tier_ctx_t;

/* Forward decl — zephyr_tier_task and zephyr_spawn_next_tier are mutually
 * recursive (each tier's task spawns the next tier via this helper). */
static int zephyr_spawn_next_tier(void* session_handle, uint8_t domain_id,
                                  const nros_tier_spec_t* remaining, size_t n_remaining);

/* Minimum spin delay: 1 ms. */
#define SPIN_PERIOD_FLOOR_MS 1u

/* zephyr_tier_task — body of each non-boot tier task.
 *
 * Opens a borrowed executor over the shared session, gates it to the tier's
 * callback groups, calls the tier's setup function, spawns the next tier
 * (issue #144), then spins forever at the tier's declared period. On failure:
 * parks (never returns — the boot thread is the session owner and must outlive
 * all borrowed executors). Signature is `void* (*)(void*)` to match the
 * nros_zephyr_tier_task_create shim's entry type. */
static void* zephyr_tier_task(void* arg) {
    nros_zephyr_tier_ctx_t* ctx = (nros_zephyr_tier_ctx_t*)arg;

    /* Open a borrowed executor that shares the primary session. The primary
     * executor (boot thread) must outlive this task — the boot spin loop runs
     * forever, enforcing this. */
    int rc = nros_cpp_executor_open_over_session(ctx->session_handle, "tier_node", ctx->domain_id,
                                                 ctx->executor_storage);
    if (rc != 0) {
        /* Cannot open the borrowed executor; park forever (boot thread
         * continues). NOTE (issue #144): the spawn of the next tier sits AFTER
         * this tier's setup, so failing here HALTS the chain — ctx->rest will
         * not start. Intentional (a degraded deploy), but a fault-isolation
         * change from the pre-#144 loop-spawn. */
        for (;;) {
            nros_zephyr_msleep(1000);
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
            /* Setup failed; close the borrowed executor and park. As with the
             * open failure above, this HALTS the chain (issue #144). */
            nros_cpp_fini(ctx->executor_storage);
            for (;;) {
                nros_zephyr_msleep(1000);
            }
        }
    }

    /* issue #144 — this tier's setup is done, so bringing up the next tier can
     * no longer race our declares: spawn rest[0] (carrying rest[1..]). A failed
     * DOWNSTREAM spawn must NOT stop this tier spinning its own work, so ignore
     * the return (zephyr_spawn_next_tier frees what it allocated on failure). */
    (void)zephyr_spawn_next_tier(ctx->session_handle, (uint8_t)ctx->domain_id, ctx->rest,
                                 ctx->n_rest);

    /* Spin loop. Pass the tier period as the spin_once timeout — a BLOCKING
     * read drives the shared session's TX/handshake from the spin path and
     * yields the CPU cooperatively (mirrors the Rust zephyr run_tiers tier
     * loop, which spins with a real period and no extra sleep). */
    uint32_t period_ms = (uint32_t)(ctx->spin_period_us / 1000u);
    if (period_ms < SPIN_PERIOD_FLOOR_MS) {
        period_ms = SPIN_PERIOD_FLOOR_MS;
    }
    for (;;) {
        nros_cpp_spin_once(ctx->executor_storage, (int32_t)period_ms);
    }

    /* Unreachable — the spin loop never exits; satisfies the non-void return. */
    return NULL;
}

/* zephyr_spawn_next_tier — issue #144 chained tier spawn.
 *
 * Spawns exactly ONE k_thread (via the shim) for remaining[0], handing it
 * remaining[1..] as its own `rest` so the chain continues once its setup
 * completes. Empty `remaining` → nothing left, return 0. Serializing spawns
 * behind each setup guarantees no two setup() (entity declare) calls run
 * concurrently on the shared zenoh-pico session — the interest-handshake race
 * that silently closes a losing publisher's write filter.
 *
 * On any alloc/create failure, frees what IT allocated and returns -1. It does
 * NOT touch the caller's storage. */
static int zephyr_spawn_next_tier(void* session_handle, uint8_t domain_id,
                                  const nros_tier_spec_t* remaining, size_t n_remaining) {
    if (n_remaining == 0u) {
        return 0;
    }
    const nros_tier_spec_t* t = &remaining[0];

    /* Allocate executor storage for this tier. */
    void* tier_exec = nros_platform_alloc(NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES);
    if (tier_exec == NULL) {
        return -1;
    }
    memset(tier_exec, 0, NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES);

    /* Allocate the tier task context (lives for firmware lifetime). */
    nros_zephyr_tier_ctx_t* ctx =
        (nros_zephyr_tier_ctx_t*)nros_platform_alloc(sizeof(nros_zephyr_tier_ctx_t));
    if (ctx == NULL) {
        nros_platform_dealloc(tier_exec);
        return -1;
    }

    ctx->session_handle = session_handle;
    ctx->domain_id = (uint32_t)domain_id;
    ctx->groups = t->groups;
    ctx->n_groups = t->n_groups;
    ctx->spin_period_us = t->spin_period_us;
    ctx->setup = t->setup;
    ctx->executor_storage = tier_exec;
    /* Chain tail: this task will spawn remaining[1] after its own setup. */
    ctx->rest = remaining + 1;
    ctx->n_rest = n_remaining - 1u;

    /* RAW Zephyr priority from the tier spec (the system.toml
     * [tiers.*.zephyr].priority is the numeric Zephyr value verbatim —
     * negatives are cooperative). Clamp to int32 range. The shim manages the
     * tier's stack from its static pool (NROS_ZEPHYR_TIER_STACK_SIZE), so
     * t->stack_bytes is not consulted here. */
    int64_t p = t->priority;
    if (p > (int64_t)INT32_MAX) {
        p = (int64_t)INT32_MAX;
    } else if (p < (int64_t)INT32_MIN) {
        p = (int64_t)INT32_MIN;
    }

    int rc = nros_zephyr_tier_task_create(zephyr_tier_task, ctx, (int32_t)p,
                                          (t->name != NULL) ? t->name : "nros_tier");
    if (rc != 0) {
        nros_platform_dealloc(ctx);
        nros_platform_dealloc(tier_exec);
        return -1;
    }
    return 0;
}

/* nros_board_zephyr_run_tiers — phase-281 W3a (RFC-0015 Model 1 embedded).
 *
 * Called from ZephyrBoard::run_tiers (main.hpp) which is called from the
 * generated `int main(void)`. By this point: the Zephyr kernel is running, the
 * network is up (CONFIG_NET_CONFIG_AUTO_INIT), and we are executing on the
 * main() thread.
 *
 * The `tiers` array is laid out identically to nros_native_tier_spec_t (the
 * caller casts NativeTierSpec* → nros_native_tier_spec_t* → nros_tier_spec_t*).
 *
 * `locator`      — zenoh connect endpoint (compile-time NROS_ENTRY_LOCATOR)
 * `domain_id`    — ROS domain ID (compile-time NROS_ENTRY_DOMAIN_ID)
 * `session_name` — primary session / node name; NULL or empty → "node"
 * `tiers`        — tier-spec array, highest-priority-first (codegen order)
 * `n_tiers`      — number of tiers (>= 1)
 *
 * Returns: normally never returns (the boot tier spins forever). Returns a
 * non-zero error code only if the primary session open or a spawn fails. */
int32_t nros_board_zephyr_run_tiers(const char* locator, uint8_t domain_id,
                                    const char* session_name, const nros_tier_spec_t* tiers,
                                    size_t n_tiers) {
    if (tiers == NULL || n_tiers == 0) {
        return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
    }

    /* Weak network-readiness gate (no-op on the canonical Zephyr
     * auto-init path; a board/app may provide a strong override). */
    nros_board_network_wait();

    /* --- Open the primary (owning) executor on the boot thread --- */
    const char* sn = (session_name != NULL && session_name[0] != '\0') ? session_name : "node";

    /* Allocate executor storage from the Zephyr heap (8-byte aligned). */
    void* boot_storage = nros_platform_alloc(NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES);
    if (boot_storage == NULL) {
        return -1; /* NROS_CPP_RET_ERROR */
    }
    memset(boot_storage, 0, NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES);

    int rc = nros_cpp_init(locator, domain_id, sn, NULL, boot_storage);
    if (rc != 0) {
        nros_platform_dealloc(boot_storage);
        return (int32_t)rc;
    }

    /* Retrieve the session handle so non-boot tiers can borrow it. The handle
     * remains valid as long as boot_storage lives (it lives forever — the boot
     * spin loop never returns). */
    void* session_handle = nros_cpp_executor_session_handle(boot_storage);

    /* --- Run boot tier (tiers[0]) setup on THIS thread FIRST --- */
    /* issue #144 — boot setup runs BEFORE any tier spawn: concurrent entity
     * declares from two threads race the zenoh-pico interest handshake, and the
     * losing publisher's write filter stays closed (every put silently
     * dropped). Running boot's declares first, then CHAINING the remaining
     * spawns, makes setup order total (boot, t1, t2, …) so no two declares
     * overlap. */
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

    /* --- Kick off the chained spawn (tiers[1] carrying tiers[2..]) --- */
    /* A boot-side spawn failure is fatal: tear down boot_storage (which the
     * helper never touches) and return error. Downstream tier tasks handle
     * their own spawn failures by parking + continuing to spin. */
    int src = zephyr_spawn_next_tier(session_handle, domain_id, &tiers[1], n_tiers - 1u);
    if (src != 0) {
        nros_cpp_fini(boot_storage);
        nros_platform_dealloc(boot_storage);
        return -1;
    }

    /* The boot thread runs tiers[0] itself — adopt its declared RAW priority
     * (the spawned tiers already got theirs at k_thread_create; without this
     * the boot tier keeps the main-thread default and the declared tier QoS
     * would not hold for it). */
    {
        int64_t bp = boot->priority;
        if (bp > (int64_t)INT32_MAX) {
            bp = (int64_t)INT32_MAX;
        } else if (bp < (int64_t)INT32_MIN) {
            bp = (int64_t)INT32_MIN;
        }
        nros_zephyr_set_current_priority((int32_t)bp);
    }

    /* Boot tier spin loop — runs forever. Blocking-read spin_once (period as
     * timeout) so the boot session's zenoh handshake is driven from the spin
     * path and the CPU yields cooperatively (mirrors the Rust zephyr
     * run_tiers boot loop). */
    uint32_t period_ms = (uint32_t)(boot->spin_period_us / 1000u);
    if (period_ms < SPIN_PERIOD_FLOOR_MS) {
        period_ms = SPIN_PERIOD_FLOOR_MS;
    }
    for (;;) {
        nros_cpp_spin_once(boot_storage, (int32_t)period_ms);
    }

    /* Unreachable — satisfies the compiler. */
    nros_cpp_fini(boot_storage);
    nros_platform_dealloc(boot_storage);
    return 0;
}
