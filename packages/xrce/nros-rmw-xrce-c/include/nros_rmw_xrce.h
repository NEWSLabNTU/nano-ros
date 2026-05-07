#ifndef NROS_RMW_XRCE_H
#define NROS_RMW_XRCE_H

#include "nros/rmw_ret.h"

/**
 * @file nros_rmw_xrce.h
 * @brief Public C entry point for the micro-XRCE-DDS-Client nano-ros RMW backend.
 *
 * The backend is a static C library implementing `nros_rmw_vtable_t`
 * (see `<nros/rmw_vtable.h>`). At runtime, the host application calls
 * `nros_rmw_xrce_register()` once, before any session creation,
 * to install the backend's vtable via `nros_rmw_cffi_register()`.
 *
 * Typical wiring (driven by `nros-c`'s CMake when
 * `-DNROS_C_RMW=xrce` is set):
 *
 *     nros_rmw_xrce_register();
 *     nros_support_init(...);
 *     // ... create publishers / subscribers ...
 *
 * Phase 115.K.2 — vtable scaffold; every entry returns
 * `NROS_RMW_RET_UNSUPPORTED` until the actual `uxr_*` calls land in
 * 115.K.2.1+.
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
nros_rmw_ret_t nros_rmw_xrce_register(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_XRCE_H */
