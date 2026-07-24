#ifndef NROS_RMW_TRANSPORT_H
#define NROS_RMW_TRANSPORT_H

#include <stddef.h>
#include <stdint.h>

#include "nros/rmw_ret.h"

/**
 * @file rmw_transport.h
 * @brief Runtime-pluggable custom transport vtable (Phase 115.A).
 *
 * Lets a consumer plug a custom byte-pipe (USB-CDC, BLE, RS-485,
 * semihosting bridge, ring-buffer loopback, etc.) at runtime — no
 * board-crate, Cargo-feature, or rebuild required. The active RMW
 * backend (zenoh-pico, XRCE, dust-DDS, …) consumes this vtable as
 * the read/write surface for every wire frame.
 *
 * Mirrors micro-ROS's
 * `rmw_uros_set_custom_transport(framing, params, open, close,
 * write, read)` and is the C-side view of the Rust-side
 * `nros_rmw::NrosTransportOps` (single `#[repr(C)]` layout; no
 * parallel definitions to drift).
 *
 * ## Threading contract (v1)
 *
 *  - `read` and `write` may NOT be invoked concurrently from
 *    different threads. The active backend serialises them through
 *    its drive-io / spin-once path.
 *  - Callbacks may NOT be invoked from interrupt context. Wrap
 *    ISR-driven hardware in a queue + `read` poller.
 *  - `user_data` is opaque to the runtime — its Send/Sync discipline
 *    is the caller's responsibility. The four fn pointers are
 *    always thread-safe by construction.
 *
 * ## Versioning
 *
 *  - `abi_version` MUST be set to `NROS_TRANSPORT_OPS_ABI_VERSION_V1`
 *    (1). Mismatched values are rejected at registration time with
 *    `NROS_RMW_RET_INCOMPATIBLE_ABI` and the previously-installed
 *    transport (if any) stays untouched.
 *  - **Major** bump (V1 → V2): existing fields removed or
 *    reordered. Old consumers fail cleanly via the version check.
 *  - **Minor** bump: struct gains an appended fn pointer. Version
 *    stays the same; new consumers detect the new fn via the size
 *    of the trailing `_reserved` region. Today there is none — V1
 *    is the inaugural version.
 *
 * ## See also
 *
 *  - Rust API: `nros_rmw::custom_transport::{NrosTransportOps,
 *    set_custom_transport, peek_custom_transport,
 *    take_custom_transport}` (the canonical Rust-side surface;
 *    this header mirrors its `#[repr(C)]` layout exactly).
 *  - Porting guide: `book/src/porting/custom-transport.md` — when
 *    to use, full Rust/C/C++ examples, framing per backend.
 */

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Current ABI version of `nros_transport_ops_t`. Consumers MUST
 * fill this in before passing the struct to
 * `nros_set_custom_transport`.
 */
#define NROS_TRANSPORT_OPS_ABI_VERSION_V1 ((uint32_t)1)

/**
 * Runtime-pluggable custom transport. The runtime never
 * dereferences `user_data`; it's the caller's per-transport
 * context, threaded back into every callback's first argument.
 *
 * `#[repr(C)]` mirror of the Rust-side `NrosTransportOps`. Same
 * layout, same threading contract, same return codes.
 */
typedef struct nros_transport_ops_s {
    /**
     * ABI version. MUST equal `NROS_TRANSPORT_OPS_ABI_VERSION_V1`.
     * Any other value is rejected at registration time with
     * `NROS_RMW_RET_INCOMPATIBLE_ABI`.
     */
    uint32_t abi_version;

    /** Reserved padding for alignment stability across appends.
     * Set to zero. */
    uint32_t _reserved;

    /** Opaque caller context, threaded back into every callback.
     * Lifetime: must outlive the transport's active period
     * (i.e. until `close` returns). */
    void *user_data;

    /**
     * Open the underlying medium.
     *
     * @param user_data Caller-supplied context.
     * @param params Opaque per-transport metadata (e.g. UART baud
     *               rate, USB-CDC endpoint id). May be NULL.
     * @retval NROS_RMW_RET_OK on success.
     * @retval <0 on failure (any `nros_rmw_ret_t` error code).
     */
    int32_t (*open)(void *user_data, const void *params);

    /**
     * Tear the transport down. After `close` returns, the runtime
     * will not invoke `read` or `write` on this transport unless
     * `nros_set_custom_transport` is called again.
     */
    void (*close)(void *user_data);

    /**
     * Send `len` bytes from `buf`. Must NOT block beyond a brief
     * hardware retry; long blocking should surface as
     * `NROS_RMW_RET_TIMEOUT`.
     *
     * @retval NROS_RMW_RET_OK on success.
     * @retval <0 on failure (any `nros_rmw_ret_t` error code).
     */
    int32_t (*write)(void *user_data, const uint8_t *buf, size_t len);

    /**
     * Receive up to `len` bytes into `buf` within `timeout_ms`.
     *
     * @retval >=0 number of bytes read (may be less than `len`).
     * @retval <0 on error / timeout (any `nros_rmw_ret_t` error
     *            code).
     */
    int32_t (*read)(void *user_data, uint8_t *buf, size_t len,
                    uint32_t timeout_ms);
} nros_transport_ops_t;

/**
 * Install a custom transport for subsequent session opens.
 *
 * The struct's contents are copied internally; the caller may
 * stack-allocate. To clear the slot, pass NULL.
 *
 * The fn pointer is exported from the nano-ros C staticlib
 * (`packages/core/nros-c/`), where it forwards to the
 * `nros-rmw-cffi` registry. C++ consumers should include
 * `<nros/transport.hpp>` (from `nros-cpp`) which calls this
 * function under the hood.
 *
 * @retval NROS_RMW_RET_OK on success (transport installed or
 *         cleared).
 * @retval NROS_RMW_RET_INCOMPATIBLE_ABI when `ops` is non-NULL but
 *         `ops->abi_version` does not match
 *         `NROS_TRANSPORT_OPS_ABI_VERSION_V1`. The previously
 *         installed transport (if any) is left untouched.
 */
nros_rmw_ret_t nros_rmw_cffi_set_custom_transport(const nros_transport_ops_t *ops);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // NROS_RMW_TRANSPORT_H
