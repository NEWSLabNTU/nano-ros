/**
 * @file action.h
 * @ingroup grp_action
 * @brief Action server and client API.
 *
 * Actions provide long-running goal-oriented communication with
 * feedback and cancellation support.
 */

#ifndef NROS_ACTION_H
#define NROS_ACTION_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/action.h>` continues to compile. */
#include "nros/types.h"

/**
 * @brief rclc's default preset constructor for an action client.
 *
 * phase-417 W5.d. Ledger row `c:action_client_init_default` was `declined` on
 * "`nros_action_client_init` takes no QoS and IS the default form" — which
 * says the CAPABILITY corresponds, not that the name may be missing. RFC-0089
 * is explicit that a correspondence is a reason to take the spelling, so a
 * ported `rclc_action` file compiles on this line instead of on a diff.
 *
 * **The argument order is rclc's, and that is the whole reason this exists as
 * a separate name rather than as a rename of nros_action_client_init():**
 * rclc puts the typesupport third and the action name fourth, ours has them
 * the other way round. Both are pointers, so a caller who guessed wrong gets a
 * WARNING and a client initialised with a type descriptor as its name — the
 * silent-reorder hazard RFC-0089 records, which is exactly why phase-417 pairs
 * every reorder with a rename. The forwarder below is where the swap happens,
 * once, and `tests/compile/preset_constructors.c` promotes
 * `-Wincompatible-pointer-types` to an error so a mis-wired swap cannot ship.
 *
 * This is the CALLBACK-mode client (rclc registers the goal/feedback/result
 * callbacks later, at `rclc_executor_add_action_client`), so it forwards to
 * nros_action_client_init() and not to nros_action_client_init_polling().
 *
 * `static inline`: no symbol, no writable data.
 *
 * @param[out] action_client Zero-initialised action client to fill in.
 * @param[in]  node          An initialised node.
 * @param[in]  type_info     Generated action type descriptor.
 * @param[in]  action_name   Action name, null-terminated.
 * @return Whatever nros_action_client_init() returns.
 */
static inline nros_ret_t rclc_action_client_init_default(struct nros_action_client_t* action_client,
                                                         const struct nros_node_t* node,
                                                         const struct nros_action_type_t* type_info,
                                                         const char* action_name) {
    return nros_action_client_init(action_client, node, action_name, type_info);
}

/*
 * NOT provided: `rclc_action_server_init_default`. phase-417 W5.d looked and
 * refused, and the reason is a PARAMETER rather than an order:
 *
 *   rclc: rcl_ret_t rclc_action_server_init_default(
 *             rclc_action_server_t *action_server, rcl_node_t *node,
 *             rclc_support_t *support,
 *             const rosidl_action_type_support_t *type_support,
 *             const char *action_name)
 *
 * rclc's body (`rclc/src/rclc/action_server.c` @ `10eadcc`) reads `support`
 * for exactly one thing — it passes `&support->clock` to `rcl_action_server_init`.
 * Ours has nothing to pass it to: `nros_action_server_init` "stores metadata
 * (name, type, callbacks)" and defers RMW entity creation to
 * `nros_executor_add_action_server()`, so it takes neither a clock nor a
 * support handle. A faithful-looking five-argument forwarder would therefore
 * carry an argument it IGNORES — the inert-parameter defect RFC-0089 exists to
 * end, and the same refusal `nros_node_resolve_name` makes for rcl's
 * `allocator` and `is_service`.
 *
 * The second half is the callbacks. rclc registers goal and cancel handlers at
 * `rclc_executor_add_action_server`; ours are bound at init
 * (`nros_action_server_init(server, node, action_name, type_info, goal_cb,
 * cancel_cb, accepted_cb, ...)`). Forwarding to the callback-free
 * `nros_action_server_init_polling` instead would compile and then dispatch
 * nothing, which is the "plausible name over an opposite data contract" class.
 *
 * Closing this needs the `rclc_executor_add_action_server` binding site, not a
 * wrapper. Ledger row `c:action_server_init_default` records it, still declined.
 */

#endif /* NROS_ACTION_H */
