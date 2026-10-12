/*
 * phase-417 W5.a / phase-492 W8 (RFC-0089) — TYPED subscription delivery for C,
 * in rclc's own signature.
 *
 * rclc hands a subscription callback a DESERIALISED message, on a path with no
 * allocator, by making the caller own the storage:
 *
 *     rclc_executor_add_subscription_with_context(&exec, &sub, &msg, &cb, &ctx, ON_NEW_DATA);
 *     typedef void (*rclc_subscription_callback_with_context_t)(const void*, void*);
 *
 * Until phase-492 W8 ours took a SEVENTH argument, the deserialiser, because
 * the type descriptor carried none, and a generated per-type macro collapsed it
 * away. The descriptor now carries `serialize` / `deserialize`, the
 * subscription keeps the descriptor it was created with, and the registration
 * is rclc's six arguments with no macro in between.
 *
 * Compile-only (no main). The entry points are taken as function POINTERS with
 * the documented signatures, so a drift in argument list or order fails here
 * rather than at a consumer's call site. The message type is hand-written on
 * purpose: this probe pins the API, `rosidl_talker_listener.c` pins the
 * generated message headers.
 */

#include "nros/executor.h"
#include "nros/publisher.h"
#include "nros/rcl_compat.h"
#include "nros/subscription.h"

/* ------------------------------------------------------------------ *
 * 1. The entry points exist with the documented signatures.
 * ------------------------------------------------------------------ */

static rcl_ret_t (*const kAddWithContext)(struct rclc_executor_t*, struct rcl_subscription_t*,
                                          void*, rclc_subscription_callback_with_context_t, void*,
                                          rclc_executor_handle_invocation_t) =
    rclc_executor_add_subscription_with_context;

static rcl_ret_t (*const kPublish)(const struct rcl_publisher_t*, const void*,
                                   rmw_publisher_allocation_t*) = rcl_publish;

static rcl_ret_t (*const kTake)(struct rclc_polling_subscription_t*, void*,
                                rmw_subscription_allocation_t*) = rclc_polling_subscription_take;

/* The descriptor's codec slots, with the RFC's signatures. */
static const rclc_message_serialize_fn_t kNullSerialize = (rclc_message_serialize_fn_t)0;
static const rclc_message_deserialize_fn_t kNullDeserialize = (rclc_message_deserialize_fn_t)0;

/* The raw path STAYS: a byte-oriented subscriber is a legitimate thing to
 * want. It takes its callback and context itself (phase-487 W1). */
static rcl_ret_t (*const kAddRaw)(
    struct rclc_executor_t*, struct rcl_subscription_t*, rclc_subscription_raw_callback_t, void*,
    rclc_executor_handle_invocation_t) = rclc_executor_add_subscription_raw;

/* ------------------------------------------------------------------ *
 * 2. A ported rclc node, as it now reads.
 * ------------------------------------------------------------------ */

typedef struct probe_reading {
    int32_t value;
} probe_reading;

typedef struct {
    int count;
} probe_app_ctx;

static struct {
    rclc_executor_t executor;
    rcl_subscription_t subscription;
    /* The caller's message storage: no allocator on the delivery path. */
    probe_reading msg;
    probe_app_ctx app;
} g_app;

/* The ported callback body: cast and read FIELDS. Never called on a failed
 * decode — the sample is dropped and counted instead. */
static void probe_on_reading(const void* msgin, void* context) {
    const probe_reading* msg = (const probe_reading*)msgin;
    probe_app_ctx* ctx = (probe_app_ctx*)context;
    ctx->count += (msg->value != 0) ? 1 : 0;
}

/* rclc's callback typedef name resolves to ours. */
static const rclc_subscription_callback_with_context_t kCb = probe_on_reading;

rcl_ret_t nros_typed_subscription_probe(void);
rcl_ret_t nros_typed_subscription_probe(void) {
    (void)kAddWithContext;
    (void)kPublish;
    (void)kTake;
    (void)kNullSerialize;
    (void)kNullDeserialize;
    (void)kAddRaw;

    /* rclc's six arguments, in rclc's order. */
    return rclc_executor_add_subscription_with_context(&g_app.executor, &g_app.subscription,
                                                       &g_app.msg, kCb, &g_app.app, ON_NEW_DATA);
}
