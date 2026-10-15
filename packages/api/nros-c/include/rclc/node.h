/**
 * @file node.h
 * @brief `<rclc/node.h>` at upstream's include path (phase-492 W2, phase-501).
 *
 * Every declaration this header names lives in the generated
 * `<rclc/detail/nros_generated.h>`, reached through `<rclc/types.h>`.
 */

#ifndef RCLC__NODE_H_
#define RCLC__NODE_H_

#include "rclc/types.h"

#endif /* RCLC__NODE_H_ */

/* Every upstream-path header carries the whole C API, as `<rclc/rclc.h>` does
 * (phase-492): a ported file that includes only `<rclc/node.h>` still finds
 * `RCLC_CHECK_RET`, `RCLC_LOG_*` and the rcl spellings. Outside the guard, so
 * this header's own declarations are complete before the umbrella reads them. */
#include "rclc/rclc.h"
