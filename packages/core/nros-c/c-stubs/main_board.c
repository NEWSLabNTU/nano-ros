/**
 * Phase 219.C — C-FFI Board adapter for Entry pkgs.
 *
 * Defines `nros_board_native_run(entry)` — the C-language counterpart
 * of the C++ `nros::board::NativeBoard::run(lambda)` adapter in
 * `<nros/main.hpp>`. The Entry pkg's generated `main.c` (emitted by
 * `nros codegen entry --lang c`) calls into this fn with a register
 * driver that invokes every Node pkg's mangled register fn in launch
 * order.
 *
 * Lifecycle:
 *   1. `nros_support_init(&support)` — open the middleware session.
 *   2. Build a stub `nros_node_context_t` whose ops are no-ops.
 *      (The real Native NodeContext runtime — the bit that turns
 *      recorded entities into running pubs/subs — sits below the
 *      Phase 219 "pure orchestration" scope; see phase doc §7.)
 *   3. `entry(context)` — runs once before the spin loop.
 *   4. Sleep-spin until the process is signalled. No executor is
 *      attached today — that's the runtime gap acknowledged above.
 *   5. `nros_support_fini(&support)`.
 *
 * Phase 212.L.2 keeps Entry pkgs `native`-only at the cmake surface.
 */

/* node_pkg.h pulls in only stdint + visibility — none of the
 * `nros_generated.h` opaque-size macros that need the per-build
 * `nros_config_generated.h` header. main.h is similarly light.
 * `nros_support_t` is forward-declared below (its storage size is
 * irrelevant here — we hold a value via the by-value
 * `*_get_zero_initialized` call and only pass pointers around). */
#include "nros/node_pkg.h"

#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>

/* Phase 219.C stub-grade adapter — no `nros_support_init` / `_fini`
 * ritual today. Calling those needs the full `nros_support_t` layout
 * (the opaque-size macros sit in the per-build `nros_config_generated.h`,
 * which the surrounding cc-rs invocation deliberately does NOT include
 * for this stub TU). The lifecycle expansion lands when the Native
 * NodeContext runtime arrives. */

static volatile sig_atomic_t __nros_board_running = 1;

static void __nros_board_signal_handler(int signum) {
    (void)signum;
    __nros_board_running = 0;
}

/* No-op NodeContext ops — placeholders until the Native NodeContext
 * runtime lands. Each fn returns NROS_RET_OK so the user's per-Node
 * register fn finishes without errors. */
static nros_ret_t __nros_board_noop_create_node(void* user_data, const char* stable_id,
                                                const nros_node_pkg_options_t* options,
                                                nros_declared_node_t* out_node) {
    (void)user_data;
    (void)stable_id;
    (void)options;
    if (out_node) {
        out_node->stable_id = stable_id;
        out_node->runtime_handle = NULL;
        out_node->context = NULL;
    }
    return NROS_RET_OK;
}

static nros_ret_t __nros_board_noop_create_entity(void* user_data, const void* desc) {
    (void)user_data;
    (void)desc;
    return NROS_RET_OK;
}

static nros_ret_t __nros_board_noop_record_callback_effect(void* user_data, const char* cb_id,
                                                           nros_node_callback_effect_kind_t kind,
                                                           const char* entity_id) {
    (void)user_data;
    (void)cb_id;
    (void)kind;
    (void)entity_id;
    return NROS_RET_OK;
}

int nros_board_native_run(nros_node_register_fn entry) {
    if (!entry) {
        return (int)NROS_RET_INVALID_ARGUMENT;
    }

    static const nros_node_context_ops_t ops = {
        /* create_node            */ &__nros_board_noop_create_node,
        /* create_entity          */ &__nros_board_noop_create_entity,
        /* record_callback_effect */ &__nros_board_noop_record_callback_effect,
    };
    nros_node_context_t context;
    context.user_data = NULL;
    context.ops = &ops;

    nros_ret_t rc = entry(&context);
    if (rc != NROS_RET_OK) {
        return (int)rc;
    }

    /* Install signal handlers so Ctrl+C / SIGTERM end the loop. */
    signal(SIGINT, &__nros_board_signal_handler);
    signal(SIGTERM, &__nros_board_signal_handler);

    /* Sleep-spin. The real executor-driven spin lands once the Native
     * NodeContext runtime is wired (see phase doc §7). 100 ms ticks =
     * acceptable Ctrl+C latency for a desktop runner. */
    struct timespec tick = {.tv_sec = 0, .tv_nsec = 100 * 1000 * 1000};
    while (__nros_board_running) {
        nanosleep(&tick, NULL);
    }

    return 0;
}
