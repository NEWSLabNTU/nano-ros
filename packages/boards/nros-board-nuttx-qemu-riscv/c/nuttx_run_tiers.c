/*
 * nuttx_run_tiers.c — phase-281 W3 (nuttx): C++ embedded multi-tier entry for NuttX.
 *
 * Implements `nros_board_nuttx_run_tiers`, the NuttX analog of the FreeRTOS
 * `nros_board_freertos_run_tiers` and the Zephyr `nros_board_zephyr_run_tiers`:
 * opens ONE RMW session on the calling thread (the NuttX `app_main` thread — an
 * already-running post-boot task), runs the boot tier's setup, then CHAIN-spawns
 * the remaining tiers (each with a borrowed executor sharing the one session) so
 * their setups serialize (issue #144), and runs the boot tier on the caller.
 *
 * NuttX deltas vs the FreeRTOS / Zephyr mirrors:
 *   - NuttX is POSIX, so non-boot tiers spawn via `pthread_create` (NOT
 *     xTaskCreate / k_thread and NOT a tier shim): each tier gets its own
 *     `pthread_attr_t` with `pthread_attr_setstacksize` (tier.stack_bytes, else
 *     a 16 KiB default — the executor working set is heap-allocated via
 *     `nros_platform_alloc`, so the tier stack only carries call frames) and
 *     `SCHED_FIFO` + `pthread_attr_setschedparam` at the tier's RAW NuttX
 *     priority (`[tiers.*.nuttx].priority` verbatim), with
 *     `PTHREAD_EXPLICIT_SCHED` so the attr's policy/priority actually take;
 *   - the boot tier adopts its declared RAW priority on the caller thread via
 *     `pthread_setschedparam(pthread_self(), SCHED_FIFO, …)` (the spawned tiers
 *     get theirs at `pthread_create`);
 *   - NuttX owns boot + networking: the board FFI `main` brings up `eth0`
 *     (virtio-net) BEFORE `app_main` runs (phase-280), so there is no network
 *     bring-up here — just the weak `nros_board_network_wait()` gate (mirrors
 *     `NuttxBoard::run_components` in main.hpp), not the FreeRTOS lwIP path.
 *
 * Called from `NuttxBoard::run_tiers` (main.hpp) via the generated
 * `nros_app_main` (RFC-0015 Model 1 embedded, RFC-0043 typed path — the NuttX
 * startup path calls `app_main`, like FreeRTOS, NOT Zephyr's `main(void)`).
 *
 * phase-281 W3 / RFC-0015 §5.
 */

#include <pthread.h>
#include <sched.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

/* --- nros-cpp C FFI forward declarations ---
 *
 * These symbols are defined in nros-cpp (Rust) and linked into the final image.
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
 * that must block for link-up. On the canonical NuttX path the board FFI `main`
 * already brought up eth0 before app_main (phase-280), so this is a no-op — the
 * kernel-side network config runs before the app entry. */
extern void nros_board_network_wait(void);

/* RFC-0034 — the sole sanctioned allocation seam (wraps the NuttX heap); a
 * single counter sees all traffic. Direct malloc/free are forbidden. */
extern void* nros_platform_alloc(size_t size);
extern void nros_platform_dealloc(void* ptr);

/* --- Executor storage sizing ---
 *
 * nros_cpp_init / nros_cpp_executor_open_over_session both need storage of
 * CPP_EXECUTOR_OPAQUE_U64S * 8 bytes, 8-byte aligned. The exact per-build value
 * is in the cmake-generated nros_cpp_config_generated.h; to keep this seam
 * standalone-compilable (mirroring freertos_run_tiers.c / zephyr_run_tiers.c,
 * which cannot include the generated header) we use the NuttX/embedded ARM
 * fallback (79304 bytes, nros_cpp_config_generated_nuttx.h) rounded up to 80 KiB
 * for headroom. nros_platform_alloc on the NuttX heap returns aligned memory. */
/* issue #245 — prefer the REAL per-build executor size when the generated
 * header is visible to this compile; the hardcoded fallback silently went
 * 32 bytes short on Zephyr when the executor grew (heap-corruption crash).
 * If this platform's executor outgrows the fallback, the same corruption
 * follows — keep the generated-header path working. */
#if defined(__has_include)
#if __has_include(<nros/nros_cpp_config_generated.h>)
#include <nros/nros_cpp_config_generated.h>
#endif
#endif
#ifdef NROS_CPP_EXECUTOR_STORAGE_SIZE
#define NROS_NUTTX_EXECUTOR_STORAGE_BYTES ((NROS_CPP_EXECUTOR_STORAGE_SIZE + 7u) & ~7u)
#else
#define NROS_NUTTX_EXECUTOR_STORAGE_BYTES 81920u
#endif

/* --- Per-tier pthread stack ---
 *
 * The borrowed executor's working set (the 80 KiB above + zenoh-pico buffers)
 * lives on the heap via nros_platform_alloc, so a tier pthread's stack only
 * carries call frames — 16 KiB is a sane default. `[tiers.*.nuttx].stack_bytes`
 * overrides it when set. NOTE (mirrors the freertos seam's stack note): this
 * default is untuned for the full run_tiers path under QEMU; runtime-proving
 * (the next wave) may need to raise it. */
#define NROS_NUTTX_TIER_STACK_BYTES 16384u

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
    /* RFC-0052 W2 — appended (ABI append-only): CPU pin + 1 (0 = unpinned)
     * and the ThreadX preemption threshold (-1 = unset; bake-validated
     * ThreadX-only, so this mirror never consumes it). Keep the offset
     * comment above in sync when appending further. */
    uint32_t core_plus1;
    int64_t preempt_threshold;
    /* phase-296 W5.7 — appended: RTOS-agnostic real-time policy (NULL/0 =
     * unset). Not consumed by this mirror yet (NuttX kernel-native sporadic
     * is a follow-up); rides for layout parity. */
    const char* tier_class;
    uint64_t period_us;
    uint64_t budget_us;
    uint64_t deadline_us;
    const char* deadline_policy;
} nros_tier_spec_t;

/* --- Per-tier thread context ---
 *
 * Heap-allocated by the spawning thread before pthread_create; lives for the
 * firmware lifetime (the spawned thread never returns). executor_storage is a
 * separate heap block passed to nros_cpp_executor_open_over_session. */
typedef struct {
    void* session_handle;
    uint32_t domain_id;
    const char* const* groups;
    size_t n_groups;
    uint64_t spin_period_us;
    nros_tier_setup_fn_t setup;
    void* executor_storage;
    /* issue #144 — chained spawn tail: the tiers still to bring up AFTER this
     * one. This thread spawns rest[0] (carrying rest[1..]) only after its own
     * setup returns, so no two setups overlap on the shared session. */
    const nros_tier_spec_t* rest;
    size_t n_rest;
    /* phase-296 W5.9 — tier identity + sporadic policy, carried so the tier
     * thread can self-apply SCHED_SPORADIC (mirrors the zephyr ctx append). */
    const char* name;
    const char* tier_class;
    uint64_t budget_us;
    uint64_t period_us;
    int64_t priority;
    /* phase-296 W5.11 — placement dim: CPU pin + 1 (0 = unpinned), carried so
     * the spawned tier thread can self-apply its core affinity. */
    uint32_t core_plus1;
} nros_nuttx_tier_ctx_t;

/* Forward decl — nuttx_tier_thread and nuttx_spawn_next_tier are mutually
 * recursive (each tier's thread spawns the next tier via this helper). */
static int nuttx_spawn_next_tier(void* session_handle, uint8_t domain_id,
                                 const nros_tier_spec_t* remaining, size_t n_remaining);

/* Minimum spin delay: 1 ms. */
#define SPIN_PERIOD_FLOOR_MS 1u

/* Clamp a raw tier priority (int64 from the tier spec) to the SCHED_FIFO range
 * NuttX accepts, so pthread_attr_setschedparam / pthread_setschedparam never
 * reject it. */
static int nuttx_clamp_priority(int64_t p) {
    int lo = sched_get_priority_min(SCHED_FIFO);
    int hi = sched_get_priority_max(SCHED_FIFO);
    if (p < (int64_t)lo) {
        return lo;
    }
    if (p > (int64_t)hi) {
        return hi;
    }
    return (int)p;
}

/* phase-296 W5.9 — NuttX kernel-native SPORADIC SERVER (the budget dim's
 * first `Native` realization, RFC-0052). Applies SCHED_SPORADIC on the
 * CALLING thread when the tier is real-time with BOTH a budget and a
 * replenishment period: runs at the tier priority while `budget_us` of CPU
 * remains in each `period_us` window, dropping to the low background
 * priority when exhausted (POSIX sporadic server; NuttX implements it when
 * CONFIG_SCHED_SPORADIC=y). The marker prints ONLY when the kernel actually
 * accepted the policy — an image without the config (or a kernel rejection)
 * falls back to the already-applied SCHED_FIFO, loudly, and the executor's
 * cooperative Sporadic SchedContext (W3a) remains the enforcement.
 * Non-static: the Rust `nros-board-nuttx` run_tiers externs and self-applies
 * through this same helper (one implementation, one marker). The printf
 * literal MUST match `nros_tests::output::NUTTX_SPORADIC_MARKER`. */
int nros_nuttx_apply_current_sporadic(const char* name, const char* tier_class, uint64_t budget_us,
                                      uint64_t period_us, int64_t priority);
int nros_nuttx_apply_current_sporadic(const char* name, const char* tier_class, uint64_t budget_us,
                                      uint64_t period_us, int64_t priority) {
    if (tier_class == NULL || strcmp(tier_class, "real_time") != 0 || budget_us == 0u ||
        period_us == 0u) {
        return 0;
    }
#ifdef CONFIG_SCHED_SPORADIC
    struct sched_param sp;
    memset(&sp, 0, sizeof(sp));
    sp.sched_priority = nuttx_clamp_priority(priority);
    sp.sched_ss_low_priority = sched_get_priority_min(SCHED_FIFO);
    sp.sched_ss_max_repl = 1;
    sp.sched_ss_repl_period.tv_sec = (time_t)(period_us / 1000000u);
    sp.sched_ss_repl_period.tv_nsec = (long)(period_us % 1000000u) * 1000L;
    sp.sched_ss_init_budget.tv_sec = (time_t)(budget_us / 1000000u);
    sp.sched_ss_init_budget.tv_nsec = (long)(budget_us % 1000000u) * 1000L;
    int rc = pthread_setschedparam(pthread_self(), SCHED_SPORADIC, &sp);
    if (rc == 0) {
        printf("nros: sporadic budget set tier=`%s` %lluus/%lluus\n", (name != NULL) ? name : "?",
               (unsigned long long)budget_us, (unsigned long long)period_us);
        return 1;
    }
    printf("nros: sporadic budget FAILED tier=`%s` rc=%d — tier stays SCHED_FIFO "
           "(executor Sporadic SchedContext is the enforcement)\n",
           (name != NULL) ? name : "?", rc);
#else
    /* Fail-loud (RFC-0052): the tier DECLARED a sporadic budget but this
     * kernel was built without CONFIG_SCHED_SPORADIC — the declared policy
     * cannot be honored natively. The executor's cooperative Sporadic
     * SchedContext (W3a) is the enforcement; say so once per tier. */
    printf("nros: sporadic budget declared for tier=`%s` but kernel lacks "
           "CONFIG_SCHED_SPORADIC — executor SchedContext only\n",
           (name != NULL) ? name : "?");
    (void)priority;
#endif
    return 0;
}

/* phase-296 W5.11 — NuttX SMP core affinity (the placement dim's `Native`
 * realization, RFC-0052). Pins the CALLING thread to `core_plus1 - 1` when a
 * tier declares a `core` (core_plus1 > 0 — the emit_c/macro `core + 1` encoding,
 * 0 = unpinned). The marker prints ONLY when the kernel accepted the pin; a
 * uniprocessor build (no CONFIG_SMP) or a rejection falls back LOUDLY — the
 * placement is NEVER silently dropped (the prior behavior: the ABI carried
 * `core_plus1` but no consumer applied it). Non-static: the Rust
 * `nros-board-nuttx` run_tiers externs and self-applies through this same
 * helper (one implementation, one marker). The printf literals MUST match
 * `nros_tests::output::NUTTX_CORE_PIN_MARKER` / `_FALLBACK_MARKER`. */
int nros_nuttx_apply_current_affinity(const char* name, uint32_t core_plus1);
int nros_nuttx_apply_current_affinity(const char* name, uint32_t core_plus1) {
    if (core_plus1 == 0u) {
        return 0; /* unpinned */
    }
    int cpu = (int)(core_plus1 - 1u);
#ifdef CONFIG_SMP
    cpu_set_t set;
    CPU_ZERO(&set);
    CPU_SET(cpu, &set);
    int rc = pthread_setaffinity_np(pthread_self(), sizeof(set), &set);
    if (rc == 0) {
        printf("nros: core pin tier=`%s` cpu=%d\n", (name != NULL) ? name : "?", cpu);
        return 1;
    }
    printf("nros: core pin FAILED tier=`%s` cpu=%d rc=%d — tier runs unpinned\n",
           (name != NULL) ? name : "?", cpu, rc);
#else
    /* Fail-loud (RFC-0052): the tier DECLARED a `core` but this kernel was
     * built without CONFIG_SMP — no affinity API to honor it. */
    printf("nros: core pin FAILED tier=`%s` cpu=%d — kernel lacks CONFIG_SMP, "
           "tier runs unpinned\n",
           (name != NULL) ? name : "?", cpu);
#endif
    return 0;
}

/* nuttx_tier_thread — body of each non-boot tier thread.
 *
 * Opens a borrowed executor over the shared session, gates it to the tier's
 * callback groups, calls the tier's setup function, spawns the next tier
 * (issue #144), then spins forever at the tier's declared period. On failure:
 * parks (never returns — the boot thread is the session owner and must outlive
 * all borrowed executors). Signature is `void* (*)(void*)` to match
 * pthread_create's entry type. */
static void* nuttx_tier_thread(void* arg) {
    nros_nuttx_tier_ctx_t* ctx = (nros_nuttx_tier_ctx_t*)arg;

    /* phase-296 W5.9 — upgrade this tier thread to the kernel sporadic
     * server when its policy declares budget+period (else it keeps the
     * SCHED_FIFO priority set at pthread_create). */
    (void)nros_nuttx_apply_current_sporadic(ctx->name, ctx->tier_class, ctx->budget_us,
                                            ctx->period_us, ctx->priority);
    /* phase-296 W5.11 — placement dim: pin to the declared core (fail-loud). */
    (void)nros_nuttx_apply_current_affinity(ctx->name, ctx->core_plus1);

    /* Open a borrowed executor that shares the primary session. The primary
     * executor (boot thread) must outlive this thread — the boot spin loop runs
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
            usleep(1000000u);
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
                usleep(1000000u);
            }
        }
    }

    /* issue #144 — this tier's setup is done, so bringing up the next tier can
     * no longer race our declares: spawn rest[0] (carrying rest[1..]). A failed
     * DOWNSTREAM spawn must NOT stop this tier spinning its own work, so ignore
     * the return (nuttx_spawn_next_tier frees what it allocated on failure). */
    (void)nuttx_spawn_next_tier(ctx->session_handle, (uint8_t)ctx->domain_id, ctx->rest,
                                ctx->n_rest);

    /* Spin loop. Pass the tier period as the spin_once timeout — a BLOCKING
     * read drives the shared session's TX/handshake from the spin path and
     * yields the CPU cooperatively (on NuttX `zpico_spin_once` paces with
     * z_sleep_ms; the preemptive scheduler releases the CPU to the zenoh-pico
     * read/lease tasks). Mirrors the Rust nuttx run_tiers tier loop. */
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

/* nuttx_spawn_next_tier — issue #144 chained tier spawn.
 *
 * Spawns exactly ONE pthread for remaining[0], handing it remaining[1..] as its
 * own `rest` so the chain continues once its setup completes. Empty `remaining`
 * → nothing left, return 0. Serializing spawns behind each setup guarantees no
 * two setup() (entity declare) calls run concurrently on the shared zenoh-pico
 * session — the interest-handshake race that silently closes a losing
 * publisher's write filter.
 *
 * On any alloc/create failure, frees what IT allocated and returns -1. It does
 * NOT touch the caller's storage. */
static int nuttx_spawn_next_tier(void* session_handle, uint8_t domain_id,
                                 const nros_tier_spec_t* remaining, size_t n_remaining) {
    if (n_remaining == 0u) {
        return 0;
    }
    const nros_tier_spec_t* t = &remaining[0];

    /* Allocate executor storage for this tier. */
    void* tier_exec = nros_platform_alloc(NROS_NUTTX_EXECUTOR_STORAGE_BYTES);
    if (tier_exec == NULL) {
        return -1;
    }
    memset(tier_exec, 0, NROS_NUTTX_EXECUTOR_STORAGE_BYTES);

    /* Allocate the tier thread context (lives for firmware lifetime). */
    nros_nuttx_tier_ctx_t* ctx =
        (nros_nuttx_tier_ctx_t*)nros_platform_alloc(sizeof(nros_nuttx_tier_ctx_t));
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
    /* Chain tail: this thread will spawn remaining[1] after its own setup. */
    ctx->rest = remaining + 1;
    ctx->n_rest = n_remaining - 1u;
    ctx->name = t->name;
    ctx->tier_class = t->tier_class;
    ctx->budget_us = t->budget_us;
    ctx->period_us = t->period_us;
    ctx->priority = t->priority;
    ctx->core_plus1 = t->core_plus1;

    /* Build the tier pthread attributes: detached (never joined — the tier
     * thread spins forever), an explicit stack size (tier.stack_bytes, else the
     * 16 KiB default), and SCHED_FIFO at the tier's RAW NuttX priority with
     * PTHREAD_EXPLICIT_SCHED so the policy/priority actually apply (default is
     * PTHREAD_INHERIT_SCHED → the creator's params). */
    pthread_attr_t attr;
    int arc = pthread_attr_init(&attr);
    if (arc != 0) {
        nros_platform_dealloc(ctx);
        nros_platform_dealloc(tier_exec);
        return -1;
    }
    (void)pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);

    size_t stack_bytes = (t->stack_bytes > 0u) ? t->stack_bytes : NROS_NUTTX_TIER_STACK_BYTES;
    (void)pthread_attr_setstacksize(&attr, stack_bytes);

    (void)pthread_attr_setschedpolicy(&attr, SCHED_FIFO);
    (void)pthread_attr_setinheritsched(&attr, PTHREAD_EXPLICIT_SCHED);
    struct sched_param sp;
    memset(&sp, 0, sizeof(sp));
    sp.sched_priority = nuttx_clamp_priority(t->priority);
    (void)pthread_attr_setschedparam(&attr, &sp);

    pthread_t tid;
    int ret = pthread_create(&tid, &attr, nuttx_tier_thread, ctx);
    (void)pthread_attr_destroy(&attr);
    if (ret != 0) {
        nros_platform_dealloc(ctx);
        nros_platform_dealloc(tier_exec);
        return -1;
    }
    return 0;
}

/* nros_board_nuttx_run_tiers — phase-281 W3 (RFC-0015 Model 1 embedded).
 *
 * Called from NuttxBoard::run_tiers (main.hpp) which is called from the
 * generated nros_app_main. By this point: the NuttX kernel is running, eth0 is
 * up (the board FFI main brought up virtio-net before app_main — phase-280), and
 * we are executing on the app_main thread.
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
int32_t nros_board_nuttx_run_tiers(const char* locator, uint8_t domain_id, const char* session_name,
                                   const nros_tier_spec_t* tiers, size_t n_tiers) {
    if (tiers == NULL || n_tiers == 0) {
        return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
    }

    /* Weak network-readiness gate (no-op on the canonical NuttX path — eth0 is
     * already up from the board FFI main; a board/app may provide a strong
     * override). */
    nros_board_network_wait();

    /* --- Open the primary (owning) executor on the boot thread --- */
    const char* sn = (session_name != NULL && session_name[0] != '\0') ? session_name : "node";

    /* Allocate executor storage from the NuttX heap (aligned). */
    void* boot_storage = nros_platform_alloc(NROS_NUTTX_EXECUTOR_STORAGE_BYTES);
    if (boot_storage == NULL) {
        return -1; /* NROS_CPP_RET_ERROR */
    }
    memset(boot_storage, 0, NROS_NUTTX_EXECUTOR_STORAGE_BYTES);

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
     * helper never touches) and return error. Downstream tier threads handle
     * their own spawn failures by parking + continuing to spin. */
    int src = nuttx_spawn_next_tier(session_handle, domain_id, &tiers[1], n_tiers - 1u);
    if (src != 0) {
        nros_cpp_fini(boot_storage);
        nros_platform_dealloc(boot_storage);
        return -1;
    }

    /* The boot thread runs tiers[0] itself — adopt its declared RAW priority
     * (the spawned tiers already got theirs at pthread_create; without this the
     * boot tier keeps the app_main-thread default and the declared tier QoS
     * would not hold for it). */
    {
        struct sched_param bsp;
        memset(&bsp, 0, sizeof(bsp));
        bsp.sched_priority = nuttx_clamp_priority(boot->priority);
        (void)pthread_setschedparam(pthread_self(), SCHED_FIFO, &bsp);
    }
    /* phase-296 W5.9 — boot tier upgrades to the kernel sporadic server too,
     * when declared (overrides the FIFO adopt above). */
    (void)nros_nuttx_apply_current_sporadic(boot->name, boot->tier_class, boot->budget_us,
                                            boot->period_us, boot->priority);
    /* phase-296 W5.11 — placement dim: pin the boot tier too (fail-loud). */
    (void)nros_nuttx_apply_current_affinity(boot->name, boot->core_plus1);

    /* Boot tier spin loop — runs forever. Blocking-read spin_once (period as
     * timeout) so the boot session's zenoh handshake is driven from the spin
     * path and the CPU yields cooperatively (mirrors the Rust nuttx run_tiers
     * boot loop). */
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
