/* Issue 2712 — a C subscription callback reads each message's metadata.
 *
 * `rclc_executor_add_subscription_with_message_info` hands the callback an
 * `rmw_message_info_t`: timestamps, sequence numbers and the publisher's GID,
 * as the backend reported them. The two older `*_with_info` calls hand over
 * the wire attachment and no metadata at all, which is what this replaces.
 *
 * The stub backend fills its `take_with_info` slot from what a sample was
 * staged with. This probe checks:
 *
 *   1. a sample staged WITH metadata reaches the callback carrying exactly
 *      that metadata: both timestamps, both sequence numbers, the GID — never
 *      a zeroed struct posing as one;
 *   2. the payload arrives whole, and the context word is passed through;
 *   3. a backend with no `take_with_info` slot still dispatches, with a NULL
 *      `info` ("reported nothing"), not a zeroed struct;
 *   4. NULL arguments are refused;
 *   5. `<nros/rmw_entity.h>` and `<rclc/rclc.h>` both declare
 *      `rmw_message_info_t`, and one translation unit can include both.
 *
 * Mutation-checked: passing a zeroed `rmw_message_info_t` instead of the
 * converted one fails (1).
 */

#include "stub_rmw_backend.h"

#include <nros/rmw_entity.h>
#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
    (void)nros_stub_rmw_register_without_message_info("stub_no_info");
}

static int s_failures = 0;

#define CHECK(cond, msg)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL: %s (%s:%d)\n", (msg), __FILE__, __LINE__);                      \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

#define CHECK_RET(expr, expected, msg)                                                             \
    do {                                                                                           \
        rcl_ret_t _r = (expr);                                                                     \
        if (_r != (expected)) {                                                                    \
            fprintf(stderr, "FAIL: %s -- got %d, expected %d (%s:%d)\n", (msg), (int)_r,           \
                    (int)(expected), __FILE__, __LINE__);                                          \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

static const rclc_message_type_t INT32 = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 64,
};

/* A CDR Int32: encapsulation header + one int32. */
static const uint8_t SAMPLE[8] = {0x00, 0x01, 0x00, 0x00, 42, 0, 0, 0};

static int s_calls = 0;
static size_t s_len = 0;
static bool s_info_null = true;
static rmw_message_info_t s_info;

static void on_sample(const uint8_t* data, size_t len, const rmw_message_info_t* info,
                      void* context) {
    CHECK(context == (void*)&s_calls, "the context word is passed through");
    s_calls++;
    s_len = len;
    CHECK(len == sizeof(SAMPLE) && memcmp(data, SAMPLE, len) == 0, "the payload is whole");
    s_info_null = (info == NULL);
    if (info != NULL) s_info = *info;
}

static void pump(rclc_executor_t* executor) {
    for (int i = 0; i < 4; i++)
        (void)rclc_executor_spin_some(executor, 0);
}

typedef struct {
    struct rclc_support_t support;
    rclc_executor_t executor;
    rcl_node_t node;
    rcl_subscription_t sub;
} rig_t;

static bool rig_up(rig_t* rig, const char* rmw, const char* topic) {
    rig->support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&rig->support, "stub://none", 51, "message_info", rmw),
              RCL_RET_OK, "support opened on the stub backend");
    rig->executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&rig->executor, &rig->support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rig->node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&rig->executor, &rig->node, "listener", &opts), RCL_RET_OK,
              "node bound to the executor");
    rig->sub = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&rig->sub, &rig->node, &INT32, topic), RCL_RET_OK,
              "subscription created");
    CHECK_RET(rclc_executor_add_subscription_with_message_info(&rig->executor, &rig->sub, on_sample,
                                                               &s_calls, ON_NEW_DATA),
              RCL_RET_OK, "message-info subscription added to the executor");
    return s_failures == 0;
}

static void rig_down(rig_t* rig) {
    (void)rclc_executor_remove_subscription(&rig->executor, &rig->sub);
    (void)rcl_subscription_fini(&rig->sub, &rig->node);
    (void)rcl_node_fini(&rig->node);
    (void)rclc_executor_fini(&rig->executor);
    (void)rclc_support_fini(&rig->support);
}

int main(void) {
    nros_stub_rmw_set_accept_entities(true);
    /* Staged samples have no arrival callback: the executor must poll. */
    nros_stub_rmw_set_entity_wake_supported(false);

    /* 4. NULL arguments. */
    CHECK_RET(
        rclc_executor_add_subscription_with_message_info(NULL, NULL, on_sample, NULL, ON_NEW_DATA),
        RCL_RET_INVALID_ARGUMENT, "NULL executor refused");

    /* 1 + 2. Staged metadata reaches the callback verbatim. */
    rig_t rig;
    if (!rig_up(&rig, NROS_STUB_RMW_NAME, "/chatter")) return 1;
    nros_stub_message_info_t info;
    memset(&info, 0, sizeof(info));
    info.source_timestamp = 1700000000000000001LL;
    info.received_timestamp = 1700000000000000777LL;
    info.publication_sequence_number = 41;
    info.reception_sequence_number = 7;
    for (int i = 0; i < 24; i++)
        info.publisher_gid[i] = (uint8_t)(0xa0 + i);
    CHECK(nros_stub_rmw_stage_sample_info("/chatter", SAMPLE, sizeof(SAMPLE), &info),
          "stage a sample with metadata");
    pump(&rig.executor);
    CHECK(s_calls == 1, "the callback ran once");
    CHECK(!s_info_null, "a backend that reports metadata gives the callback a non-NULL info");
    CHECK(s_info.source_timestamp == info.source_timestamp, "the source timestamp");
    CHECK(s_info.received_timestamp == info.received_timestamp, "the received timestamp");
    CHECK(s_info.publication_sequence_number == 41, "the publication sequence number");
    CHECK(s_info.reception_sequence_number == 7, "the reception sequence number");
    CHECK(memcmp(s_info.publisher_gid.data, info.publisher_gid, 24) == 0, "the publisher GID");
    rig_down(&rig);

    /* 3. A backend with no take_with_info slot: dispatched, info NULL. */
    s_calls = 0;
    s_info_null = false;
    if (!rig_up(&rig, "stub_no_info", "/chatter_plain")) return 1;
    CHECK(nros_stub_rmw_stage_sample("/chatter_plain", SAMPLE, sizeof(SAMPLE)),
          "stage a sample with no metadata");
    pump(&rig.executor);
    CHECK(s_calls == 1, "the callback ran once without metadata");
    CHECK(s_info_null, "no metadata reported is a NULL info, not a zeroed struct");
    rig_down(&rig);

    if (s_failures == 0) {
        printf("subscription_message_info: OK\n");
        return 0;
    }
    fprintf(stderr, "subscription_message_info: %d failure(s)\n", s_failures);
    return 1;
}
