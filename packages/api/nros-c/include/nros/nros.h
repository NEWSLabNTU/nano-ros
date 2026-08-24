/**
 * @file nros.h
 * @brief Umbrella header for the nros C API.
 *
 * Including this single header provides access to all nros C API
 * modules.  You may also include individual headers (e.g.,
 * @c <nros/publisher.h>) if you prefer finer-grained includes.
 */

#ifndef NROS_H
#define NROS_H

#include "nros/types.h"
#include "nros/check.h"
#include "nros/app_main.h"
#include "nros/init.h"
#include "nros/node.h"
#include "nros/publisher.h"
#include "nros/subscription.h"
#include "nros/service.h"
#include "nros/client.h"
#include "nros/executor.h"
#include "nros/timer.h"
#include "nros/guard_condition.h"
#include "nros/lifecycle.h"
#include "nros/action.h"
#include "nros/parameter.h"
#include "nros/node_pkg.h"
#include "nros/cdr.h"
/* issue 0795 — the RFC-0033 zero-copy reader (`nros_cdr_borrow_*`,
 * `nros_borrowed_str_t`, the `nros_le_slice_view_*_t` family). Reached only
 * from generated message headers until now, so a user who included the
 * umbrella had no zero-copy read path and the C surface read as lacking one. */
#include "nros/borrowed.h"
#include "nros/clock.h"
/* issue 0795 — `nros.hpp` has always included its C++ twin; C had no logging
 * surface through the umbrella at all. A C author who finds no logger reaches
 * for `printf`, and issue 0589 makes the equivalent fatal on Zephyr
 * native_sim, so the logger has to be the thing found first. */
#include "nros/log.h"
#include "nros/boot_config.h"

#endif /* NROS_H */
