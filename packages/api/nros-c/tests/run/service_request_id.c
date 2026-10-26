/* Issue 2081 — a C service callback learns WHO sent the request.
 *
 * rclc's `rclc_executor_add_service_with_request_id` hands the callback an
 * `rmw_request_id_t` (the client's writer GUID and the sequence number that
 * client assigned) between the request and the response. Ours is
 * `rclc_executor_add_service_raw_with_request_id`, the CDR-byte form, and its
 * id comes from the RMW backend's `take_request_with_info` slot.
 *
 * The stub backend fills that slot from what a request was staged with. This
 * probe checks:
 *
 *   1. a request staged WITH an id reaches the callback carrying exactly that
 *      id — GUID and the client's sequence number, which is NOT the backend's
 *      reply token the response is sent against;
 *   2. a request staged with NO id reaches it all zero ("not reported");
 *   3. a backend with no `take_request_with_info` slot (a C backend written
 *      before it) still dispatches, with an all-zero id;
 *   4. the reply still goes out against the backend's token.
 *
 * Mutation-checked: building the id from `RawRequestId::default()` instead of
 * the server's `request_info()` (`arena.rs`) fails (1).
 */

#include "stub_rmw_backend.h"

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

static const rclc_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

/* A CDR request: encapsulation header + one int64. */
static const uint8_t REQUEST[12] = {0x00, 0x01, 0x00, 0x00, 7, 0, 0, 0, 0, 0, 0, 0};
static const uint8_t RESPONSE[8] = {0x00, 0x01, 0x00, 0x00, 1, 2, 3, 4};

static int s_calls = 0;
static rmw_request_id_t s_seen;

static bool on_request(const uint8_t* req, size_t req_len, rmw_request_id_t* request_id,
                       uint8_t* resp, size_t resp_cap, size_t* resp_len, void* context) {
    (void)req;
    (void)req_len;
    CHECK(context == (void*)&s_calls, "the context word is passed through");
    s_calls++;
    if (request_id != NULL) s_seen = *request_id;
    if (resp_cap < sizeof(RESPONSE)) return false;
    memcpy(resp, RESPONSE, sizeof(RESPONSE));
    *resp_len = sizeof(RESPONSE);
    return true;
}

static bool all_zero(const rmw_request_id_t* id) {
    for (size_t i = 0; i < sizeof(id->writer_guid); i++)
        if (id->writer_guid[i] != 0) return false;
    return id->sequence_number == 0;
}

static void pump(rclc_executor_t* executor) {
    for (int i = 0; i < 4; i++) (void)rclc_executor_spin_some(executor, 0);
}

typedef struct {
    struct rclc_support_t support;
    rclc_executor_t executor;
    rcl_node_t node;
    rcl_service_t service;
} rig_t;

static bool rig_up(rig_t* rig, const char* rmw, const char* service_name) {
    rig->support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&rig->support, "stub://none", 49, "service_request_id", rmw),
              RCL_RET_OK, "support opened on the stub backend");
    rig->executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&rig->executor, &rig->support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rig->node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&rig->executor, &rig->node, "server", &opts), RCL_RET_OK,
              "node bound to the executor");
    rig->service = rcl_get_zero_initialized_service();
    CHECK_RET(rclc_service_init_default(&rig->service, &rig->node, &SRV_TYPE, service_name),
              RCL_RET_OK, "service created");
    CHECK_RET(rclc_executor_add_service_raw_with_request_id(&rig->executor, &rig->service,
                                                            on_request, &s_calls),
              RCL_RET_OK, "request-id service added to the executor");
    return s_failures == 0;
}

static void rig_down(rig_t* rig) {
    (void)rclc_executor_remove_service(&rig->executor, &rig->service);
    (void)rcl_service_fini(&rig->service, &rig->node);
    (void)rcl_node_fini(&rig->node);
    (void)rclc_executor_fini(&rig->executor);
    (void)rclc_support_fini(&rig->support);
}

int main(void) {
    nros_stub_rmw_set_accept_entities(true);
    /* Staged requests have no arrival callback: the executor must poll. */
    nros_stub_rmw_set_entity_wake_supported(false);

    CHECK_RET(rclc_executor_add_service_raw_with_request_id(NULL, NULL, on_request, NULL),
              RCL_RET_INVALID_ARGUMENT, "NULL executor refused");

    rig_t rig;
    if (!rig_up(&rig, NROS_STUB_RMW_NAME, "/add_two_ints")) return 1;

    /* 1. An id staged with the request reaches the callback verbatim. */
    nros_stub_service_info_t info;
    memset(&info, 0, sizeof(info));
    for (int i = 0; i < 16; i++) info.writer_guid[i] = (int8_t)(i % 2 == 0 ? i + 1 : -(i + 1));
    info.sequence_number = 4242;
    info.source_timestamp = 1700000000000000001LL;
    CHECK(nros_stub_rmw_stage_request_info("/add_two_ints", 5, REQUEST, sizeof(REQUEST), &info),
          "stage a request with an id");
    pump(&rig.executor);
    CHECK(s_calls == 1, "the request-id callback ran once");
    CHECK(memcmp(s_seen.writer_guid, info.writer_guid, 16) == 0,
          "the callback sees the client's writer GUID");
    CHECK(s_seen.sequence_number == 4242,
          "the callback sees the CLIENT's sequence number, not the reply token");

    /* 4. ...and the reply goes out against the backend's token. */
    int64_t seq = -1;
    size_t len = 0;
    const uint8_t* sent = nros_stub_rmw_last_response("/add_two_ints", &seq, &len);
    CHECK(sent != NULL && len == sizeof(RESPONSE) && memcmp(sent, RESPONSE, len) == 0,
          "the callback's response was sent");
    CHECK(seq == 5, "the response is sent against the backend's reply token");

    /* 2. No id staged: all zero. */
    memset(&s_seen, 0x5a, sizeof(s_seen));
    CHECK(nros_stub_rmw_stage_request("/add_two_ints", 6, REQUEST, sizeof(REQUEST)),
          "stage a request without an id");
    pump(&rig.executor);
    CHECK(s_calls == 2, "the second request was dispatched");
    CHECK(all_zero(&s_seen), "a request the backend reported no id for is all zero");
    rig_down(&rig);

    /* 3. A backend with no `take_request_with_info` slot. */
    if (!rig_up(&rig, "stub_no_info", "/no_info")) return 1;
    memset(&s_seen, 0x5a, sizeof(s_seen));
    CHECK(nros_stub_rmw_stage_request_info("/no_info", 9, REQUEST, sizeof(REQUEST), &info),
          "stage a request on the slot-less backend");
    pump(&rig.executor);
    CHECK(s_calls == 3, "a backend without the slot still dispatches");
    CHECK(all_zero(&s_seen), "...with an all-zero id: that backend has no channel for it");
    rig_down(&rig);

    if (s_failures != 0) {
        fprintf(stderr, "service_request_id: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("service_request_id: OK\n");
    return 0;
}
