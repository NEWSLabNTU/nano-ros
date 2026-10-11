/* phase-492 W2 — every upstream include path a ported rcl/rclc file uses
 * resolves, and what it brings in is the rcl/rclc-spelled C API.
 *
 * Syntax-only (`just check c`): the defect this pins is a missing or broken
 * header, or a name still spelled the old way, and both fail to COMPILE. Each
 * header is included on its own line, so a removed file names itself.
 */

#include <rcl/allocator.h>
#include <rcl/arguments.h>
#include <rcl/client.h>
#include <rcl/context.h>
#include <rcl/domain_id.h>
#include <rcl/error_handling.h>
#include <rcl/event.h>
#include <rcl/event_callback.h>
#include <rcl/graph.h>
#include <rcl/guard_condition.h>
#include <rcl/init.h>
#include <rcl/init_options.h>
#include <rcl/log_level.h>
#include <rcl/logging.h>
#include <rcl/node.h>
#include <rcl/node_options.h>
#include <rcl/publisher.h>
#include <rcl/rcl.h>
#include <rcl/service.h>
#include <rcl/subscription.h>
#include <rcl/time.h>
#include <rcl/timer.h>
#include <rcl/types.h>
#include <rcl/wait.h>
#include <rcl_action/action_client.h>
#include <rcl_action/action_server.h>
#include <rcl_action/goal_handle.h>
#include <rcl_action/goal_state_machine.h>
#include <rcl_action/graph.h>
#include <rcl_action/rcl_action.h>
#include <rcl_action/types.h>
#include <rcl_action/wait.h>
#include <rcl_lifecycle/data_types.h>
#include <rcl_lifecycle/default_state_machine.h>
#include <rcl_lifecycle/rcl_lifecycle.h>
#include <rclc/action_client.h>
#include <rclc/action_goal_handle.h>
#include <rclc/action_server.h>
#include <rclc/client.h>
#include <rclc/executor.h>
#include <rclc/executor_handle.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/publisher.h>
#include <rclc/rclc.h>
#include <rclc/service.h>
#include <rclc/sleep.h>
#include <rclc/subscription.h>
#include <rclc/timer.h>
#include <rclc/types.h>
#include <rclc_lifecycle/rclc_lifecycle.h>

/* The spellings a ported rclc talker writes, used as rclc writes them. */
static rclc_support_t support;
static rcl_node_t node;
static rcl_publisher_t publisher;
static rcl_timer_t timer;
static rclc_executor_t executor;

int nros_upstream_include_paths_probe(void);
int nros_upstream_include_paths_probe(void) {
    rcl_ret_t rc = RCL_RET_OK;
    (void)support;
    (void)node;
    (void)publisher;
    (void)timer;
    (void)executor;
    rc = rclc_executor_spin_some(&executor, 0);
    return rc == RCL_RET_OK ? 0 : 1;
}
