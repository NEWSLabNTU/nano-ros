/* phase-417 W5.d — rclc's PRESET constructors, written the way a ported rclc
 * file writes them.
 *
 * rclc ships a named entry point per QoS preset. Five of them had no nano-ros
 * spelling at all; this TU is the assertion that they now do, with rclc's
 * argument ORDER and rclc's MEANING.
 *
 * All five are `static inline` forwarders in the per-module headers
 * (`<nros/publisher.h>`, `<nros/subscription.h>`, `<nros/service.h>`,
 * `<nros/client.h>`, `<nros/action.h>`), which is the shape
 * `nros_difference_times` in `<nros/timer.h>` already has: no exported symbol,
 * no writable data. That is also why this probe compiles rather than links —
 * there is no symbol to resolve, and a link would need a registered RMW
 * backend, which is a fixture rather than a syntax check.
 *
 * WHAT A COMPILE CAN AND CANNOT PROVE HERE, and why that split decides the
 * test plan:
 *
 *   - Four of the five change only the QoS a caller would otherwise pass by
 *     hand, and a QoS reaches the wire through a backend. rodata in, rodata
 *     out: there is no behaviour to run, so the assertion is the SHAPE.
 *   - `rclc_action_client_init_default` is the one that does something — it
 *     SWAPS arguments 3 and 4, because rclc puts the typesupport third and we
 *     put the name third. Both are pointers, so a mis-wired swap is a WARNING
 *     in C, not an error (RFC-0089's recorded hazard, and the reason phase-417
 *     pairs every reorder with a rename). The `#pragma` below promotes that
 *     warning to an error INSIDE this TU, so the forwarder's body — which is
 *     compiled here, being `static inline` — cannot ship mis-wired.
 *
 * Taking a function POINTER of each rather than only calling it is deliberate,
 * as in `rcl_compat_aliases.c`: a call-shaped test passes against a macro that
 * quietly reordered arguments, where a pointer assignment needs the signature
 * to match exactly.
 */

/* Scoped to this TU; the headers impose nothing on consumers. */
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic error "-Wincompatible-pointer-types"
#endif

#include <nros/nros.h>

#include <stddef.h>

/* ── 1. Every preset resolves, with the documented signature ────────────── */

static nros_ret_t (*const k_pub_be)(struct nros_publisher_t*, const struct nros_node_t*,
                                    const struct nros_message_type_t*,
                                    const char*) = rclc_publisher_init_best_effort;

static nros_ret_t (*const k_sub_be)(struct nros_subscription_t*, const struct nros_node_t*,
                                    const struct nros_message_type_t*,
                                    const char*) = rclc_subscription_init_best_effort;

static nros_ret_t (*const k_srv_be)(struct nros_service_t*, const struct nros_node_t*,
                                    const struct nros_service_type_t*,
                                    const char*) = rclc_service_init_best_effort;

static nros_ret_t (*const k_cli_be)(struct nros_client_t*, const struct nros_node_t*,
                                    const struct nros_service_type_t*,
                                    const char*) = rclc_client_init_best_effort;

/* rclc's order: typesupport third, action name fourth. If someone "fixes" this
 * to match `nros_action_client_init`'s order, the assignment stops compiling —
 * which is the point, because the CALL would only have warned. */
static nros_ret_t (*const k_action_cli)(struct nros_action_client_t*, const struct nros_node_t*,
                                        const struct nros_action_type_t*,
                                        const char*) = rclc_action_client_init_default;

/* ── 2. The one preset with BEHAVIOUR is proved by the pragma above ──────
 *
 * `rclc_action_client_init_default`'s body swaps arguments 3 and 4 onto
 * `nros_action_client_init`. Because the forwarder is `static inline`, that
 * body is compiled in THIS translation unit, and with
 * `-Wincompatible-pointer-types` promoted to an error a forwarder that dropped
 * the swap — passing `const struct nros_action_type_t *` into the `const char *`
 * parameter — fails here. That is a stronger check than a call could give and
 * it costs no fixture: a RUN would need an initialised node, which needs a
 * registered RMW backend.
 *
 * The other four change only which `nros_qos_t` reaches
 * `nros_*_init_with_qos`; there is no code path to exercise, so the assertion
 * is the shape above plus the profile relationships pinned as Rust unit tests
 * in `packages/api/nros-c/src/qos.rs` (`NROS_QOS_SENSOR_DATA` is NOT
 * `NROS_QOS_DEFAULT` with reliability flipped — the depth differs — and
 * `NROS_QOS_SERVICES` is field-identical to `NROS_QOS_DEFAULT`, which is what
 * makes the reliable/best-effort service pair differ in exactly one field).
 *
 * Compile-only, no `main`: like `rcl_compat_aliases.c` next door, the claim is
 * that these names resolve with these types.
 */
