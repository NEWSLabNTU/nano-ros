/**
 * @file rclc.h
 * @brief Umbrella header for the nano-ros C API, at upstream's path.
 *
 * Including this single header provides the whole C API: rclc's and rcl's
 * names, the nano-ros-only parts (`RCLC_C_COMPONENT`, `RCLC_CHECK_RET`,
 * `RCLC_LOG_*`), and the rcl spellings of the return codes. Every other
 * upstream-path header (`<rclc/node.h>`, `<rcl/rcl.h>`, ...) includes it.
 * Headers under `rclc/detail/` are internal (phase-501).
 */

#ifndef RCLC_H
#define RCLC_H

#include "rclc/types.h"
#include "rclc/check.h"
#include "rclc/detail/app_main.h"
#include "rclc/init.h"
#include "rclc/node.h"
#include "rclc/publisher.h"
#include "rclc/subscription.h"
#include "rclc/service.h"
#include "rclc/client.h"
#include "rclc/executor.h"
#include "rclc/timer.h"
#include "rclc/types.h"
#include "rclc/types.h"
#include "rclc/action.h"
#include "rclc/detail/node_pkg.h"
/* `RCLC_C_COMPONENT` — so a C component includes only <rclc/rclc.h>; the nros/
 * headers are internal (phase-492, maintainer ruling 2026-10-13). */
#include "rclc/component.h"
#include "rclc/detail/cdr.h"
/* issue 0795 — the RFC-0033 zero-copy reader (`nros_cdr_borrow_*`,
 * `rclc_view_str_t`, the `rclc_le_slice_view_*_t` family). Reached only
 * from generated message headers until now, so a user who included the
 * umbrella had no zero-copy read path and the C surface read as lacking one. */
#include "rclc/detail/view.h"
#include "rclc/types.h"
/* issue 0795 — `nros.hpp` has always included its C++ twin; C had no logging
 * surface through the umbrella at all. A C author who finds no logger reaches
 * for `printf`, and issue 0589 makes the equivalent fatal on Zephyr
 * native_sim, so the logger has to be the thing found first. */
#include "rclc/log.h"
#include "rclc/rosout.h"
#include "rclc/detail/boot_config.h"
/* The rcl spellings of the return codes (`RMW_RET_*`). Refuses to compile
 * beside a real `<rcl/types.h>`. */
#include "rclc/detail/rcl_compat.h"

#endif /* RCLC_H */
