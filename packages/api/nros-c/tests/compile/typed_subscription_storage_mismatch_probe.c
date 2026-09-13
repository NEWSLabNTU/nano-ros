/*
 * phase-417 W5.a — EXPECTED-FAILURE probe: the caller's message storage must be
 * of the type whose deserialiser is about to write into it.
 *
 * The positive control is `typed_subscription_delivery.c`; this is its negative
 * control, and the pair is the whole assertion. Typed delivery hands the FFI a
 * `void *msg` plus a type-erased deserialiser, which is rclc's shape — and
 * `void *` accepts any object pointer silently. Five of the six arguments the
 * generated macro supplies come from ONE type token and therefore cannot
 * disagree; `msg` is the sixth, it comes from the caller, and nothing about it
 * is a type until the macro makes it one.
 *
 * Get it wrong and there is no allocator to notice: the deserialiser writes a
 * `probe_msg_reading`-shaped message over whatever the storage really is, every
 * dispatch, forever. That is memory corruption with a clean compile — the
 * "compiles and differs" RFC-0089 forbids, in its most expensive form.
 *
 * So the macro routes `msg` through `1 ? (msg) : (<Msg>*)0`. A conditional
 * expression's two branches must be COMPATIBLE pointers, so the wrong struct is
 * diagnosed here, naming both types. This TU must NOT compile clean.
 *
 * The lane compiles it with -Werror, deliberately: in C an incompatible pointer
 * in a conditional is a constraint violation that gcc and clang both report as
 * a WARNING (the same asymmetry RFC-0089's `rcl_node_init_reorder_probe`
 * records for an incompatible pointer ARGUMENT), while C++ rejects it outright.
 * -Werror is what makes the C diagnostic load-bearing rather than advisory.
 */

#include "nros/executor.h"
#include "nros/subscription.h"

/* Two stand-in generated message types. Hand-written for the reason
 * `typed_subscription_delivery.c` gives: `generated/` trees do not exist in a
 * fresh clone, so a probe that included one could not run on this lane. */
typedef struct probe_msg_reading {
    int32_t value;
    char label[32];
} probe_msg_reading;

typedef struct probe_msg_command {
    double setpoint;
} probe_msg_command;

int32_t probe_msg_reading_deserialize(probe_msg_reading* msg, const uint8_t* buffer,
                                      size_t buffer_size);

static inline int32_t probe_msg_reading_deserialize_erased(void* msg, const uint8_t* buffer,
                                                           size_t buffer_size) {
    return probe_msg_reading_deserialize((probe_msg_reading*)msg, buffer, buffer_size);
}

/* Byte-for-byte what `packages/cli/rosidl-codegen/packs/c/message.h.jinja`
 * emits, modulo the type token — including the storage check. */
#define PROBE_MSG_READING_RX_MAX_SERIALIZED_SIZE 48
#define probe_msg_reading_executor_add_subscription_sized(executor, subscription, msg, cb, ctx,    \
                                                          invocation, rx_bytes)                    \
    nros_executor_add_subscription_typed_sized(                                                    \
        (executor), (subscription), (1 ? (msg) : (probe_msg_reading*)0),                           \
        probe_msg_reading_deserialize_erased, (cb), (ctx), (invocation), (uint32_t)(rx_bytes))
#define probe_msg_reading_executor_add_subscription(executor, subscription, msg, cb, ctx,          \
                                                    invocation)                                    \
    probe_msg_reading_executor_add_subscription_sized((executor), (subscription), (msg), (cb),     \
                                                      (ctx), (invocation),                         \
                                                      PROBE_MSG_READING_RX_MAX_SERIALIZED_SIZE)

static struct {
    nros_executor_t executor;
    nros_subscription_t subscription;
    /* The storage the caller owns -- and it is the WRONG type for the
     * subscription being registered below. */
    probe_msg_command msg;
} g_app;

static void probe_on_reading(const void* msgin, void* context) {
    (void)msgin;
    (void)context;
}

nros_ret_t nros_typed_subscription_mismatch_probe(void);
nros_ret_t nros_typed_subscription_mismatch_probe(void) {
    /* `&g_app.msg` is a `probe_msg_command*`; the registration is for
     * `probe_msg_reading`. This is the line that must not compile. */
    return probe_msg_reading_executor_add_subscription(&g_app.executor, &g_app.subscription,
                                                       &g_app.msg, probe_on_reading, NULL,
                                                       NROS_EXECUTOR_ON_NEW_DATA);
}
