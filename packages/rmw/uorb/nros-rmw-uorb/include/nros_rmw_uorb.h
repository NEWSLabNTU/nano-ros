#ifndef NROS_RMW_UORB_H
#define NROS_RMW_UORB_H

#include "nros/rmw_ret.h"

/**
 * @file nros_rmw_uorb.h
 * @brief Public C entry point for the uORB nano-ros RMW backend.
 *
 * Phase 115.K.4 — C++ port of `nros-rmw-uorb` (the previous Rust
 * implementation, now retired). The backend is a static C++ library
 * implementing `nros_rmw_vtable_t` (see `<nros/rmw_vtable.h>`).
 *
 * At runtime, the host application (typically a PX4 module's
 * `task_main`) calls `nros_rmw_uorb_register()` once before any
 * session creation:
 *
 *     nros_rmw_uorb_register();
 *     nros::init(nullptr, 0, "my_module");
 *     // ... create publishers / subscribers ...
 *
 * Phase 115.K.4.0 (this commit) — vtable scaffold; every entry
 * returns `NROS_RMW_RET_UNSUPPORTED` until K.4.1 (session
 * lifecycle), K.4.2 (pub/sub data plane), and K.4.3 (type-hash
 * correlation) fill them in.
 */

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Register the uORB backend with the nano-ros RMW runtime.
 *
 * Idempotent: subsequent calls re-register the same vtable; the
 * runtime treats this as a no-op.
 *
 * @retval NROS_RMW_RET_OK    on success.
 * @retval NROS_RMW_RET_ERROR if the runtime rejected the vtable.
 */
nros_rmw_ret_t nros_rmw_uorb_register(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_UORB_H */
