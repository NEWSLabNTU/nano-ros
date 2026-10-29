/*
 * phase-515 C3 -- rclc's TYPED service and client registrations, called with
 * rclc's signatures on a real executor over the stub RMW backend:
 *
 *     rclc_executor_add_service(&exec, &srv, &req, &resp, cb);
 *     rclc_executor_add_service_with_request_id(&exec, &srv, &req, &resp, cb);
 *     rclc_executor_add_service_with_context(&exec, &srv, &req, &resp, cb, ctx);
 *     rclc_executor_add_client(&exec, &cli, &resp, cb);
 *     rclc_executor_add_client_with_request_id(&exec, &cli, &resp, cb);
 *
 * The executor deserialises through the entity's descriptor, so the callbacks
 * see the caller's own message storage. Asserted here:
 *
 *   1. a staged request reaches each service's callback deserialised into its
 *      own `req`, and the response the callback wrote goes out serialised;
 *   2. the with_request_id form sees the id the request was staged with, and
 *      the with_context form its context word;
 *   3. a staged reply reaches the client's callback deserialised into `resp`,
 *      with `msgin == &resp`;
 *   4. a descriptor with no codec is UNSUPPORTED, and a NULL callback or
 *      message is INVALID_ARGUMENT;
 *   5. (issue 2136) the client with_request_id form sees the id of the request
 *      the reply answers: the sequence number it was matched on, and the
 *      writer GUID the backend's `take_response_with_info` reported.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
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

/* Request and response are both one int32: CDR header (CDR_LE) + the value. */
typedef struct {
    int32_t data;
} int32_msg_t;

static int32_t int32_serialize(const void* msg, uint8_t* buffer, size_t capacity, size_t* out_len) {
    if (capacity < 8) return -1;
    const uint8_t header[4] = {0x00, 0x01, 0x00, 0x00};
    memcpy(buffer, header, 4);
    memcpy(buffer + 4, &((const int32_msg_t*)msg)->data, 4);
    *out_len = 8;
    return 0;
}

static int32_t int32_deserialize(const uint8_t* buffer, size_t len, void* msg) {
    if (len != 8) return -1;
    memcpy(&((int32_msg_t*)msg)->data, buffer + 4, 4);
    return 0;
}

static const rclc_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::Double_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .request_serialize = int32_serialize,
    .request_deserialize = int32_deserialize,
    .response_serialize = int32_serialize,
    .response_deserialize = int32_deserialize,
};

/* Names only: what a descriptor built without generated code carries. */
static const rclc_service_type_t SRV_TYPE_NO_CODEC = {
    .type_name = "example_interfaces::srv::dds_::Double_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

static void cdr_int32(uint8_t out[8], int32_t v) {
    const uint8_t header[4] = {0x00, 0x01, 0x00, 0x00};
    memcpy(out, header, 4);
    memcpy(out + 4, &v, 4);
}

static int32_t sent_int32(const char* service, int64_t* seq) {
    size_t len = 0;
    const uint8_t* sent = nros_stub_rmw_last_response(service, seq, &len);
    int32_t v = 0;
    if (sent == NULL || len != 8) return INT32_MIN;
    memcpy(&v, sent + 4, 4);
    return v;
}

static int32_msg_t s_req_plain, s_resp_plain;
static int32_msg_t s_req_id, s_resp_id;
static int32_msg_t s_req_ctx, s_resp_ctx;
static int32_msg_t s_client_resp;
static int s_plain_calls = 0, s_id_calls = 0, s_ctx_calls = 0, s_client_calls = 0;
static int64_t s_seen_seq = -1;
static void* s_seen_ctx = NULL;
static const void* s_client_msgin = NULL;
static int32_t s_client_seen = 0;

static void on_plain(const void* req, void* resp) {
    s_plain_calls++;
    ((int32_msg_t*)resp)->data = ((const int32_msg_t*)req)->data * 2;
}

static void on_with_id(const void* req, rmw_request_id_t* id, void* resp) {
    s_id_calls++;
    s_seen_seq = id != NULL ? id->sequence_number : -1;
    ((int32_msg_t*)resp)->data = ((const int32_msg_t*)req)->data + 1;
}

static void on_with_ctx(const void* req, void* resp, void* context) {
    s_ctx_calls++;
    s_seen_ctx = context;
    ((int32_msg_t*)resp)->data = -((const int32_msg_t*)req)->data;
}

static int s_client_id_calls = 0;
static int32_t s_client_id_seen = 0;
static rmw_request_id_t s_client_id;

static void on_reply_with_id(const void* msgin, rmw_request_id_t* request_id) {
    s_client_id_calls++;
    s_client_id_seen = ((const int32_msg_t*)msgin)->data;
    s_client_id = *request_id;
}

static void on_reply(const void* msgin) {
    s_client_calls++;
    s_client_msgin = msgin;
    s_client_seen = ((const int32_msg_t*)msgin)->data;
}

static void pump(rclc_executor_t* executor) {
    for (int i = 0; i < 4; i++)
        (void)rclc_executor_spin_some(executor, 0);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);
    /* Staged requests have no arrival callback: the executor must poll. */
    nros_stub_rmw_set_entity_wake_supported(false);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 52, "typed_service_client",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 8, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "typed", &opts), RCL_RET_OK,
              "node bound to the executor");

    rcl_service_t plain = rcl_get_zero_initialized_service();
    rcl_service_t with_id = rcl_get_zero_initialized_service();
    rcl_service_t with_ctx = rcl_get_zero_initialized_service();
    rcl_service_t no_codec = rcl_get_zero_initialized_service();
    CHECK_RET(rclc_service_init_default(&plain, &node, &SRV_TYPE, "/double"), RCL_RET_OK,
              "plain service created");
    CHECK_RET(rclc_service_init_default(&with_id, &node, &SRV_TYPE, "/increment"), RCL_RET_OK,
              "request-id service created");
    CHECK_RET(rclc_service_init_default(&with_ctx, &node, &SRV_TYPE, "/negate"), RCL_RET_OK,
              "context service created");
    CHECK_RET(rclc_service_init_default(&no_codec, &node, &SRV_TYPE_NO_CODEC, "/names_only"),
              RCL_RET_OK, "codec-less service created");

    /* rclc's signatures, taken as pointers so a drift fails to compile. */
    rcl_ret_t (*add_service)(rclc_executor_t*, rcl_service_t*, void*, void*,
                             rclc_service_callback_t) = rclc_executor_add_service;
    rcl_ret_t (*add_service_id)(rclc_executor_t*, rcl_service_t*, void*, void*,
                                rclc_service_callback_with_request_id_t) =
        rclc_executor_add_service_with_request_id;
    rcl_ret_t (*add_service_ctx)(rclc_executor_t*, rcl_service_t*, void*, void*,
                                 rclc_service_callback_with_context_t, void*) =
        rclc_executor_add_service_with_context;
    rcl_ret_t (*add_client)(rclc_executor_t*, rcl_client_t*, void*, rclc_client_callback_t) =
        rclc_executor_add_client;

    /* 4. Refusals. */
    CHECK_RET(add_service(&executor, &plain, &s_req_plain, &s_resp_plain, NULL),
              RCL_RET_INVALID_ARGUMENT, "a NULL callback is refused");
    CHECK_RET(add_service(&executor, &plain, NULL, &s_resp_plain, on_plain),
              RCL_RET_INVALID_ARGUMENT, "a NULL request message is refused");
    CHECK_RET(add_service(&executor, &no_codec, &s_req_plain, &s_resp_plain, on_plain),
              RCL_RET_UNSUPPORTED, "a descriptor with no codec is UNSUPPORTED");

    CHECK_RET(add_service(&executor, &plain, &s_req_plain, &s_resp_plain, on_plain), RCL_RET_OK,
              "typed service registered");
    CHECK_RET(add_service_id(&executor, &with_id, &s_req_id, &s_resp_id, on_with_id), RCL_RET_OK,
              "typed request-id service registered");
    CHECK_RET(
        add_service_ctx(&executor, &with_ctx, &s_req_ctx, &s_resp_ctx, on_with_ctx, &s_ctx_calls),
        RCL_RET_OK, "typed context service registered");
    if (s_failures != 0) return 1;

    /* 1 + 2. One request to each service. */
    uint8_t frame[8];
    cdr_int32(frame, 21);
    CHECK(nros_stub_rmw_stage_request("/double", 3, frame, sizeof frame), "stage /double");
    cdr_int32(frame, 41);
    nros_stub_service_info_t info;
    memset(&info, 0, sizeof(info));
    /* A nonzero GUID: an all-zero id reads as "the backend reported none". */
    for (int i = 0; i < 16; i++)
        info.writer_guid[i] = (int8_t)(i + 1);
    info.sequence_number = 777;
    CHECK(nros_stub_rmw_stage_request_info("/increment", 4, frame, sizeof frame, &info),
          "stage /increment with an id");
    cdr_int32(frame, 9);
    CHECK(nros_stub_rmw_stage_request("/negate", 5, frame, sizeof frame), "stage /negate");
    pump(&executor);

    int64_t seq = -1;
    CHECK(s_plain_calls == 1, "the plain callback ran once");
    CHECK(s_req_plain.data == 21, "the request was deserialised into the caller's storage");
    CHECK(sent_int32("/double", &seq) == 42 && seq == 3,
          "the caller's response went out serialised, against the request's token");
    CHECK(s_id_calls == 1, "the request-id callback ran once");
    CHECK(s_seen_seq == 777, "the request-id callback sees the staged id");
    CHECK(sent_int32("/increment", &seq) == 42 && seq == 4, "the request-id response went out");
    CHECK(s_ctx_calls == 1, "the context callback ran once");
    CHECK(s_seen_ctx == &s_ctx_calls, "the context word is passed through");
    CHECK(sent_int32("/negate", &seq) == -9 && seq == 5, "the context response went out");

    /* 3. A typed client. */
    rcl_client_t client = rcl_get_zero_initialized_client();
    rcl_client_t client_no_codec = rcl_get_zero_initialized_client();
    CHECK_RET(rclc_client_init_default(&client, &node, &SRV_TYPE, "/double_client"), RCL_RET_OK,
              "client created");
    CHECK_RET(rclc_client_init_default(&client_no_codec, &node, &SRV_TYPE_NO_CODEC, "/nc_client"),
              RCL_RET_OK, "codec-less client created");
    CHECK_RET(add_client(&executor, &client, &s_client_resp, NULL), RCL_RET_INVALID_ARGUMENT,
              "a NULL client callback is refused");
    CHECK_RET(add_client(&executor, &client_no_codec, &s_client_resp, on_reply),
              RCL_RET_UNSUPPORTED, "a client descriptor with no codec is UNSUPPORTED");
    CHECK_RET(add_client(&executor, &client, &s_client_resp, on_reply), RCL_RET_OK,
              "typed client registered");

    nros_stub_rmw_reset_client();
    cdr_int32(frame, 5);
    CHECK_RET(rclc_client_send_request_async(&client, frame, sizeof frame), RCL_RET_OK,
              "request sent");
    cdr_int32(frame, 10);
    CHECK(nros_stub_rmw_stage_reply_for(nros_stub_rmw_last_request_seq(), frame, sizeof frame),
          "stage the reply");
    pump(&executor);
    CHECK(s_client_calls == 1, "the client callback ran once");
    CHECK(s_client_msgin == &s_client_resp, "msgin is the caller's own storage");
    CHECK(s_client_seen == 10, "the reply was deserialised into it");

    /* 5. Issue 2136 — a typed request-id client. The executor holds four
     * handles, all taken: retire the plain service to make room. */
    CHECK_RET(rclc_executor_remove_service(&executor, &plain), RCL_RET_OK,
              "the plain service is removed to make room");
    rcl_ret_t (*add_client_id)(rclc_executor_t*, rcl_client_t*, void*,
                               rclc_client_callback_with_request_id_t) =
        rclc_executor_add_client_with_request_id;
    rcl_client_t client_id = rcl_get_zero_initialized_client();
    static int32_msg_t s_client_id_resp;
    CHECK_RET(rclc_client_init_default(&client_id, &node, &SRV_TYPE, "/id_client"), RCL_RET_OK,
              "request-id client created");
    CHECK_RET(add_client_id(&executor, &client_id, &s_client_id_resp, NULL),
              RCL_RET_INVALID_ARGUMENT, "a NULL request-id client callback is refused");
    CHECK_RET(add_client_id(&executor, &client_id, &s_client_id_resp, on_reply_with_id),
              RCL_RET_OK, "typed request-id client registered");

    nros_stub_rmw_reset_client();
    int8_t guid[16];
    for (int i = 0; i < 16; i++)
        guid[i] = (int8_t)(0x40 + i);
    nros_stub_rmw_set_reply_writer_guid(guid);
    cdr_int32(frame, 6);
    CHECK_RET(rclc_client_send_request_async(&client_id, frame, sizeof frame), RCL_RET_OK,
              "request-id client request sent");
    const int64_t sent_seq = nros_stub_rmw_last_request_seq();
    cdr_int32(frame, 12);
    CHECK(nros_stub_rmw_stage_reply_for(sent_seq, frame, sizeof frame), "stage the id reply");
    pump(&executor);
    CHECK(s_client_id_calls == 1, "the request-id client callback ran once");
    CHECK(s_client_id_seen == 12, "the id reply was deserialised");
    CHECK(s_client_id.sequence_number == sent_seq,
          "the request id carries the sequence the reply was matched on");
    CHECK(memcmp(s_client_id.writer_guid, guid, sizeof(guid)) == 0,
          "the request id carries the writer GUID the backend reported");

    /* A backend that reports no GUID: the sequence still arrives, the GUID is
     * all zero ("not reported"). */
    nros_stub_rmw_reset_client();
    cdr_int32(frame, 7);
    CHECK_RET(rclc_client_send_request_async(&client_id, frame, sizeof frame), RCL_RET_OK,
              "second request-id client request sent");
    const int64_t sent_seq2 = nros_stub_rmw_last_request_seq();
    cdr_int32(frame, 14);
    CHECK(nros_stub_rmw_stage_reply_for(sent_seq2, frame, sizeof frame), "stage the 2nd reply");
    pump(&executor);
    int8_t zero[16] = {0};
    CHECK(s_client_id_calls == 2 && s_client_id_seen == 14, "the second reply reached it");
    CHECK(s_client_id.sequence_number == sent_seq2 &&
              memcmp(s_client_id.writer_guid, zero, sizeof(zero)) == 0,
          "no reported GUID reads all zero, the sequence still set");

    if (s_failures != 0) {
        fprintf(stderr, "typed_service_client: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("typed_service_client: OK\n");
    return 0;
}
