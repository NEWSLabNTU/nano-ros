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
#include "nros/cdr.h"
#include "nros/clock.h"

#endif /* NROS_H */
