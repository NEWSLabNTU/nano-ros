#ifndef NROS_RMW_XRCE_H
#define NROS_RMW_XRCE_H

#include "nros/rmw_ret.h"

#include <stddef.h>
#include <stdint.h>

/**
 * @file nros_rmw_xrce.h
 * @brief Public C entry point for the micro-XRCE-DDS-Client nano-ros RMW backend.
 *
 * The backend is a static C library implementing `nros_rmw_vtable_t`
 * (see `<nros/rmw_vtable.h>`). At runtime, the host application calls
 * `nros_rmw_xrce_register()` once, before any session creation,
 * to install the backend's vtable via `nros_rmw_cffi_register_named()`.
 *
 * Typical wiring (driven by `nros-c`'s CMake when
 * `-DNROS_C_RMW=xrce` is set):
 *
 *     nros_rmw_xrce_register();
 *     rclc_support_init(...);
 *     // ... create publishers / subscribers ...
 *
 * Phase 115.K.2.0–115.K.2.3 fill in session / pub / sub / service
 * paths against `uxr_*`. Phase 115.K.2.4 adds the custom-transport
 * bridge below for boards that want to plug a non-UDP transport
 * (USB-CDC, BLE, RS-485) at runtime.
 */

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Register the micro-XRCE-DDS-Client backend with the nano-ros RMW
 * runtime.
 *
 * Idempotent: subsequent calls re-register the same vtable; the
 * runtime treats this as a no-op.
 *
 * @retval NROS_RMW_RET_OK    on success.
 * @retval NROS_RMW_RET_ERROR if the runtime rejected the vtable.
 */
rmw_ret_t nros_rmw_xrce_register(void);

/**
 * Phase 115.K.2.4 — runtime transport vtable for the XRCE-DDS
 * custom-transport bridge.
 *
 * Same shape as `nros_transport_ops_t` in `<rclc/detail/nros_generated.h>`
 * and `nros_rmw::NrosTransportOps` (Phase 115.A). The two surfaces
 * are deliberately layout-identical so a single struct can be
 * used at every layer; defining a copy here avoids pulling in
 * `nros-c`'s generated header (which would create a circular
 * dependency with the C API).
 *
 * Field semantics:
 *
 * - `user_data`: opaque caller context, threaded back into every
 *   callback as the first argument. Must outlive the transport's
 *   active period (i.e. until `close` returns).
 * - `open`: open the underlying medium. `params` is opaque
 *   per-transport metadata; may be NULL. Returns 0 on success,
 *   negative `rmw_ret_t` on failure.
 * - `close`: tear the transport down. Complement of `open`.
 * - `write`: send `len` bytes from `buf`. Returns 0 on success,
 *   negative `rmw_ret_t` on failure.
 * - `read`: receive up to `len` bytes within `timeout_ms`. Returns
 *   the non-negative byte count on success, negative `rmw_ret_t`
 *   on error / timeout.
 *
 * Threading contract (matches `<nros/transport.h>`):
 *  - `read` and `write` are NEVER invoked concurrently from
 *    different threads.
 *  - Callbacks must NOT be invoked from interrupt context.
 *  - `user_data` is opaque to nros — its `Send` / `Sync` discipline
 *    is the caller's responsibility.
 */
typedef struct nros_rmw_xrce_transport_ops_t {
    void *user_data;
    int32_t (*open)(void *user_data, const void *params);
    void    (*close)(void *user_data);
    int32_t (*write)(void *user_data, const uint8_t *buf, size_t len);
    int32_t (*read) (void *user_data, uint8_t *buf, size_t len,
                     uint32_t timeout_ms);
} nros_rmw_xrce_transport_ops_t;

/**
 * Phase 115.K.2.4 — install a custom-transport vtable for the XRCE
 * backend's `custom://` locator path. After this call returns OK,
 * `xrce_session_create` invoked with a locator starting `custom://`
 * routes through `uxr_set_custom_transport_callbacks` +
 * `uxr_init_custom_transport` instead of UDP.
 *
 * `framing` is `true` for byte-stream transports (serial / UART)
 * that need HDLC framing, `false` for packet-oriented transports.
 *
 * The struct is copied into backend-local storage; the caller may
 * free or mutate it after this call returns. The fn-pointer
 * targets, however, must remain valid until the session closes.
 *
 * Idempotent — calling twice replaces the previous registration.
 *
 * @retval NROS_RMW_RET_OK on success.
 * @retval NROS_RMW_RET_INVALID_ARGUMENT if `ops` is NULL or any of
 *         the four fn pointers is NULL.
 */
rmw_ret_t nros_rmw_xrce_set_custom_transport_ops(
    const nros_rmw_xrce_transport_ops_t *ops, int framing);

/**
 * phase-521 W5 (RFC-0113 D4) — the custom transport's receive notification.
 *
 * A transport that can say when bytes have arrived (a UART's RX interrupt, a
 * USB-CDC receive callback) supplies `set_rx_notify`. The backend calls it with
 * the executor's wake callback once a session opens on the custom transport,
 * and with `cb == NULL` to take it off. While installed, the transport calls
 * `cb(ctx)` every time it has received data the next `read` would return.
 * The session then states `Notify` readiness, so the executor parks until an
 * arrival or its next deadline instead of blocking in `read`.
 *
 * `cb` may be called from the transport's receive path, interrupt context
 * included where the platform's wake signal is interrupt-safe (it is on every
 * port that defines `nros_platform_wake_signal_from_isr` the same as
 * `nros_platform_wake_signal`). After a call with `cb == NULL` returns, the
 * transport must not call the previous `cb` again.
 *
 * `set_rx_notify` returns 0 on success. NULL clears the registration. A
 * custom transport with no `set_rx_notify` states no readiness form, and the
 * runtime refuses such a session at open (RFC-0113 D5).
 *
 * @retval NROS_RMW_RET_OK always.
 */
typedef int32_t (*nros_rmw_xrce_transport_set_rx_notify_fn)(void *user_data,
                                                           void (*cb)(void *ctx),
                                                           void *ctx);
rmw_ret_t nros_rmw_xrce_set_custom_transport_rx_notify(
    nros_rmw_xrce_transport_set_rx_notify_fn set_rx_notify);

/**
 * Phase 115.K.2.4 — drain whatever
 * `nros_rmw::custom_transport::take_custom_transport()` produces and
 * install it via `nros_rmw_xrce_set_custom_transport_ops`. Convenience
 * for callers that registered via `rclc_set_custom_transport()`
 * (the `nros-c` C surface) and want the active backend to consume
 * the slot at backend-init time.
 *
 * Requires the runtime to expose a C-callable
 * ~~`nros_rmw_take_custom_transport()`~~ symbol; see docs/reference/xrce-known-limitations.md
 * for the gap. v1 of K.2.4 returns `NROS_RMW_RET_UNSUPPORTED` until
 * the runtime side lands; callers can route around via
 * `nros_rmw_xrce_set_custom_transport_ops` directly.
 *
 * @param framing same semantics as in `nros_rmw_xrce_set_custom_transport_ops`.
 *
 * @retval NROS_RMW_RET_OK          on success.
 * @retval NROS_RMW_RET_NO_DATA     no transport currently registered.
 * @retval NROS_RMW_RET_UNSUPPORTED runtime drain symbol not exported (K.2.4 gap).
 */
rmw_ret_t nros_rmw_xrce_init_custom_transport(int framing);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_XRCE_H */
