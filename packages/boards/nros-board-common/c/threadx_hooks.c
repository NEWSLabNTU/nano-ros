/*
 * threadx_hooks.c — generic ThreadX `tx_application_define` stub.
 *
 * Phase 152.2.B.1 — lifted out of both `nros-board-threadx-linux`
 * and `nros-board-threadx-qemu-riscv64`'s `c/app_define.c`. Owns
 * the parts that were byte-for-byte identical:
 *
 *   - 4 MB byte pool create + registration with both
 *     `nros_platform_threadx_set_byte_pool` and the legacy
 *     `zpico_threadx_byte_pool` global.
 *   - RNG seed (`srand` + `nros_platform_threadx_seed_rng`).
 *   - App thread create + `app_thread_entry` that dispatches to
 *     a Rust callback or a C/C++ `app_main`.
 *   - `nros_threadx_set_app_callback` / `nros_threadx_set_app_main`
 *     FFI setters.
 *
 * The board-specific parts live in each overlay's
 * `c/board_threadx_<board>.c`:
 *
 *   - `nros_threadx_set_config(...)` — divergent signatures
 *     (Linux takes `interface_name`; RISC-V does not).
 *   - `nros_board_init_eth()` — weak hook this file calls between
 *     pool create and app-thread create. Linux/NSOS returns 0
 *     without touching the network; RISC-V runs the full
 *     `nx_*_initialize / nx_ip_create / nx_arp_enable / nx_*_enable /
 *     nx_bsd_initialize` sequence.
 *   - `nros_board_log(const char *)` — weak diagnostic; Linux maps
 *     to `printf`, RISC-V maps to `uart_puts`.
 *   - `nros_board_compute_rng_seed(uint32_t *out)` — overlay
 *     derives a seed from its config IP/MAC (no shared cfg
 *     storage here since the Config shapes diverge).
 *
 * Overlays override the stack-size + priority via weak getter
 * functions (`nros_board_app_stack_size`/`_priority`); the weak
 * defaults match the Linux overlay (Phase 247 W3.2).
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "tx_api.h"

/* ---- Sizing constants ---- */
#define BYTE_POOL_SIZE          (4 * 1024 * 1024)

/* ---- Overlay-tunable parameters (weak getters — overlay strong-overrides) ----
 *
 * Phase 247 W3.2 (#50) — these are weak GETTER FUNCTIONS, not weak data.
 *
 * History: they were weak `const uint32_t` globals; gcc folded the weak
 * value (64 KB) at the use site as a compile-time constant, so a sibling
 * TU's strong override never won at link time. Phase 155.A worked around
 * that by dropping `const` (a plain weak load honours the linker-chosen
 * cell) — but a weak *data* symbol relying on link resolution is still the
 * #50-class footgun. A weak *function* cannot be const-folded across a TU
 * boundary (no LTO here; and a weak fn is never inlined past a possible
 * strong override), so the override deterministically wins. Same
 * override-default shape as the `nros_board_*` weak hooks below.
 *
 * The original failure (kept for the record): on RISC-V the board's
 * 512 KB stack override was dropped, `tx_thread_create` got the 64 KB
 * default, Rust `Executor::open`'s frame overran it, sp underflowed past
 * byte_pool storage into `.text` at `CffiSession::open_with_vtable+128`,
 * the next `sd s7, 88(sp)` corrupted that code → illegal-instruction trap. */
__attribute__((weak)) uint32_t nros_board_app_stack_size(void) { return 64 * 1024; }
__attribute__((weak)) uint32_t nros_board_app_priority(void) { return 4; }

/* ---- Weak hooks the overlay implements ---- */
__attribute__((weak)) void nros_board_log(const char *s) { (void)s; }
__attribute__((weak)) int  nros_board_init_eth(void) { return 0; }
__attribute__((weak)) void nros_board_compute_rng_seed(uint32_t *out)
{
    /* Default = constant non-zero seed; overlay overrides with an
     * IP/MAC-derived value so peers do not collide on zenoh-pico
     * session IDs. */
    if (out) { *out = 0xDEADBEEFu; }
}

/* ---- Platform byte pool + RNG registration ---- */
extern void nros_platform_threadx_set_byte_pool(TX_BYTE_POOL *pool);
extern void nros_platform_threadx_seed_rng(uint32_t value);

/* Legacy: zpico-sys C system.c reads this global. */
TX_BYTE_POOL *zpico_threadx_byte_pool;

/* ---- Static objects ---- */
static TX_BYTE_POOL     byte_pool;
static UCHAR            byte_pool_storage[BYTE_POOL_SIZE];
static TX_THREAD        app_thread;

/* ---- Rust callback (set from Rust before tx_kernel_enter) ---- */
static void (*rust_app_entry)(void *) = 0;
static void *rust_app_arg = 0;

/* ---- C/C++ entry point (set by `nros_threadx_set_app_main` or
 * left null for Rust-only builds). Stored as a function pointer
 * instead of `__attribute__((weak)) extern void app_main(void)`
 * to avoid PC-relative relocation overflow on RISC-V when
 * undefined (R_RISCV_PCREL_HI20 ±512KB range vs. weak-undefined
 * resolving to address 0 with .text at 0x80000000). */
static void (*c_app_main)(void) = (void (*)(void))0;
/* Phase 154 fallback — when `nros_threadx_set_app_main` was
 * never called (Linux-host C / C++ examples just define
 * `app_main()` and rely on link-time symbol resolution),
 * pick up the weak symbol below.
 *
 * Gated on non-RISC-V because `R_RISCV_PCREL_HI20` has a
 * ±512 KB range and a weak-undefined `app_main` resolves to
 * address 0, which is far outside that window from `.text`
 * at `0x80000000` — the link fails with "relocation
 * R_RISCV_PCREL_HI20 out of range". RISC-V consumers must
 * use the `nros_threadx_set_app_main` FFI setter explicitly
 * (the Rust-side `nros_board_threadx::run<B>` does so via
 * `nros_threadx_set_app_callback`; C / C++ ports on RISC-V
 * need to wire it manually). */
#if !defined(__riscv)
extern void app_main(void) __attribute__((weak));
#endif

void nros_threadx_set_app_callback(void (*entry)(void *), void *arg)
{
    rust_app_entry = entry;
    rust_app_arg = arg;
}

void nros_threadx_set_app_main(void (*entry)(void))
{
    c_app_main = entry;
}

/* ---- App thread entry: invokes Rust callback or C/C++ app_main ---- */
static void app_thread_entry(ULONG input)
{
    (void)input;

    if (rust_app_entry) {
        nros_board_log("[app_thread] Calling Rust entry...\n");
        rust_app_entry(rust_app_arg);
        nros_board_log("[app_thread] Rust entry returned\n");
        return;
    }
    if (c_app_main) {
        nros_board_log("[app_thread] Calling c_app_main (FFI)...\n");
        c_app_main();
        nros_board_log("[app_thread] c_app_main returned\n");
        return;
    }
#if !defined(__riscv)
    if (app_main) {
        nros_board_log("[app_thread] Calling app_main (weak)...\n");
        app_main();
        nros_board_log("[app_thread] app_main returned\n");
        return;
    }
#endif
    nros_board_log("ERROR: no app entry point (set Rust callback or define app_main)\n");
}

/* ---- ThreadX tx_application_define (called by tx_kernel_enter) ---- */
void tx_application_define(void *first_unused_memory)
{
    UINT status;
    UCHAR *pointer;
    uint32_t stack_size;
    uint32_t priority;

    (void)first_unused_memory;

    nros_board_log("[app_define] Creating byte pool...\n");
    status = tx_byte_pool_create(&byte_pool, "nros_byte_pool",
                                  byte_pool_storage, BYTE_POOL_SIZE);
    if (status != TX_SUCCESS) {
        nros_board_log("ERROR: byte pool create failed\n");
        return;
    }

    zpico_threadx_byte_pool = &byte_pool;
    nros_platform_threadx_set_byte_pool(&byte_pool);

    /* Seed RNG from overlay-computed value (IP/MAC-derived). Used
     * by zenoh-pico session IDs — must vary per-instance so two
     * simulations do not collide. */
    {
        uint32_t seed = 0;
        nros_board_compute_rng_seed(&seed);
        if (seed == 0) { seed = 1; }
        srand(seed);
        nros_platform_threadx_seed_rng(seed);
    }

    /* Per-board NetX / driver init. Linux/NSOS overlay no-ops;
     * RISC-V overlay runs the full NetX-Duo + virtio-net setup. */
    nros_board_log("[app_define] Running board network init...\n");
    if (nros_board_init_eth() != 0) {
        nros_board_log("ERROR: nros_board_init_eth failed\n");
        return;
    }

    /* Create application thread. Resolve the overlay-tunable size/priority
     * via the weak getters once (the overlay strong-overrides the fns). */
    stack_size = nros_board_app_stack_size();
    priority   = nros_board_app_priority();
    nros_board_log("[app_define] Creating app thread...\n");
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               stack_size, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        nros_board_log("ERROR: app thread stack alloc failed\n");
        return;
    }

    status = tx_thread_create(&app_thread, "nros_app",
                               app_thread_entry, 0,
                               pointer, stack_size,
                               priority,
                               priority,
                               TX_NO_TIME_SLICE, TX_AUTO_START);
    if (status != TX_SUCCESS) {
        nros_board_log("ERROR: app thread create failed\n");
        return;
    }
    nros_board_log("[app_define] App thread created, returning to kernel...\n");
}

/* ---- Phase 297 W2 — multi-tier thread-creation shim (RFC-0053) --------------
 *
 * `nros_threadx_create_task` is the SINGLE thread-creation backend the Rust
 * `run_tiers` (W4) and any C/C++ entry call to spawn a per-tier thread —
 * mirroring the FreeRTOS `nros_freertos_create_task` shape, not a per-language
 * reimplementation (the common-backend principle).
 *
 * ThreadX has no default heap: `tx_thread_create` needs a caller-provided
 * STACK. W3 bakes one aligned `static` stack per tier and passes `(ptr, len)`
 * here (RFC-0053 Option A) — the RAM-heavy part stays exact, no pool. The
 * TX_THREAD control blocks are small and fixed-size, so this file owns a
 * bounded static array of them rather than exposing the port-specific
 * `sizeof(TX_THREAD)` to the Rust side.
 *
 * `entry` is ThreadX-native `void(*)(ULONG)`; `arg` (the Rust spawn context,
 * cast to `usize`) rides in as the ULONG thread input — no trampoline. A
 * `preempt_threshold` of `-1` means "= priority" (no preemption-threshold
 * effect); a value `0..TX_MAX_PRIORITIES-1` is ThreadX's native
 * `non_preempt_scope` realization (RFC-0052) — a thread with a threshold
 * higher-priority (lower-number) than its own priority cannot be preempted by
 * threads at or below that threshold. `tx_thread_create` applies the threshold
 * directly (no separate `tx_thread_preemption_change` needed at creation).
 *
 * Returns 0 on success, -1 on failure (cap exceeded, null/empty stack, or
 * `tx_thread_create` error). */
#define NROS_TX_MAX_TASKS 8
static TX_THREAD nros_tx_task_blocks[NROS_TX_MAX_TASKS];
static int nros_tx_task_count = 0;

int nros_threadx_create_task(
    const char *name,
    void (*entry)(ULONG),
    ULONG arg,
    void *stack_ptr,
    unsigned long stack_len,
    unsigned int priority,
    int preempt_threshold)
{
    UINT status;
    UINT pt;
    TX_THREAD *thread;

    if (entry == 0) {
        nros_board_log("ERROR: nros_threadx_create_task: null entry\n");
        return -1;
    }
    if (stack_ptr == 0 || stack_len == 0) {
        nros_board_log("ERROR: nros_threadx_create_task: null/empty stack\n");
        return -1;
    }
    if (nros_tx_task_count >= NROS_TX_MAX_TASKS) {
        nros_board_log("ERROR: nros_threadx_create_task: NROS_TX_MAX_TASKS exceeded\n");
        return -1;
    }

    thread = &nros_tx_task_blocks[nros_tx_task_count];
    /* -1 sentinel → threshold == priority (ThreadX's "no threshold" state). */
    pt = (preempt_threshold < 0) ? priority : (UINT)preempt_threshold;

    status = tx_thread_create(thread, (CHAR *)(name ? name : "nros_tier"),
                               entry, arg,
                               stack_ptr, (ULONG)stack_len,
                               priority, pt,
                               TX_NO_TIME_SLICE, TX_AUTO_START);
    if (status != TX_SUCCESS) {
        nros_board_log("ERROR: nros_threadx_create_task: tx_thread_create failed\n");
        return -1;
    }

    nros_tx_task_count++;
    return 0;
}
