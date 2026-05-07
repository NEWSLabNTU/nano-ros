/* Phase 115.K.2.4 — runtime custom-transport bridge.
 *
 * C-side equivalent of `nros-rmw-xrce::init_transport_from_custom_ops`
 * in `packages/xrce/nros-rmw-xrce/src/lib.rs:875`. Stores a vtable in
 * file-scope storage (single registration per process — same shape
 * the Rust impl uses) and exposes four trampolines that XRCE's
 * `uxrCustomTransport` invokes on each open / close / write / read.
 * The trampolines fan back out to the user's callbacks.
 *
 * Two registration paths:
 *
 *  1. `nros_rmw_xrce_set_custom_transport_ops(ops, framing)` —
 *     direct copy from a caller-supplied struct. Pure-C clients use
 *     this to wire e.g. a USB-CDC bridge without round-tripping
 *     through the runtime's `set_custom_transport` slot.
 *
 *  2. `nros_rmw_xrce_init_custom_transport(framing)` —
 *     drains whatever the runtime registered via
 *     `nros_set_custom_transport`. Requires
 *     `nros_rmw_take_custom_transport()` from `nros-rmw-cffi`; that
 *     symbol is not yet exported, so this path returns
 *     `NROS_RMW_RET_UNSUPPORTED`. See KNOWN-LIMITATIONS.md.
 */

#include "nros_rmw_xrce.h"

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <uxr/client/profile/transport/custom/custom_transport.h>

/* File-scope slot — single registration per process. The Rust impl
 * uses a `SharedCell` here for the same reason: each trampoline
 * call would otherwise need to re-acquire the runtime mutex. */
typedef struct xrce_custom_ops_storage {
    nros_rmw_xrce_transport_ops_t ops;
    bool framing;
    bool armed;
} xrce_custom_ops_storage;

static xrce_custom_ops_storage g_xrce_custom_ops;

int xrce_custom_transport_is_armed(void) {
    return g_xrce_custom_ops.armed ? 1 : 0;
}

/* ---- Trampolines (C-side equivalents of
 *      `xrce_custom_open_trampoline` etc. in lib.rs:811-856) ---- */

static bool xrce_custom_open_trampoline(struct uxrCustomTransport *t) {
    (void)t;
    if (!g_xrce_custom_ops.armed || g_xrce_custom_ops.ops.open == NULL) {
        return true; /* No open callback registered → success no-op. */
    }
    int32_t ret = g_xrce_custom_ops.ops.open(g_xrce_custom_ops.ops.user_data, NULL);
    return ret == 0;
}

static bool xrce_custom_close_trampoline(struct uxrCustomTransport *t) {
    (void)t;
    if (g_xrce_custom_ops.armed && g_xrce_custom_ops.ops.close != NULL) {
        g_xrce_custom_ops.ops.close(g_xrce_custom_ops.ops.user_data);
    }
    return true;
}

static size_t xrce_custom_write_trampoline(struct uxrCustomTransport *t,
                                           const uint8_t *buf, size_t len,
                                           uint8_t *err) {
    (void)t;
    (void)err;
    if (!g_xrce_custom_ops.armed || g_xrce_custom_ops.ops.write == NULL) {
        return 0;
    }
    int32_t ret = g_xrce_custom_ops.ops.write(
        g_xrce_custom_ops.ops.user_data, buf, len);
    /* The Rust impl returns `len` on success (caller tracked 0/!=0
     * as success/failure); mirror that exactly. */
    return ret == 0 ? len : 0;
}

static size_t xrce_custom_read_trampoline(struct uxrCustomTransport *t,
                                          uint8_t *buf, size_t len,
                                          int timeout, uint8_t *err) {
    (void)t;
    (void)err;
    if (!g_xrce_custom_ops.armed || g_xrce_custom_ops.ops.read == NULL) {
        return 0;
    }
    uint32_t timeout_ms = timeout < 0 ? 0u : (uint32_t)timeout;
    int32_t ret = g_xrce_custom_ops.ops.read(
        g_xrce_custom_ops.ops.user_data, buf, len, timeout_ms);
    return ret < 0 ? 0u : (size_t)ret;
}

/* ---- Registration entry points ---- */

nros_rmw_ret_t nros_rmw_xrce_set_custom_transport_ops(
    const nros_rmw_xrce_transport_ops_t *ops, int framing) {
    if (ops == NULL || ops->open == NULL || ops->close == NULL
        || ops->write == NULL || ops->read == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    g_xrce_custom_ops.ops     = *ops;
    g_xrce_custom_ops.framing = framing != 0;
    g_xrce_custom_ops.armed   = true;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t nros_rmw_xrce_init_custom_transport(int framing) {
    (void)framing;
    /* Phase 115.K.2.4 gap: `nros_rmw_take_custom_transport()` is not
     * yet exported by nros-rmw-cffi. Until that lands, callers must
     * use `nros_rmw_xrce_set_custom_transport_ops` directly. */
    return NROS_RMW_RET_UNSUPPORTED;
}

/* ---- Install on a session (called from session.c on `custom://`) -- */

nros_rmw_ret_t xrce_custom_transport_install(xrce_session_state_t *st,
                                             bool framing) {
    if (st == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (!g_xrce_custom_ops.armed) {
        /* Caller selected `custom://` without first registering a
         * transport — actionable error, not a stub. */
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    /* Allow the caller to override framing per-session, but default
     * to whatever was registered on the slot. */
    bool effective_framing = framing || g_xrce_custom_ops.framing;

    uxr_set_custom_transport_callbacks(
        &st->custom, effective_framing,
        xrce_custom_open_trampoline,
        xrce_custom_close_trampoline,
        xrce_custom_write_trampoline,
        xrce_custom_read_trampoline);

    if (!uxr_init_custom_transport(&st->custom, NULL)) {
        return NROS_RMW_RET_ERROR;
    }
    return NROS_RMW_RET_OK;
}
