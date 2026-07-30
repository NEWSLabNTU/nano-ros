#ifndef NROS_RMW_CYCLONEDDS_H
#define NROS_RMW_CYCLONEDDS_H

#include "nros/rmw_ret.h"

/**
 * @file nros_rmw_cyclonedds.h
 * @brief Public C entry point for the Cyclone DDS nano-ros RMW backend.
 *
 * The backend is a static C++ library implementing `nros_rmw_vtable_t`
 * (see `<nros/rmw_vtable.h>`). At runtime, the host application calls
 * `nros_rmw_cyclonedds_register()` once, before any session creation,
 * to install the backend's vtable via `nros_rmw_cffi_register()`.
 *
 * Typical wiring (driven by `nros-cpp`'s CMake when
 * `-DNROS_CPP_RMW=cyclonedds` is set):
 *
 *     nros_rmw_cyclonedds_register();
 *     nros::init(nullptr, 0, "my_node");
 *     // ... create publishers / subscribers ...
 *
 * Phase 117.3 — vtable scaffold; every entry returns
 * `NROS_RMW_RET_UNSUPPORTED` until 117.4 / 117.6 / 117.7 fill in
 * session, pub/sub, and service paths.
 */

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Register the Cyclone DDS backend with the nano-ros RMW runtime.
 *
 * Idempotent: subsequent calls re-register the same vtable; the
 * runtime treats this as a no-op.
 *
 * @retval NROS_RMW_RET_OK    on success.
 * @retval NROS_RMW_RET_ERROR if the runtime rejected the vtable.
 */
nros_rmw_ret_t nros_rmw_cyclonedds_register(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_CYCLONEDDS_H */
