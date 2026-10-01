/*
 * freertos_run_tiers.c — Phase 274.W3: C++ embedded multi-tier entry for FreeRTOS.
 *
 * Implements `nros_board_freertos_run_tiers`, the FreeRTOS analog of the native
 * `nros_board_native_run_tiers`: opens ONE RMW session on the calling task (the
 * startup.c `app_task`), runs the boot tier's setup, then CHAIN-spawns the
 * remaining tiers (each with a borrowed executor sharing the session) so their
 * setups serialize (issue #144), and runs the boot tier on the caller.
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
extern int nros_cpp_executor_derive_min_stack_headroom(void* executor, size_t stack_bytes);

extern int nros_cpp_spin_once(void* handle, int32_t timeout_ms);

/* issue 0636 option 3 — the tier spin loop's scheduled gap.
 *
 * ONE implementation, shared with the Rust tier runners
 * (`nros_platform::board::tier`): a second spelling per language is how the
 * tier-priority marker drifted, which is most of what this issue was. Pass 0
 * on the first call and keep the returned value; the sleep, when the rule
 * calls for one, happens inside. State is an opaque u64 rather than a struct
 * so there is no hand-mirrored FFI layout to drift (three prior incidents). */
extern uint64_t nros_tier_spin_gap_step(uint64_t state, uint64_t iter_start_ns, uint64_t now_ns,
                                        uint32_t spin_period_us);
extern uint64_t nros_platform_clock_ns(void);

/* nros_board_network_wait: weak no-op in <nros/main.h> (phase-432 W3.1 moved
 * it there from the C++ sibling main.hpp so a pure C entry links); strong
 * override on boards that need an extra poll-wait after network bring-up. On
 * MPS2-AN385 the startup.c already waits 2 s and polls lwIP, so this is
 * usually a no-op. */
extern void nros_board_network_wait(void);

/* RFC-0034 — the sole sanctioned allocation seam (wraps the FreeRTOS heap); a
 * single counter sees all traffic. Direct pvPortMalloc/vPortFree are forbidden
 * (check-no-direct-kernel-alloc). */
extern void* nros_platform_alloc(size_t size);
extern void nros_platform_dealloc(void* ptr);

/* --- Executor storage sizing ---
 *
 * nros_cpp_init / nros_cpp_executor_open_over_session build a `CppContext` in
 * storage the caller supplies, with no size argument. Issue 1568 — the size
 * comes from the LINKED library (`nros_cpp_executor_storage_size`), and the
 * refusal for a caller's block is the library's too
 * (`nros_cpp_executor_storage_check`), because this file is compiled where no
 * nros header is in reach (the cargo lane's `build.rs` glue).
 * It used to `__has_include` the per-build header and fall back to 81,920
 * bytes — "80 KiB of headroom" over a 79,304-byte executor. The executor grew
 * past it: the CMake lane saw the header and was exact (88,480 on mps2-an385), the cargo lane did
 * not and would have been short. One number, from the object that is built in the storage, cannot
 * drift. */
extern size_t nros_cpp_executor_storage_size(void);
extern int nros_cpp_executor_storage_check(const void* storage, size_t bytes);

/* The board's console, used for the loud placement-dim note below. Declared
 * here (no shared header).
 *
 * phase-370 — this was `semihosting_write0` (the ARM helper in
 * `freertos_hooks.c`), which made a file that is otherwise pure FreeRTOS + the
 * nros-cpp FFI depend on one board family's DEBUG TRANSPORT. It linked only
 * because every FreeRTOS board was a Cortex-M under QEMU; the POSIX simulator
 * board is not, and the failure was an undefined reference from this TU naming
 * a symbol nothing in this file is about.
 *
 * Every FreeRTOS board compiles exactly one board TU, so each supplies one
 * STRONG definition — no weak fallback, and no board `#ifdef` here. */
extern void nros_board_freertos_console_write(const char* s);

/* --- Local tier-spec type ---
 *
 * Mirror of nros_native_tier_spec_t (nros/main.h). Layout MUST match: same
 * field order, same types, same ABI (verified by the C++ caller casting
 * NativeTierSpec* → nros_native_tier_spec_t* → this type). Gated by
 * `check-ffi-struct-mirrors`, which compares this declaration to main.h's
 * field by field (the `nros_tier_setup_fn_t` typedef being the one legitimate
 * spelling difference). On 32-bit ARM:
 *   offset 0:  name (ptr, 4 B)
 *   offset 4:  groups (ptr, 4 B)
 *   offset 8:  n_groups (size_t, 4 B)
 *   offset 12: [pad 4 B]
 *   offset 16: priority (int64_t, 8 B)
 *   offset 24: stack_bytes (size_t, 4 B)
 *   offset 28: [pad 4 B]
 *   offset 32: spin_period_us (uint64_t, 8 B)
 *   offset 40: setup (fn ptr, 4 B)
 *   offset 44: core_plus1 (uint32_t, 4 B)
 *   offset 48: preempt_threshold (int64_t, 8 B)
 *   offset 56: tier_class (ptr, 4 B)
 *   offset 60: [pad 4 B]
 *   offset 64: period_us (uint64_t, 8 B)
 *   offset 72: budget_us (uint64_t, 8 B)
 *   offset 80: deadline_us (uint64_t, 8 B)
 *   offset 88: deadline_policy (ptr, 4 B)
 *   offset 92: [tail pad 4 B]
 *   total: 96 B */
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
     * unset). Not consumed by this mirror yet (FreeRTOS has no kernel
     * EDF); rides for layout parity. */
    const char* tier_class;
    uint64_t period_us;
    uint64_t budget_us;
    uint64_t deadline_us;
    const char* deadline_policy;
} nros_tier_spec_t;

/* Issue 1598 — one tier task's memory, owned by the generated entry. Mirror of
 * `nros_tier_task_memory_t` in <nros/main.h> (this TU is compiled where no
 * nros header is in reach — the cargo lane's `build.rs` glue). Three pointer-
 * sized fields, so the layout cannot drift by padding; the assert pins it.
 *
 *   stack       — the task's stack (`StackType_t[]`), NULL for "none".
 *   stack_bytes — its size in bytes, as the entry declared it (already raised
 *                 to the port's floor by the entry's macro, issue 0667).
 *   tcb         — the task's `StaticTask_t`. */
typedef struct {
    void* stack;
    size_t stack_bytes;
    void* tcb;
} nros_tier_task_memory_t;
_Static_assert(sizeof(nros_tier_task_memory_t) == 3u * sizeof(void*),
               "nros_tier_task_memory_t mirrors <nros/main.h>: three pointer-sized fields");

/* --- Per-tier task context ---
 *
 * Heap-allocated by the boot task before xTaskCreate; lives for the firmware
 * lifetime (the spawned task never returns). executor_storage is this tier's
 * block of the CALLER's storage (issue 1568 — a file-scope static in the
 * generated entry, not a heap block), passed to
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
    /* issue 1568 — the storage for rest[0] (rest[k] is at k * storage_stride),
     * carried down the chain beside the specs it belongs to (Zephyr's shape,
     * issue 1551). */
    unsigned char* rest_storage;
    size_t storage_stride;
    /* issue 1598 — the task memory for rest[0] (rest[k] at rest_task_memory[k]),
     * carried down the chain beside the specs it belongs to; NULL when the
     * caller supplied none (an `_in` / `_ns` entry), which keeps those tiers on
     * `xTaskCreate`. */
    const nros_tier_task_memory_t* rest_task_memory;
    /* issue 0636 — the tier's identity and declared priority, so the task can
     * ANNOUNCE what it got (#579). The task already holds the priority from
     * `xTaskCreate`; what was missing was any way to say so. */
    const char* name;
    uint32_t priority;
    /* issue 1250 — the EFFECTIVE stack this task was created with, in bytes
     * (`stack_words * 4`), NOT the tier spec's declared `stack_bytes`: an
     * unset spec still gets the 256 KiB default below, and a headroom bound
     * derived from `0` would leave the rule off on exactly the tiers that
     * declared nothing. `freertos_spawn_next_tier` assigns it and
     * `freertos_tier_task` reads it; both arrived in a6aa0f721 and the field
     * itself did not, so every FreeRTOS image — C and C++ alike — failed to
     * compile. */
    size_t stack_bytes;
} nros_freertos_tier_ctx_t;

/* RFC-0052 W2/W5.11 — apply and ANNOUNCE the placement dim for one task.
 *
 * `core_plus1` 0 means unpinned and says nothing. Otherwise the outcome is
 * announced LOUDLY either way — a declared `core` is NEVER silently dropped
 * (before W5.11 the uniprocessor branch was a silent `(void)task`). The
 * literals match `nros_tests::output::FREERTOS_CORE_PIN_MARKER` /
 * `_FALLBACK_MARKER` — keep in lockstep.
 *
 * issue 0636 — a FUNCTION, called by the spawn path AND the boot path, because
 * the boot tier is no longer `tiers[0]`. It is now the LEAST urgent tier, and
 * in `realtime-cpp`'s bringup that is `low` — the one tier that declares
 * `core = 0`. With the announcement living only in the spawn path, moving that
 * tier onto the boot task made the dim vanish from the console, and the
 * `CorePin/freertos/cpp` cell reported it as silently dropped. It was: by this
 * file, not by the kernel.
 *
 * `task` may be NULL for the boot path, which pins the CALLING task — the
 * FreeRTOS affinity API takes NULL to mean exactly that. */
static void freertos_apply_core_pin(TaskHandle_t task, const char* name, uint32_t core_plus1) {
    if (core_plus1 == 0u) {
        return;
    }
    const char* tname = (name != NULL) ? name : "?";
#if defined(configUSE_CORE_AFFINITY) && (configUSE_CORE_AFFINITY == 1)
    vTaskCoreAffinitySet(task, (UBaseType_t)1u << (core_plus1 - 1u));
    nros_board_freertos_console_write("nros: core pin tier=`");
    nros_board_freertos_console_write(tname);
    nros_board_freertos_console_write("`\n");
#else
    nros_board_freertos_console_write("nros: core pin FAILED tier=`");
    nros_board_freertos_console_write(tname);
    nros_board_freertos_console_write("` — FreeRTOS build lacks configUSE_CORE_AFFINITY "
                                      "(uniprocessor), tier runs unpinned\n");
    (void)task;
#endif
}

/* Render an unsigned decimal into `buf` and return a pointer to the first
 * digit. This TU has no stdio ON PURPOSE — the console is a semihosting string
 * writer, not a printf — so the tier-priority marker below formats its own
 * number instead of pulling newlib's formatting into every FreeRTOS image.
 * `buf` must hold at least 11 bytes (10 digits + NUL). */
static const char* freertos_u32_dec(uint32_t v, char* buf, size_t len) {
    size_t i = len - 1u;
    buf[i] = '\0';
    do {
        buf[--i] = (char)('0' + (char)(v % 10u));
        v /= 10u;
    } while (v != 0u && i > 0u);
    return &buf[i];
}

/* issue 0636 / #579 — adopt the tier's declared priority on the CALLING task
 * and ANNOUNCE the outcome, for the SPAWN path and the BOOT path alike.
 *
 * Until this existed, FreeRTOS printed no tier-priority marker in either
 * language, so #579's rule ("every DECLARING tier adopts its priority or says
 * why") was enforced on NuttX only and this kernel was silently exempt. That is
 * how the boot task came to adopt no priority at all: spawned tiers get theirs
 * from `xTaskCreate`, the boot tier got nothing, and with nothing printing
 * there was no cell that could notice. The literals match
 * `nros_tests::output::FREERTOS_TIER_PRIORITY_MARKER` / `_FAILED_MARKER` and
 * are the same strings the NuttX seam prints — keep all three in lockstep.
 *
 * Re-applying on a spawned task is deliberate and mirrors the NuttX arm: the
 * task already holds this priority from `xTaskCreate`, and the point is that
 * ONE helper both applies and reports, so the two paths cannot drift.
 *
 * A caller passing 0 is treated as "undeclared, keep what was inherited", the
 * same convention the NuttX helper uses. NOTE that unlike SCHED_FIFO, 0 is a
 * LEGAL FreeRTOS priority (the idle band), so a tier that genuinely declares
 * `priority = 0` is indistinguishable from one that declared nothing. Both
 * arms share the hole; it is recorded rather than given a second convention
 * here, because a per-kernel spelling of "declared" is how these two seams
 * drifted in the first place. */
static int freertos_apply_tier_priority(const char* name, uint32_t priority) {
    const char* tname = (name != NULL) ? name : "?";
    if (priority == 0u) {
        return 0; /* undeclared — keep inherited priority */
    }
    char numbuf[12];
    const char* pstr = freertos_u32_dec(priority, numbuf, sizeof(numbuf));
    if (priority >= (uint32_t)configMAX_PRIORITIES) {
        /* Fail-loud (RFC-0052): the tier DECLARED a priority this build cannot
         * express. vTaskPrioritySet would clamp it silently, which makes a
         * mis-authored table look honored. */
        nros_board_freertos_console_write("nros: tier priority FAILED tier=`");
        nros_board_freertos_console_write(tname);
        nros_board_freertos_console_write("` prio=");
        nros_board_freertos_console_write(pstr);
        nros_board_freertos_console_write(" — exceeds configMAX_PRIORITIES, tier does NOT "
                                          "hold its declared priority\n");
        return 0;
    }
    vTaskPrioritySet(NULL, (UBaseType_t)priority);
    nros_board_freertos_console_write("nros: tier priority set tier=`");
    nros_board_freertos_console_write(tname);
    nros_board_freertos_console_write("` prio=");
    nros_board_freertos_console_write(pstr);
    nros_board_freertos_console_write("\n");
    return 1;
}

/* issue 0636 — a tier that never STARTS is the same silent drop as a tier that
 * never adopts its priority, and the chain hid it: `freertos_tier_task` ignores
 * the downstream spawn's return on purpose (a failed child must not stop this
 * tier spinning its own work), so an out-of-heap `xTaskCreate` lost a whole
 * tier without a word. Continuing to spin stays correct; being quiet about it
 * does not. Announced through the same fail-loud channel as the dims. */
static void freertos_announce_spawn_failure(const char* name, const char* why) {
    nros_board_freertos_console_write("nros: tier spawn FAILED tier=`");
    nros_board_freertos_console_write((name != NULL) ? name : "?");
    nros_board_freertos_console_write("` — ");
    nros_board_freertos_console_write(why);
    nros_board_freertos_console_write(", tier does NOT start\n");
}

/* Forward decl — freertos_tier_task and freertos_spawn_next_tier are mutually
 * recursive (each tier's task spawns the next tier via this helper). */
static int freertos_spawn_next_tier(void* session_handle, uint8_t domain_id,
                                    const nros_tier_spec_t* remaining, size_t n_remaining,
                                    unsigned char* storage, size_t storage_stride,
                                    const nros_tier_task_memory_t* task_memory);

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

    /* issue 0636 / #579 — this tier already holds its declared priority from
     * `xTaskCreate`; re-applying through the shared helper is what makes it
     * SAY so, on the same line the boot tier uses. */
    (void)freertos_apply_tier_priority(ctx->name, ctx->priority);

    /* Open a borrowed executor that shares the primary session. The primary
     * executor (boot task) must outlive this task — the startup sequence
     * enforces this (the boot task spins forever). */
    int rc = nros_cpp_executor_open_over_session(ctx->session_handle, "tier_node", ctx->domain_id,
                                                 ctx->executor_storage);
    if (rc != 0) {
        /* Cannot open the borrowed executor; idle forever (boot task continues).
         * NOTE (issue #144): the spawn of the next tier sits AFTER this tier's
         * setup, so failing here HALTS the chain — ctx->rest (this tier's
         * downstream tiers) will not start. Intentional (a tier that can't open
         * its executor means a degraded deploy), but it is a fault-isolation
         * change from the pre-#144 loop-spawn where tiers came up independently. */
        for (;;) {
            vTaskDelay(pdMS_TO_TICKS(1000u));
        }
    }

    /* Gate this executor to its tier's callback groups. */
    if (ctx->n_groups > 0 && ctx->groups != NULL) {
        nros_cpp_executor_set_active_groups(ctx->executor_storage, ctx->groups, ctx->n_groups);
    }

    /* phase-436 follow-up — declare a default stack-headroom bound from the
     * stack this tier's task was actually created with. The executor cannot
     * derive one itself (`check_stack_headroom`: it never sees `stack_bytes`),
     * but this layer can, because it just spawned the task with that size.
     *
     * BEFORE `setup()` deliberately: an explicit
     * `nros_cpp_executor_set_min_stack_headroom` inside the tier's setup then
     * overwrites this, so declared beats derived by ordering alone.
     *
     * Spawned tiers only. The boot tier runs on the caller (`app_task`, a
     * 512 KiB stack this layer never sized), so a bound derived from its spec
     * would be measured against the wrong stack. */
    if (nros_cpp_executor_derive_min_stack_headroom(ctx->executor_storage, ctx->stack_bytes) != 0) {
        /* Same fail-loud rule as a spawn failure above: a bound that was never
         * set leaves `stack-headroom-runtime` off, and a monitor that silently
         * fails to arm is indistinguishable from a healthy system. */
        nros_board_freertos_console_write("nros: stack-headroom bound NOT set tier=`");
        nros_board_freertos_console_write((ctx->name != NULL) ? ctx->name : "?");
        nros_board_freertos_console_write("` — the rule stays off\n");
    }

    /* Run the tier's node-setup function. */
    if (ctx->setup != NULL) {
        rc = ctx->setup(ctx->executor_storage);
        if (rc != 0) {
            /* Setup failed; close the borrowed executor and idle. As with the
             * open failure above, this HALTS the chain (issue #144): ctx->rest
             * will not start. Intentional — degraded deploy — but a
             * fault-isolation change from the pre-#144 independent loop-spawn. */
            nros_cpp_fini(ctx->executor_storage);
            for (;;) {
                vTaskDelay(pdMS_TO_TICKS(1000u));
            }
        }
    }

    /* issue #144 — this tier's setup is done, so bringing up the next tier can
     * no longer race our declares: spawn rest[0] (carrying rest[1..]). A failed
     * DOWNSTREAM spawn must NOT stop this tier spinning its own work, so ignore
     * the return (freertos_spawn_next_tier frees what it allocated on failure). */
    (void)freertos_spawn_next_tier(ctx->session_handle, (uint8_t)ctx->domain_id, ctx->rest,
                                   ctx->n_rest, ctx->rest_storage, ctx->storage_stride,
                                   ctx->rest_task_memory);

    /* Spin loop. Pass the tier period as the spin_once timeout — a BLOCKING
     * read (issue #126 defect B): timeout 0 returns immediately and never drives
     * the zenoh-pico session's TX/handshake from the spin path, so the shared
     * session never connects. `run_components` (`component_spin_loop`) and the
     * Rust `run_tiers_entry` both spin with a real timeout; mirror that. */
    uint32_t period_ms = (uint32_t)(ctx->spin_period_us / 1000u);
    if (period_ms < SPIN_PERIOD_FLOOR_MS) {
        period_ms = SPIN_PERIOD_FLOOR_MS;
    }
    /* issue 0636 option 3 — every iteration reaches a scheduling point. The
     * executor's own wait is skipped whenever a wake already fired, so under
     * sustained traffic this loop would otherwise never block, and a task that
     * never blocks never lets a lower-priority tier run.
     *
     * This REPLACES an unconditional `vTaskDelay(1)`, which was the same
     * guarantee hand-rolled for one kernel: it paid a tick on EVERY iteration,
     * including the ones whose spin already blocked, and the three other
     * kernels running the identical loop had nothing. Same rule, one
     * implementation, and cost only on the path that needs it. */
    uint64_t gap_state = 0;
    for (;;) {
        uint64_t iter_start_ns = nros_platform_clock_ns();
        nros_cpp_spin_once(ctx->executor_storage, (int32_t)period_ms);
        gap_state = nros_tier_spin_gap_step(gap_state, iter_start_ns, nros_platform_clock_ns(),
                                            (uint32_t)ctx->spin_period_us);
    }
}

/* freertos_spawn_next_tier — issue #144 chained tier spawn.
 *
 * Spawns exactly ONE FreeRTOS task for remaining[0], handing it remaining[1..]
 * as its own `rest` so the chain continues once its setup completes. Empty
 * `remaining` (n_remaining == 0) → nothing left, return 0. Serializing spawns
 * behind each setup guarantees no two setup() (entity declare) calls run
 * concurrently on the shared zenoh-pico session — the interest-handshake race
 * that silently closes a losing publisher's write filter.
 *
 * On any alloc/xTaskCreate failure, frees what IT allocated and returns -1. It
 * does NOT touch boot_storage — the caller (boot) owns that. */
static int freertos_spawn_next_tier(void* session_handle, uint8_t domain_id,
                                    const nros_tier_spec_t* remaining, size_t n_remaining,
                                    unsigned char* storage, size_t storage_stride,
                                    const nros_tier_task_memory_t* task_memory) {
    if (n_remaining == 0u) {
        return 0;
    }
    const nros_tier_spec_t* t = &remaining[0];

    /* Issue 1568 — this tier's executor is the caller's block, never a heap
     * block: the storage is the generated entry's `.bss` static, sized by the
     * build and placed by the linker. */
    void* tier_exec = storage;
    memset(tier_exec, 0, storage_stride);

    /* Allocate the tier task context (lives for firmware lifetime). */
    nros_freertos_tier_ctx_t* ctx =
        (nros_freertos_tier_ctx_t*)nros_platform_alloc(sizeof(nros_freertos_tier_ctx_t));
    if (ctx == NULL) {
        freertos_announce_spawn_failure(t->name, "tier context allocation failed");
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
    ctx->rest_storage = storage + storage_stride;
    ctx->storage_stride = storage_stride;
    ctx->rest_task_memory = (task_memory != NULL) ? task_memory + 1 : NULL;
    ctx->name = t->name;
    ctx->priority = (uint32_t)((t->priority < 0) ? 0 : t->priority);

    /* Stack size: use the tier spec's stack_bytes if set; else 256 KiB
     * (issue #126 defect A — VERIFIED). A spawned tier task opens a borrowed
     * executor and runs its spin/dispatch; that overflows 64 KiB (HardFault:
     * Prefetch Abort at tskSTACK_FILL_BYTE right after a context switch). At
     * 256 KiB the firmware runs the full run_tiers path with no fault under
     * QEMU mps2-an385. The boot tier keeps the 512 KiB app_task stack.
     * RFC-0052 W2: `[tiers.*.freertos].stack_bytes` now propagates through
     * emit_cpp (the pre-W2 literal hardcoded 0), so a configured stack is
     * honored; this default covers unset specs. */
    uint32_t stack_words = (t->stack_bytes > 0u) ? (uint32_t)(t->stack_bytes / 4u) : (262144u / 4u);
    /* Issue 1598 — the entry owns this tier's stack: its size is what the
     * entry DECLARED (the 256 KiB default above moved to the emitter, which
     * states it where it is reserved, and the port's floor was applied there
     * too — issue 0667). The fallback above is only the `_in`/`_ns` road. */
    if (task_memory != NULL) {
        stack_words = (uint32_t)(task_memory->stack_bytes / sizeof(StackType_t));
    }

    /* The EFFECTIVE stack, not the declared one: an unset `stack_bytes` still
     * gets 256 KiB above, and a bound derived from `0` would leave the
     * stack-headroom rule off on exactly the tiers that declared nothing and
     * therefore most need a default. The field is otherwise unread — nothing
     * assigned it before this. */
    ctx->stack_bytes = (size_t)stack_words * 4u;

    /* Raw FreeRTOS priority from the tier spec (the system.toml [tiers.*.freertos]
     * priority is the numeric FreeRTOS value; clamp to configMAX_PRIORITIES-1). */
    UBaseType_t prio = (t->priority > 0)
                           ? (UBaseType_t)(t->priority < (int64_t)(configMAX_PRIORITIES)
                                               ? t->priority
                                               : (int64_t)(configMAX_PRIORITIES - 1u))
                           : (UBaseType_t)1u;

    TaskHandle_t task = NULL;
    if (task_memory != NULL) {
        /* Issue 1598 — the stack and TCB are the entry's statics: no heap_4
         * block, and an image whose stacks do not fit fails at LINK. */
        task = xTaskCreateStatic(freertos_tier_task, (t->name != NULL) ? t->name : "nros_tier",
                                 stack_words, ctx, prio, (StackType_t*)task_memory->stack,
                                 (StaticTask_t*)task_memory->tcb);
        if (task == NULL) {
            freertos_announce_spawn_failure(t->name, "xTaskCreateStatic refused the entry's "
                                                     "task memory");
            nros_platform_dealloc(ctx);
            return -1;
        }
    } else {
        BaseType_t ret = xTaskCreate(freertos_tier_task, (t->name != NULL) ? t->name : "nros_tier",
                                     stack_words, ctx, prio, &task);
        if (ret != pdPASS) {
            freertos_announce_spawn_failure(t->name, "xTaskCreate failed (FreeRTOS heap)");
            nros_platform_dealloc(ctx);
            return -1;
        }
    }
    /* RFC-0052 W2/W5.11 — core pin (core_plus1: 0 = unpinned) is the placement
     * dim. Only SMP builds (configUSE_CORE_AFFINITY) expose the affinity API;
     * a uniprocessor config CANNOT honor a declared pin. Either way the outcome
     * is announced LOUDLY — a declared `core` is NEVER silently dropped
     * (RFC-0052 fail-loud; before W5.11 the uniprocessor branch was a silent
     * `(void)task`). The literals match nros_tests::output::FREERTOS_CORE_PIN_
     * MARKER / _FALLBACK_MARKER — keep in lockstep. */
    freertos_apply_core_pin(task, t->name, t->core_plus1);
    return 0;
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
                                      size_t n_tiers);
int32_t nros_board_freertos_run_tiers_ns(const char* locator, uint8_t domain_id,
                                         const char* session_name, const char* node_namespace,
                                         const nros_tier_spec_t* tiers, size_t n_tiers);
int32_t nros_board_freertos_run_tiers_in(const char* locator, uint8_t domain_id,
                                         const char* session_name, const char* node_namespace,
                                         const nros_tier_spec_t* tiers, size_t n_tiers,
                                         void* executor_storage, size_t storage_stride);
int32_t nros_board_freertos_run_tiers_tasks_in(const char* locator, uint8_t domain_id,
                                               const char* session_name, const char* node_namespace,
                                               const nros_tier_spec_t* tiers, size_t n_tiers,
                                               void* executor_storage, size_t storage_stride,
                                               const nros_tier_task_memory_t* task_memory);

int32_t nros_board_freertos_run_tiers(const char* locator, uint8_t domain_id,
                                      const char* session_name, const nros_tier_spec_t* tiers,
                                      size_t n_tiers) {
    /* Issue 1442 — delegates with a NULL namespace, which is "this image
     * declares none" and resolves to the root. Kept as its own symbol because
     * it is the one a pre-1442 generated entry TU calls, and an entry TU
     * outlives the library it was generated against (issue 1050's rule). */
    return nros_board_freertos_run_tiers_ns(locator, domain_id, session_name, NULL, tiers, n_tiers);
}

/*
 * Issue 1442 - the same runner, with the primary session's NAMESPACE.
 *
 * `node_namespace` is the launch-declared namespace the generated entry reads
 * out of `.nros_boot_config` with `nros_boot_config_namespace()`. NULL or empty
 * means the image declares none, which resolves to the ROOT - never to the
 * empty namespace, which would read as "configured to nothing" to every rung
 * above. `nros_boot_config_namespace()` already returns NULL when
 * `NROS_BOOT_SET_NAMESPACE` is clear, so the empty case only arises from a bake
 * that did not resolve.
 *
 * There is no environment rung on FreeRTOS, so the bake IS the answer here; the
 * locator and the domain stay the board's, baked by cmake. Issue 1434 drew the
 * same line one runner over (`nros_board_rtos_run_components_ns`); this is that
 * change on the tiered path.
 *
 * What it names is the SESSION. Each tier's own nodes carry their own
 * `(name, namespace, group)` triples in the spec array (issue 1172) and are
 * untouched.
 */
int32_t nros_board_freertos_run_tiers_ns(const char* locator, uint8_t domain_id,
                                         const char* session_name, const char* node_namespace,
                                         const nros_tier_spec_t* tiers, size_t n_tiers) {
    if (tiers == NULL || n_tiers == 0) {
        return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
    }
    /* Issue 1568 — kept for an entry TU generated before
     * `nros_board_freertos_run_tiers_in` existed. It takes every tier's storage
     * in ONE heap block up front, at the library's own size, and hands it to
     * the runner — so it fails before any session opens rather than part-way
     * down the spawn chain (issue 1551's lesson), and it can no longer be
     * short. */
    const size_t stride = nros_cpp_executor_storage_size();
    void* storage = nros_platform_alloc(n_tiers * stride);
    if (storage == NULL) {
        nros_board_freertos_console_write(
            "nros: freertos run_tiers: the tiers' executor storage does not fit heap_4; "
            "regenerate the entry (nros_board_freertos_run_tiers_in takes it as a static) "
            "or raise NROS_FREERTOS_HEAP_KB\n");
        return -1; /* NROS_CPP_RET_ERROR */
    }
    int32_t rc = nros_board_freertos_run_tiers_in(locator, domain_id, session_name, node_namespace,
                                                  tiers, n_tiers, storage, stride);
    nros_platform_dealloc(storage);
    return rc;
}

/*
 * Issue 1568 — the runner, over caller-supplied executor storage, and the one a
 * generated tiered freertos entry calls (`CAbiRunners::run_tiers_takes_storage`).
 * Zephyr's shape (issue 1551), so the three RTOS runners take storage one way.
 *
 * `executor_storage` holds `n_tiers` blocks of `storage_stride` bytes; block 0
 * is the boot tier's executor and blocks 1.. are the spawned tiers', in chain
 * order. The generated entry passes a file-scope static sized from the
 * per-build `NROS_CPP_EXECUTOR_STORAGE_SIZE` and its own tier count, so the
 * bytes are `.bss` the linker places and `mem-report` names instead of
 * `n_tiers` heap blocks nothing priced. A block the linked library says is too
 * small or misaligned is REFUSED before a byte is written.
 *
 * Ownership stays with the caller: nothing here frees the storage.
 */
int32_t nros_board_freertos_run_tiers_in(const char* locator, uint8_t domain_id,
                                         const char* session_name, const char* node_namespace,
                                         const nros_tier_spec_t* tiers, size_t n_tiers,
                                         void* executor_storage, size_t storage_stride) {
    /* Issue 1598 — kept for an entry TU generated before
     * `nros_board_freertos_run_tiers_tasks_in` existed: no task memory, so its
     * tiers' stacks stay `xTaskCreate` blocks of heap_4. */
    return nros_board_freertos_run_tiers_tasks_in(locator, domain_id, session_name, node_namespace,
                                                  tiers, n_tiers, executor_storage, storage_stride,
                                                  NULL);
}

/* The boot tier's index: the LEAST urgent tier, i.e. the last of a table the
 * emitter sorts highest-priority-first (issue 0636 — see the long note at the
 * call site). An unsorted table falls back to index 0 and says so. */
static size_t freertos_boot_tier_index(const nros_tier_spec_t* tiers, size_t n_tiers) {
    for (size_t i = 1u; i < n_tiers; ++i) {
        if (tiers[i].priority > tiers[i - 1u].priority) {
            nros_board_freertos_console_write(
                "nros: tier table is not sorted highest-priority-first — "
                "boot tier falls back to index 0 (issue 0636)\n");
            return 0u;
        }
    }
    return n_tiers - 1u;
}

/*
 * Issue 1598 — the runner, over caller-supplied executor storage AND
 * caller-supplied task memory, and the one a generated tiered FreeRTOS entry
 * calls (`CAbiRunners::takes_tier_task_memory`).
 *
 * `task_memory` is indexed by TIER (`task_memory[i]` is `tiers[i]`'s), NULL
 * for "none" — then every tier task is `xTaskCreate`d as before. When given,
 * every SPAWNED tier must have a stack and a TCB, and the boot tier (which runs
 * on the caller's task) must have none: the entry and this runner agree on
 * which tier boots, and a disagreement is REFUSED before the session opens
 * rather than discovered as a tier with no stack. Each spawned tier is then
 * `xTaskCreateStatic` over the entry's statics: no heap_4 block, and an image
 * whose stacks do not fit RAM fails at link.
 */
int32_t nros_board_freertos_run_tiers_tasks_in(const char* locator, uint8_t domain_id,
                                               const char* session_name, const char* node_namespace,
                                               const nros_tier_spec_t* tiers, size_t n_tiers,
                                               void* executor_storage, size_t storage_stride,
                                               const nros_tier_task_memory_t* task_memory) {
    if (tiers == NULL || n_tiers == 0 || executor_storage == NULL) {
        return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
    }
    if (nros_cpp_executor_storage_check(executor_storage, storage_stride) != 0) {
        nros_board_freertos_console_write(
            "nros: freertos run_tiers: executor storage refused — the caller sized it from a "
            "different NROS_CPP_EXECUTOR_STORAGE_SIZE than the linked library\n");
        return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
    }
    unsigned char* storage = (unsigned char*)executor_storage;

    /* --- Choose the boot tier: the LEAST urgent one (issue 0636) --- */
    /*
     * This was `&tiers[0]`, which the emitter documents as the MOST urgent
     * tier ("sorted highest-priority-first", nros/main.hpp), and FreeRTOS is a
     * bigger-number-wins kernel. The boot task owns the session and spins
     * forever, so making it outrank the tiers it spawned starves them: FreeRTOS
     * runs the highest-priority READY task and a spin loop does not reliably
     * stop being ready. `17666723d` removed this arrangement from the Rust
     * NuttX/Linux arms and left `freertos` on `tiers[0]`; that is this half.
     *
     * The least urgent tier is the LAST element of a descending table, which
     * also leaves the remaining tiers CONTIGUOUS — required here, because the
     * chain-spawn hands each tier a `rest` SLICE and skipping an interior index
     * would change that protocol. That is why this does not call
     * `nros_platform::boot_tier_index` (the Rust NuttX/Linux arms do): same
     * rule, different mechanics forced by the chain.
     *
     * The ordering is CHECKED rather than assumed. A table that is not
     * non-increasing means the emitter's contract changed, and the failure that
     * would otherwise follow is silent starvation on one platform seconds after
     * boot — the thing issue 0636 spent its history chasing. Fall back to
     * index 0, the behaviour before this change, and say so.
     *
     * Issue 1598 — chosen BEFORE the session opens now, because the entry's
     * task memory is checked against it.
     */
    const size_t boot_idx = freertos_boot_tier_index(tiers, n_tiers);
    if (task_memory != NULL) {
        for (size_t i = 0u; i < n_tiers; ++i) {
            const nros_tier_task_memory_t* m = &task_memory[i];
            const int has = (m->stack != NULL) && (m->tcb != NULL);
            if (i == boot_idx) {
                if (m->stack != NULL || m->tcb != NULL) {
                    nros_board_freertos_console_write(
                        "nros: freertos run_tiers: the entry gave the BOOT tier a task stack "
                        "(it runs on the caller's task) — the entry and this runner disagree "
                        "about which tier boots; refused\n");
                    return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
                }
                continue;
            }
            if (!has || m->stack_bytes < (size_t)configMINIMAL_STACK_SIZE * sizeof(StackType_t)) {
                freertos_announce_spawn_failure(
                    tiers[i].name, "the entry's task memory is missing or below "
                                   "configMINIMAL_STACK_SIZE; refused before the session opens");
                return -3; /* NROS_CPP_RET_INVALID_ARGUMENT */
            }
        }
    }

    /* Belt-and-suspenders network wait (startup.c already brought the network
     * up; this calls the weak no-op on MPS2-AN385 or a board's strong override). */
    nros_board_network_wait();

    /* --- Open the primary (owning) executor on the boot task --- */
    const char* sn = (session_name != NULL && session_name[0] != '\0') ? session_name : "node";
    /* Issue 1442 — NULL and empty are one case: the image declares no
     * namespace, so the next rung down answers (the root). Same test the
     * session name gets one line up. */
    const char* ns = (node_namespace != NULL && node_namespace[0] != '\0') ? node_namespace : NULL;

    /* Block 0 of the caller's storage is the boot tier's executor. */
    void* boot_storage = storage;
    memset(boot_storage, 0, storage_stride);

    int rc = nros_cpp_init(locator, domain_id, sn, ns, boot_storage);
    if (rc != 0) {
        return (int32_t)rc;
    }

    /* Retrieve the session handle so non-boot tiers can borrow it. The handle
     * remains valid as long as boot_storage lives (it lives forever — the boot
     * spin loop never returns). */
    void* session_handle = nros_cpp_executor_session_handle(boot_storage);

    /* --- Run boot tier (tiers[0]) setup on THIS task FIRST --- */
    /* issue #144 — boot setup runs BEFORE any tier spawn (previously this
     * spawned ALL of tiers[1..] BEFORE boot setup, so it had the boot↔tier
     * race too). Entity declares carry an interest handshake; concurrent
     * declares from two threads race it, and the losing publisher's write
     * filter stays closed (every put silently dropped). Running boot's
     * declares first, then CHAINING the remaining spawns (boot spawns tiers[1]
     * only; each tier spawns the next after its own setup returns), makes setup
     * order total (boot, t1, t2, …) so no two declares overlap. Spins still
     * overlap the next tier's setup, which is SAFE — a spin exchanges
     * keepalives/data, not declares. */

    /* The boot tier was chosen above, before the session opened (issue 1598). */
    const nros_tier_spec_t* boot = &tiers[boot_idx];
    /* Everything except `boot`; both arrangements leave a contiguous run. */
    const nros_tier_spec_t* rest_first = (boot_idx == 0u) ? &tiers[1] : &tiers[0];
    const size_t n_rest = n_tiers - 1u;

    /* issue 0636 — announce the boot tier's placement dim too.
     *
     * The boot tier is now the LEAST urgent tier rather than `tiers[0]`, and in
     * `realtime-cpp`'s bringup that is `low`, the one tier declaring `core = 0`.
     * The spawn path announces the dim for the tiers it creates; the boot tier
     * is created by nobody, so without this the declared pin disappears from
     * the console and RFC-0052's fail-loud rule is broken by this file. NULL
     * task = pin the CALLING task, which is what the boot tier runs on. */
    freertos_apply_core_pin(NULL, boot->name, boot->core_plus1);

    /* No derived stack-headroom bound for the boot tier here, unlike Zephyr.
     * It runs on the caller (`app_task`), and FreeRTOS offers no way to ask a
     * task how large its stack IS — `uxTaskGetStackHighWaterMark` reports what
     * is left, not what it was taken against, which is the same asymmetry
     * `check_stack_headroom` cites for why the bound must be declared. Zephyr
     * can answer (CONFIG_MAIN_STACK_SIZE) and so derives there; hardcoding
     * startup.c's 512 KiB here would be a second copy of a number that file
     * owns. A boot tier that wants the rule declares it in `setup()`. */

    /* Gate the boot executor to its tier's callback groups. */
    if (boot->n_groups > 0 && boot->groups != NULL) {
        nros_cpp_executor_set_active_groups(boot_storage, boot->groups, boot->n_groups);
    }

    /* Boot tier node setup. */
    if (boot->setup != NULL) {
        rc = boot->setup(boot_storage);
        if (rc != 0) {
            nros_cpp_fini(boot_storage);
            return (int32_t)rc;
        }
    }

    /* issue 0636 — the boot tier adopts its OWN declared priority, and does it
     * BEFORE the spawn below. Two separate defects meet here:
     *
     *   1. It adopted nothing at all. `nros_freertos_set_current_task_priority`
     *      was called only from the Rust arm, so a C or C++ boot tier kept
     *      whatever priority `app_task` was created at and its declared
     *      `[tiers.*.freertos] priority` did not hold for the tier the boot
     *      task actually runs.
     *   2. The ORDER. `boot` is the LEAST urgent tier, so every tier spawned
     *      below outranks it; FreeRTOS is preemptive, so the first
     *      `xTaskCreate` takes the CPU away at once and anything sitting after
     *      it runs only once that task first blocks. That is exactly how the
     *      NuttX arm lost this marker (8 pass / 4 fail over 12 runs) — the tier
     *      was configured correctly and could not say so.
     *
     * Nothing here needs the children to exist and no other tier task exists
     * yet to be starved by a self-demotion, so this runs while the boot task
     * still owns the CPU. #144 is untouched: boot's DECLARES already ran. */
    (void)freertos_apply_tier_priority(boot->name,
                                       (uint32_t)((boot->priority < 0) ? 0 : boot->priority));

    /* --- Kick off the chained spawn (tiers[1] carrying tiers[2..]) --- */
    /* A boot-side spawn failure is fatal: tear down boot_storage (which the
     * helper never touches) and return error. Downstream tier tasks handle
     * their own spawn failures by logging + continuing to spin. */
    /* The task memory runs parallel to `tiers`, so `rest_first`'s is at the
     * same offset (issue 1598). */
    const nros_tier_task_memory_t* rest_mem =
        (task_memory != NULL) ? task_memory + (size_t)(rest_first - tiers) : NULL;
    int src = freertos_spawn_next_tier(session_handle, domain_id, rest_first, n_rest,
                                       storage + storage_stride, storage_stride, rest_mem);
    if (src != 0) {
        nros_cpp_fini(boot_storage);
        return -1;
    }

    /* Boot tier spin loop — runs forever on embedded firmware. Blocking-read
     * spin_once (period as timeout) so the boot session's zenoh handshake is
     * driven from the spin path (issue #126 defect B); timeout 0 did not
     * connect. Mirrors run_components / the Rust run_tiers_entry. */
    uint32_t period_ms = (uint32_t)(boot->spin_period_us / 1000u);
    if (period_ms < SPIN_PERIOD_FLOOR_MS) {
        period_ms = SPIN_PERIOD_FLOOR_MS;
    }
    /* issue 0636 option 3 — every iteration reaches a scheduling point. The
     * executor's own wait is skipped whenever a wake already fired, so under
     * sustained traffic this loop would otherwise never block, and a task that
     * never blocks never lets a lower-priority tier run.
     *
     * This REPLACES an unconditional `vTaskDelay(1)`, which was the same
     * guarantee hand-rolled for one kernel: it paid a tick on EVERY iteration,
     * including the ones whose spin already blocked, and the three other
     * kernels running the identical loop had nothing. Same rule, one
     * implementation, and cost only on the path that needs it. */
    uint64_t gap_state = 0;
    for (;;) {
        uint64_t iter_start_ns = nros_platform_clock_ns();
        nros_cpp_spin_once(boot_storage, (int32_t)period_ms);
        gap_state = nros_tier_spin_gap_step(gap_state, iter_start_ns, nros_platform_clock_ns(),
                                            (uint32_t)boot->spin_period_us);
    }

    /* Unreachable — satisfies the compiler. */
    nros_cpp_fini(boot_storage);
    return 0;
}
