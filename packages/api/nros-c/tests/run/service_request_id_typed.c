/* Issue 2089 — rclc's TYPED request-id service registration, unchanged.
 *
 * rclc's
 *
 *     rcl_ret_t rclc_executor_add_service_with_request_id(
 *         rclc_executor_t *executor, rcl_service_t *service,
 *         void *request_msg, void *response_msg,
 *         rclc_service_callback_with_request_id_t callback);
 *     typedef void (*rclc_service_callback_with_request_id_t)(
 *         const void *request, rmw_request_id_t *request_id, void *response);
 *
 * is written below exactly as a ported rclc program writes it. The codecs are
 * the service descriptor's, which `rclc_service_init_default` keeps. The stub
 * backend fills `take_request_with_info` from what each request was staged
 * with. This probe checks:
 *
 *   1. the callback sees the DESERIALISED request in the caller's struct and
 *      the request's id (writer GUID + the client's sequence number, not the
 *      backend's reply token);
 *   2. the caller's response struct is serialised and sent against the
 *      backend's token;
 *   3. a request staged with no id reaches the callback all zero;
 *   4. a request that does not decode never reaches the callback and gets no
 *      reply;
 *   5. a response that does not encode is not sent;
 *   6. a service whose descriptor has no codecs is refused with
 *      RCL_RET_UNSUPPORTED, and NULL storage or callback with
 *      RCL_RET_INVALID_ARGUMENT.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) { (void)nros_stub_rmw_register(); }

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

/* An AddTwoInts-shaped pair with hand-written CDR codecs, standing in for a
 * generated `example_interfaces__srv__AddTwoInts` so the probe needs no
 * codegen. Little-endian CDR: a 4-byte encapsulation header, then the fields. */
typedef struct {
    int64_t a;
    int64_t b;
} add_request_t;
typedef struct {
    int64_t sum;
} add_response_t;

static const uint8_t CDR_LE_HEADER[4] = {0x00, 0x01, 0x00, 0x00};

static int32_t request_deserialize(const uint8_t* buf, size_t len, void* msg) {
    add_request_t* req = (add_request_t*)msg;
    if (len != 4 + 16 || memcmp(buf, CDR_LE_HEADER, 4) != 0) return -1;
    memcpy(&req->a, buf + 4, 8);
    memcpy(&req->b, buf + 12, 8);
    return 0;
}

static int32_t response_serialize(const void* msg, uint8_t* buf, size_t cap, size_t* out_len) {
    const add_response_t* res = (const add_response_t*)msg;
    /* A sum of -1 stands for "does not fit": the probe's oversized reply. */
    if (cap < 4 + 8 || res->sum == -1) return -1;
    memcpy(buf, CDR_LE_HEADER, 4);
    memcpy(buf + 4, &res->sum, 8);
    *out_len = 4 + 8;
    return 0;
}

static const rclc_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .request_deserialize = request_deserialize,
    .response_serialize = response_serialize,
};

/* Names alone: a descriptor the typed registration cannot use. */
static const rclc_service_type_t SRV_TYPE_NO_CODECS = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

static add_request_t s_req;
static add_response_t s_res;
static int s_calls = 0;
static add_request_t s_seen_req;
static rmw_request_id_t s_seen_id;

/* Exactly rclc's callback shape: no context, no return value. */
static void on_request(const void* req, rmw_request_id_t* request_id, void* res) {
    const add_request_t* in = (const add_request_t*)req;
    add_response_t* out = (add_response_t*)res;
    s_calls++;
    s_seen_req = *in;
    s_seen_id = *request_id;
    out->sum = in->a + in->b;
}

static void make_request(uint8_t out[20], int64_t a, int64_t b) {
    memcpy(out, CDR_LE_HEADER, 4);
    memcpy(out + 4, &a, 8);
    memcpy(out + 12, &b, 8);
}

static bool all_zero(const rmw_request_id_t* id) {
    for (size_t i = 0; i < sizeof(id->writer_guid); i++)
        if (id->writer_guid[i] != 0) return false;
    return id->sequence_number == 0;
}

static void pump(rclc_executor_t* executor) {
    for (int i = 0; i < 4; i++) (void)rclc_executor_spin_some(executor, 0);
}

int main(void) {
    nros_stub_rmw_set_accept_entities(true);
    /* Staged requests have no arrival callback: the executor must poll. */
    nros_stub_rmw_set_entity_wake_supported(false);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 50, "service_request_id_typed",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "server", &opts), RCL_RET_OK,
              "node bound to the executor");

    /* 6. Refusals before anything is registered. */
    rcl_service_t bare = rcl_get_zero_initialized_service();
    CHECK_RET(rclc_service_init_default(&bare, &node, &SRV_TYPE_NO_CODECS, "/bare"), RCL_RET_OK,
              "codec-less service created");
    CHECK_RET(rclc_executor_add_service_with_request_id(&executor, &bare, &s_req, &s_res,
                                                        on_request),
              RCL_RET_UNSUPPORTED, "a descriptor with no codecs is refused, not dispatched raw");
    (void)rcl_service_fini(&bare, &node);

    rcl_service_t service = rcl_get_zero_initialized_service();
    CHECK_RET(rclc_service_init_default(&service, &node, &SRV_TYPE, "/add_two_ints"), RCL_RET_OK,
              "service created");
    CHECK_RET(rclc_executor_add_service_with_request_id(NULL, &service, &s_req, &s_res,
                                                        on_request),
              RCL_RET_INVALID_ARGUMENT, "NULL executor refused");
    CHECK_RET(rclc_executor_add_service_with_request_id(&executor, &service, NULL, &s_res,
                                                        on_request),
              RCL_RET_INVALID_ARGUMENT, "NULL request storage refused");
    CHECK_RET(rclc_executor_add_service_with_request_id(&executor, &service, &s_req, NULL,
                                                        on_request),
              RCL_RET_INVALID_ARGUMENT, "NULL response storage refused");
    CHECK_RET(
        rclc_executor_add_service_with_request_id(&executor, &service, &s_req, &s_res, NULL),
        RCL_RET_INVALID_ARGUMENT, "NULL callback refused");

    /* The registration a ported rclc program writes. */
    CHECK_RET(rclc_executor_add_service_with_request_id(&executor, &service, &s_req, &s_res,
                                                        on_request),
              RCL_RET_OK, "typed request-id service added");

    /* 1 + 2. A staged id and a decodable request. */
    uint8_t request[20];
    make_request(request, 40, 2);
    nros_stub_service_info_t info;
    memset(&info, 0, sizeof(info));
    for (int i = 0; i < 16; i++) info.writer_guid[i] = (int8_t)(0x10 + i);
    info.sequence_number = 77;
    CHECK(nros_stub_rmw_stage_request_info("/add_two_ints", 5, request, sizeof(request), &info),
          "stage a request with an id");
    pump(&executor);
    CHECK(s_calls == 1, "the typed callback ran once");
    CHECK(s_seen_req.a == 40 && s_seen_req.b == 2,
          "the callback sees the request DESERIALISED into the caller's struct");
    CHECK(memcmp(s_seen_id.writer_guid, info.writer_guid, 16) == 0,
          "the callback sees the client's writer GUID");
    CHECK(s_seen_id.sequence_number == 77,
          "the callback sees the CLIENT's sequence number, not the reply token");
    int64_t seq = -1;
    size_t len = 0;
    const uint8_t* sent = nros_stub_rmw_last_response("/add_two_ints", &seq, &len);
    int64_t sum = 0;
    if (sent != NULL && len == 12) memcpy(&sum, sent + 4, 8);
    CHECK(sent != NULL && len == 12 && memcmp(sent, CDR_LE_HEADER, 4) == 0 && sum == 42,
          "the caller's response struct was serialised and sent");
    CHECK(seq == 5, "the response is sent against the backend's reply token");

    /* 3. No id staged: all zero. */
    memset(&s_seen_id, 0x5a, sizeof(s_seen_id));
    make_request(request, 1, 2);
    CHECK(nros_stub_rmw_stage_request("/add_two_ints", 6, request, sizeof(request)),
          "stage a request without an id");
    pump(&executor);
    CHECK(s_calls == 2, "the second request was dispatched");
    CHECK(all_zero(&s_seen_id), "a request the backend reported no id for is all zero");

    /* 4. A request that does not decode: no callback, no reply. */
    const uint8_t truncated[8] = {0x00, 0x01, 0x00, 0x00, 1, 2, 3, 4};
    CHECK(nros_stub_rmw_stage_request("/add_two_ints", 7, truncated, sizeof(truncated)),
          "stage a malformed request");
    pump(&executor);
    CHECK(s_calls == 2, "a request that does not decode never reaches the callback");
    seq = -1;
    (void)nros_stub_rmw_last_response("/add_two_ints", &seq, &len);
    CHECK(seq != 7, "...and gets no reply");

    /* 5. A response that does not encode is not sent. */
    make_request(request, -3, 2); /* sum -1: the codec's "does not fit" */
    CHECK(nros_stub_rmw_stage_request("/add_two_ints", 8, request, sizeof(request)),
          "stage a request whose reply cannot be encoded");
    pump(&executor);
    CHECK(s_calls == 3, "the callback ran for a decodable request");
    seq = -1;
    (void)nros_stub_rmw_last_response("/add_two_ints", &seq, &len);
    CHECK(seq != 8, "a reply that does not encode is not sent");

    (void)rclc_executor_remove_service(&executor, &service);
    (void)rcl_service_fini(&service, &node);
    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "service_request_id_typed: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("service_request_id_typed: OK\n");
    return 0;
}
