/* See stub_rmw_backend.h for why this exists. */

#include "stub_rmw_backend.h"

#include <nros/rmw_vtable.h>

#include <stdbool.h>
#include <stddef.h>
#include <string.h>
#include <time.h>

/* Issue 1405 — one marker PER SESSION, handed out in open order, so an entity
 * create can say which session it landed on. A single shared marker made every
 * session look like every other one, and a registration that picked the wrong
 * session was invisible. Wraps, so a long-lived process never runs off the end;
 * the index is only meaningful within one probe. */
#define STUB_MAX_SESSIONS 16
static int s_sessions[STUB_MAX_SESSIONS];
static uint32_t s_sessions_opened = 0;
static int32_t s_last_entity_session = -1;
static uint32_t s_drive_io_calls = 0;

/* The last name an entity slot was asked to create under. Recorded even though
 * every slot REFUSES, because the refusal happens after the name has been
 * resolved: this is the only place a test can read the wire name the runtime
 * computed, rather than the source name it passed in. */
static char s_last_entity_name[256] = {0};

static void record_entity_session(const rmw_node_t* node) {
    s_last_entity_session = -1;
    if (node == NULL || node->session == NULL) return;
    const int* marker = (const int*)node->session->backend_data;
    if (marker >= &s_sessions[0] && marker < &s_sessions[STUB_MAX_SESSIONS]) {
        s_last_entity_session = (int32_t)(marker - &s_sessions[0]);
    }
}

int32_t nros_stub_rmw_last_entity_session(void) {
    return s_last_entity_session;
}

uint32_t nros_stub_rmw_sessions_opened(void) {
    return s_sessions_opened;
}

static void record_entity_name(const char* name) {
    if (name == NULL) {
        s_last_entity_name[0] = '\0';
        return;
    }
    size_t n = strlen(name);
    if (n >= sizeof(s_last_entity_name)) n = sizeof(s_last_entity_name) - 1;
    memcpy(s_last_entity_name, name, n);
    s_last_entity_name[n] = '\0';
}

const char* nros_stub_rmw_last_entity_name(void) {
    return s_last_entity_name;
}

void nros_stub_rmw_clear_last_entity_name(void) {
    s_last_entity_name[0] = '\0';
}

/* Issue 1609 — the ACCEPT mode. Off by default, so every existing probe still
 * gets the loud refusal it was written against. On, every `create_*` succeeds
 * with a handle naming `s_entity_token` and counts it live, and the matching
 * `destroy_*` counts it dead: so a probe can watch an entity LEAVE THE GRAPH,
 * which is the half of "release" that no executor-side number shows. */
static bool s_accept_entities = false;
static int s_entity_token = 0;
static int32_t s_live_entities = 0;

void nros_stub_rmw_set_accept_entities(bool accept) {
    s_accept_entities = accept;
}

int32_t nros_stub_rmw_live_entities(void) {
    return s_live_entities;
}

/* The create half of the accept mode: `true` (and the handle filled) when the
 * stub is accepting, `false` to fall through to the refusal. */
static bool accept_create(void** backend_data) {
    if (!s_accept_entities) return false;
    *backend_data = &s_entity_token;
    s_live_entities++;
    return true;
}

/* The destroy half: only a handle THIS mode minted is counted, so a refusal
 * path's destroy can never drive the count below what was created. */
static bool accept_destroy(void* backend_data) {
    if (backend_data != &s_entity_token) return false;
    s_live_entities--;
    return true;
}

/* ---- Session lifecycle: the only slots that succeed outside accept mode -- */

static rmw_ret_t stub_create_session(const char* locator, uint8_t mode, uint32_t domain_id,
                                     const char* node_name, const rmw_session_options_t* options,
                                     rmw_session_t* out) {
    (void)locator;
    (void)mode;
    (void)domain_id;
    (void)node_name;
    (void)options;
    /* The runtime treats a NULL `backend_data` as "no session"; anything
     * non-NULL and stable will do, and nothing dereferences it here. */
    out->backend_data = &s_sessions[s_sessions_opened % STUB_MAX_SESSIONS];
    s_sessions_opened++;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_destroy_session(rmw_session_t* session) {
    (void)session;
    return NROS_RMW_RET_OK;
}

/* Honours the timeout by sleeping it out, which is what a real backend's
 * blocking read does and is what makes WALL TIME pass in a spin loop. A stub
 * that returned instantly would make every steady-clock timer in a probe look
 * like it never fires — the executor's spin delta is real time, and a loop
 * over a non-blocking drive_io does not spend any. */
static rmw_ret_t stub_drive_io(rmw_session_t* session, int32_t timeout_ms) {
    (void)session;
    s_drive_io_calls++;
    if (timeout_ms > 0) {
        struct timespec req;
        req.tv_sec = (time_t)(timeout_ms / 1000);
        req.tv_nsec = (long)(timeout_ms % 1000) * 1000000L;
        (void)nanosleep(&req, NULL);
    }
    return NROS_RMW_RET_OK;
}

/* ---- Entities: present so registration succeeds, refusing so a test that
 *      strays past this backend's purpose fails loudly ---------------------- */

/* issue 1975 — the profile the last publisher was created with, echoed back
 * as its GRANTED QoS. The cffi layer reads `publisher_get_actual_qos` once,
 * right after create, so "the last one" is the one being asked about. This is
 * what a backend that grants every request reports, which lets a probe check
 * an accessor over the granted profile (`is_durability_transient_local`) on a
 * known answer rather than on the all-Unknown profile of a NULL slot. */
static rmw_qos_profile_t s_last_publisher_qos;

static rmw_ret_t
stub_create_publisher(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                      const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                      const rmw_publisher_options_t* options, rmw_publisher_t* out) {
    record_entity_name(topic_name);
    record_entity_session(node);
    (void)node;
    (void)type_support;
    (void)domain_id;
    (void)options;
    (void)out;
    if (accept_create(&out->backend_data)) {
        if (qos != NULL) s_last_publisher_qos = *qos;
        return NROS_RMW_RET_OK;
    }
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_publisher_get_actual_qos(const rmw_publisher_t* publisher,
                                               rmw_qos_profile_t* qos) {
    (void)publisher;
    if (qos == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    *qos = s_last_publisher_qos;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_destroy_publisher(rmw_publisher_t* publisher) {
    (void)publisher;
    if (publisher != NULL && accept_destroy(publisher->backend_data)) return NROS_RMW_RET_OK;
    return NROS_RMW_RET_UNSUPPORTED;
}

/* phase-492 W8 — in ACCEPT mode a publish succeeds and its bytes are kept, so
 * a C probe can compare what `rcl_publish` put on the "wire" with what the
 * message's own serialiser produces. Outside accept mode it still refuses. */
static uint8_t s_last_payload[1024];
static size_t s_last_payload_len = 0;
static uint32_t s_publish_count = 0;

static rmw_ret_t stub_publish(const rmw_publisher_t* publisher, rmw_byte_span_t payload) {
    (void)publisher;
    if (!s_accept_entities) return NROS_RMW_RET_UNSUPPORTED;
    if (payload.len > sizeof(s_last_payload)) return NROS_RMW_RET_ERROR;
    memcpy(s_last_payload, payload.data, payload.len);
    s_last_payload_len = payload.len;
    s_publish_count++;
    return NROS_RMW_RET_OK;
}

const uint8_t* nros_stub_rmw_last_payload(size_t* len) {
    if (len != NULL) *len = s_last_payload_len;
    return s_last_payload;
}

uint32_t nros_stub_rmw_publish_count(void) {
    return s_publish_count;
}

/* Issue 2015 — action-server / subscription staging.
 *
 * Each service server and subscription created in accept mode gets its OWN
 * token carrying its name, so a test can stage a request (or a sample) for one
 * entity by the TAIL of its name (`_action/send_goal`, `_action/feedback`) and
 * read back what a server answered. */
#define STUB_NAMED_TOKENS 64
typedef struct {
    char name[128];
} stub_named_t;
static stub_named_t s_named[STUB_NAMED_TOKENS];
static size_t s_next_named = 0;

static bool is_named_token(const void* p) {
    return p >= (const void*)&s_named[0] && p < (const void*)&s_named[STUB_NAMED_TOKENS];
}

static bool accept_named(void** backend_data, const char* name) {
    if (!s_accept_entities) return false;
    stub_named_t* t = &s_named[s_next_named % STUB_NAMED_TOKENS];
    ++s_next_named;
    size_t n = name != NULL ? strlen(name) : 0;
    if (n >= sizeof(t->name)) n = sizeof(t->name) - 1;
    if (n > 0) memcpy(t->name, name, n);
    t->name[n] = '\0';
    *backend_data = t;
    s_live_entities++;
    return true;
}

static bool destroy_named(void* backend_data) {
    if (!is_named_token(backend_data)) return false;
    s_live_entities--;
    return true;
}

static bool name_ends_with(const void* token, const char* suffix) {
    if (!is_named_token(token) || suffix == NULL) return false;
    const char* name = ((const stub_named_t*)token)->name;
    size_t n = strlen(name), m = strlen(suffix);
    return m <= n && memcmp(name + n - m, suffix, m) == 0;
}

#define STUB_STAGED_INBOUND 16
typedef struct {
    char suffix[64];
    int64_t seq;
    uint8_t data[512];
    size_t len;
    /* Issue 2036 — the metadata `take_with_info` reports for this sample. */
    bool has_info;
    nros_stub_message_info_t info;
    /* Issue 2081 — the metadata `take_request_with_info` reports for this
     * request. */
    bool has_service_info;
    nros_stub_service_info_t service_info;
} stub_inbound_t;
static stub_inbound_t s_inbound[STUB_STAGED_INBOUND];
static size_t s_inbound_count = 0;

static bool stage_inbound(const char* suffix, int64_t seq, const uint8_t* data, size_t len) {
    if (s_inbound_count >= STUB_STAGED_INBOUND || suffix == NULL) return false;
    if (len > sizeof(s_inbound[0].data) || (len > 0 && data == NULL)) return false;
    if (strlen(suffix) >= sizeof(s_inbound[0].suffix)) return false;
    stub_inbound_t* r = &s_inbound[s_inbound_count];
    strcpy(r->suffix, suffix);
    r->seq = seq;
    if (len > 0) memcpy(r->data, data, len);
    r->len = len;
    r->has_info = false;
    memset(&r->info, 0, sizeof(r->info));
    r->has_service_info = false;
    memset(&r->service_info, 0, sizeof(r->service_info));
    ++s_inbound_count;
    return true;
}

/* The oldest staged entry for `token`'s name, or `s_inbound_count`. */
static size_t inbound_for(const void* token) {
    for (size_t i = 0; i < s_inbound_count; ++i) {
        if (name_ends_with(token, s_inbound[i].suffix)) return i;
    }
    return s_inbound_count;
}

static rmw_ret_t take_inbound_any(const void* token, rmw_mut_byte_span_t* out, int64_t* seq_out,
                                  bool* taken, rmw_message_info_t* info_out,
                                  rmw_service_info_t* service_info_out) {
    if (taken != NULL) *taken = false;
    size_t at = inbound_for(token);
    if (at == s_inbound_count) return NROS_RMW_RET_OK;
    if (out == NULL || out->data == NULL) return NROS_RMW_RET_ERROR;
    const stub_inbound_t* r = &s_inbound[at];
    if (out->capacity < r->len) {
        out->len = r->len;
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    memcpy(out->data, r->data, r->len);
    out->len = r->len;
    if (seq_out != NULL) *seq_out = r->seq;
    if (info_out != NULL) {
        memset(info_out, 0, sizeof(*info_out));
        if (r->has_info) {
            info_out->source_timestamp = r->info.source_timestamp;
            info_out->received_timestamp = r->info.received_timestamp;
            info_out->publication_sequence_number = r->info.publication_sequence_number;
            info_out->reception_sequence_number = r->info.reception_sequence_number;
            memcpy(info_out->publisher_gid.data, r->info.publisher_gid,
                   sizeof(r->info.publisher_gid));
        }
    }
    if (service_info_out != NULL) {
        memset(service_info_out, 0, sizeof(*service_info_out));
        if (r->has_service_info) {
            service_info_out->source_timestamp = r->service_info.source_timestamp;
            service_info_out->received_timestamp = r->service_info.received_timestamp;
            memcpy(service_info_out->request_id.writer_guid, r->service_info.writer_guid,
                   sizeof(r->service_info.writer_guid));
            service_info_out->request_id.sequence_number = r->service_info.sequence_number;
        }
    }
    if (taken != NULL) *taken = true;
    memmove(&s_inbound[at], &s_inbound[at + 1], (s_inbound_count - at - 1) * sizeof(stub_inbound_t));
    --s_inbound_count;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t take_inbound_info(const void* token, rmw_mut_byte_span_t* out, int64_t* seq_out,
                                   bool* taken, rmw_message_info_t* info_out) {
    return take_inbound_any(token, out, seq_out, taken, info_out, NULL);
}

static rmw_ret_t take_inbound(const void* token, rmw_mut_byte_span_t* out, int64_t* seq_out,
                              bool* taken) {
    return take_inbound_any(token, out, seq_out, taken, NULL, NULL);
}

bool nros_stub_rmw_stage_request(const char* service_suffix, int64_t seq, const uint8_t* data,
                                 size_t len) {
    return stage_inbound(service_suffix, seq, data, len);
}

bool nros_stub_rmw_stage_request_info(const char* service_suffix, int64_t seq,
                                      const uint8_t* data, size_t len,
                                      const nros_stub_service_info_t* info) {
    if (!stage_inbound(service_suffix, seq, data, len)) return false;
    if (info != NULL) {
        s_inbound[s_inbound_count - 1].has_service_info = true;
        s_inbound[s_inbound_count - 1].service_info = *info;
    }
    return true;
}

bool nros_stub_rmw_stage_sample(const char* topic_suffix, const uint8_t* data, size_t len) {
    return stage_inbound(topic_suffix, 0, data, len);
}

bool nros_stub_rmw_stage_sample_info(const char* topic_suffix, const uint8_t* data, size_t len,
                                     const nros_stub_message_info_t* info) {
    if (!stage_inbound(topic_suffix, 0, data, len)) return false;
    if (info != NULL) {
        s_inbound[s_inbound_count - 1].has_info = true;
        s_inbound[s_inbound_count - 1].info = *info;
    }
    return true;
}

/* The last response a server sent, per service-name tail. */
#define STUB_RESPONSE_LOG 8
typedef struct {
    char name[128];
    int64_t seq;
    uint8_t data[512];
    size_t len;
} stub_response_t;
static stub_response_t s_responses[STUB_RESPONSE_LOG];
static size_t s_response_next = 0;

const uint8_t* nros_stub_rmw_last_response(const char* service_suffix, int64_t* seq, size_t* len) {
    /* newest first */
    for (size_t k = 0; k < STUB_RESPONSE_LOG && k < s_response_next; ++k) {
        stub_response_t* r = &s_responses[(s_response_next - 1 - k) % STUB_RESPONSE_LOG];
        size_t n = strlen(r->name), m = strlen(service_suffix);
        if (m <= n && memcmp(r->name + n - m, service_suffix, m) == 0) {
            if (seq != NULL) *seq = r->seq;
            if (len != NULL) *len = r->len;
            return r->data;
        }
    }
    return NULL;
}

static rmw_ret_t
stub_create_subscription(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                         const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                         const rmw_subscription_options_t* options, rmw_subscription_t* out) {
    record_entity_name(topic_name);
    record_entity_session(node);
    (void)node;
    (void)type_support;
    (void)domain_id;
    (void)qos;
    (void)options;
    if (accept_named(&out->backend_data, topic_name)) return NROS_RMW_RET_OK;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_subscription(rmw_subscription_t* subscription) {
    if (subscription != NULL && destroy_named(subscription->backend_data)) return NROS_RMW_RET_OK;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_take(const rmw_subscription_t* subscription, rmw_mut_byte_span_t* out,
                           bool* taken) {
    /* Issue 2015 — a sample staged for this subscription's topic tail. */
    return take_inbound(subscription != NULL ? subscription->backend_data : NULL, out, NULL, taken);
}

/* Issue 2036 — `take` plus the metadata staged with the sample (all zero
 * when it was staged without any). */
static rmw_ret_t stub_take_with_info(const rmw_subscription_t* subscription,
                                     rmw_mut_byte_span_t* out, bool* taken,
                                     rmw_message_info_t* message_info) {
    return take_inbound_info(subscription != NULL ? subscription->backend_data : NULL, out, NULL,
                             taken, message_info);
}

/* Issue 2060 — `has_data` is where a real backend polls a subscription's
 * status events (zenoh's deadline / liveliness / matched walk, Cyclone's
 * `take_event` drain). The stub counts the calls and, when a probe staged
 * one, fires a count event from inside the call, exactly as those backends
 * do. Defined below with the other status-event state. */
static void stub_poll_status_from_has_data(void);
static uint32_t s_sub_has_data_calls = 0;

uint32_t nros_stub_rmw_subscription_has_data_calls(void) {
    return s_sub_has_data_calls;
}

static rmw_ret_t stub_has_data(rmw_subscription_t* subscription, bool* out_has_data) {
    s_sub_has_data_calls++;
    stub_poll_status_from_has_data();
    if (out_has_data != NULL) {
        *out_has_data = subscription != NULL && inbound_for(subscription->backend_data) != s_inbound_count;
    }
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_create_service(const rmw_node_t* node,
                                     const rmw_service_type_support_t* type_support,
                                     const char* service_name, uint32_t domain_id,
                                     const rmw_qos_profile_t* qos, rmw_service_t* out) {
    record_entity_name(service_name);
    record_entity_session(node);
    (void)node;
    (void)type_support;
    (void)domain_id;
    (void)qos;
    if (accept_named(&out->backend_data, service_name)) return NROS_RMW_RET_OK;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_service(rmw_service_t* server) {
    if (server != NULL && destroy_named(server->backend_data)) return NROS_RMW_RET_OK;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_take_request(const rmw_service_t* server, rmw_mut_byte_span_t* request,
                                   int64_t* seq_out, bool* taken) {
    /* Issue 2015 — a request staged for this server's service-name tail. */
    return take_inbound(server != NULL ? server->backend_data : NULL, request, seq_out, taken);
}

/* Issue 2081 — `take_request` plus the metadata staged with the request (all
 * zero, "not reported", when it was staged without any). */
static rmw_ret_t stub_take_request_with_info(const rmw_service_t* server,
                                             rmw_mut_byte_span_t* request, int64_t* seq_out,
                                             bool* taken, rmw_service_info_t* service_info) {
    if (service_info == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    return take_inbound_any(server != NULL ? server->backend_data : NULL, request, seq_out, taken,
                            NULL, service_info);
}

static rmw_ret_t stub_has_request(rmw_service_t* server, bool* out_has_request) {
    if (out_has_request != NULL) {
        *out_has_request = server != NULL && inbound_for(server->backend_data) != s_inbound_count;
    }
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_send_response(const rmw_service_t* server, int64_t seq,
                                    rmw_byte_span_t response) {
    /* Issue 2015 — kept per server name, for `nros_stub_rmw_last_response`. */
    if (server == NULL || !is_named_token(server->backend_data)) return NROS_RMW_RET_UNSUPPORTED;
    if (response.len > sizeof(s_responses[0].data)) return NROS_RMW_RET_ERROR;
    stub_response_t* r = &s_responses[s_response_next % STUB_RESPONSE_LOG];
    ++s_response_next;
    strcpy(r->name, ((const stub_named_t*)server->backend_data)->name);
    r->seq = seq;
    if (response.len > 0) memcpy(r->data, response.data, response.len);
    r->len = response.len;
    return NROS_RMW_RET_OK;
}

/* Issue 2011 — each client created in accept mode gets its OWN token, so a
 * reply staged for a request is taken only by the client that sent it (an
 * action client is three service clients, and a process may hold several). */
#define STUB_CLIENT_TOKENS 64
static int s_client_tokens[STUB_CLIENT_TOKENS];
static size_t s_next_client_token = 0;

static bool is_client_token(const void* p) {
    return p >= (const void*)&s_client_tokens[0] &&
           p < (const void*)&s_client_tokens[STUB_CLIENT_TOKENS];
}

static rmw_ret_t stub_create_client(const rmw_node_t* node,
                                    const rmw_service_type_support_t* type_support,
                                    const char* service_name, uint32_t domain_id,
                                    const rmw_qos_profile_t* qos, rmw_client_t* out) {
    record_entity_name(service_name);
    record_entity_session(node);
    (void)node;
    (void)type_support;
    (void)domain_id;
    (void)qos;
    if (accept_create(&out->backend_data)) {
        out->backend_data = &s_client_tokens[s_next_client_token % STUB_CLIENT_TOKENS];
        ++s_next_client_token;
        return NROS_RMW_RET_OK;
    }
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_client(rmw_client_t* client) {
    if (client != NULL && is_client_token(client->backend_data)) {
        s_live_entities--;
        return NROS_RMW_RET_OK;
    }
    return NROS_RMW_RET_UNSUPPORTED;
}

/* Issue 1996 — the client request/reply pair, so a test can drive a C++
 * `rclcpp::Future` through the real runtime: `send_request` counts the call and
 * hands out increasing sequence ids; `take_response` yields the replies a test
 * STAGED, oldest first, each reporting the id it was staged for. Nothing staged
 * reads as "no reply yet". Before this pair both slots were NULL, which the
 * runtime reports as an unsupported send.
 *
 * Issue 2009 — staged BY ID (`nros_stub_rmw_stage_reply_for`), in any order, so
 * a test can deliver replies out of order and after a request was removed; an
 * outstanding-request cap (`nros_stub_rmw_set_max_outstanding`), refused the
 * way a real backend refuses a send past its own limit; and an id-less mode
 * (`nros_stub_rmw_set_ids_supported(false)`) in which neither slot writes an id,
 * the backend `NROS_RMW_SEQUENCE_ID_UNKNOWN` exists for. */
#define STUB_REPLY_SLOTS 8
typedef struct {
    uint8_t data[256];
    size_t len;
    int64_t seq;
} stub_reply_t;
static stub_reply_t s_replies[STUB_REPLY_SLOTS];
static size_t s_reply_count = 0;
static int64_t s_next_request_seq = 1;
static uint32_t s_requests_sent = 0;
static uint32_t s_max_outstanding = 0; /* 0 = no cap */
static uint32_t s_outstanding = 0;
static bool s_ids_supported = true;

/* Issue 2011 — which client sent each request, so `take_response` hands a
 * reply only to its own client. A ring: an id older than the last 64 requests
 * (or one never sent) has no owner, and any client may take its reply. */
#define STUB_SENT_LOG 64
typedef struct {
    int64_t seq;
    const void* owner;
} stub_sent_t;
static stub_sent_t s_sent[STUB_SENT_LOG];
static size_t s_sent_next = 0;

static const void* request_owner(int64_t seq) {
    for (size_t i = 0; i < STUB_SENT_LOG; ++i) {
        if (s_sent[i].owner != NULL && s_sent[i].seq == seq) return s_sent[i].owner;
    }
    return NULL;
}

static rmw_ret_t stub_send_request(const rmw_client_t* client, rmw_byte_span_t request,
                                   int64_t* sequence_id) {
    (void)request;
    if (s_max_outstanding != 0 && s_outstanding >= s_max_outstanding) {
        /* A real backend's own limit: zenoh's pending-get table, Cyclone's
         * per-client outstanding set. Nothing is sent. */
        return NROS_RMW_RET_WOULD_BLOCK;
    }
    ++s_requests_sent;
    ++s_outstanding;
    s_sent[s_sent_next % STUB_SENT_LOG].seq = s_next_request_seq;
    s_sent[s_sent_next % STUB_SENT_LOG].owner = client != NULL ? client->backend_data : NULL;
    ++s_sent_next;
    if (sequence_id != NULL && s_ids_supported) *sequence_id = s_next_request_seq;
    ++s_next_request_seq;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_take_response(const rmw_client_t* client, rmw_mut_byte_span_t* reply,
                                    int64_t* seq_out, bool* taken) {
    if (taken != NULL) *taken = false;
    /* Issue 2011 — the oldest staged reply THIS client may take: one for a
     * request it sent, or one whose request has no recorded owner. */
    size_t at = s_reply_count;
    for (size_t i = 0; i < s_reply_count; ++i) {
        const void* owner = request_owner(s_replies[i].seq);
        if (owner == NULL || client == NULL || owner == client->backend_data) {
            at = i;
            break;
        }
    }
    if (at == s_reply_count) return NROS_RMW_RET_OK;
    if (reply == NULL || reply->data == NULL) return NROS_RMW_RET_ERROR;
    const stub_reply_t* r = &s_replies[at];
    if (reply->capacity < r->len) {
        reply->len = r->len;
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    memcpy(reply->data, r->data, r->len);
    reply->len = r->len;
    if (seq_out != NULL && s_ids_supported) *seq_out = r->seq;
    if (taken != NULL) *taken = true;
    memmove(&s_replies[at], &s_replies[at + 1], (s_reply_count - at - 1) * sizeof(stub_reply_t));
    --s_reply_count;
    if (s_outstanding > 0) --s_outstanding;
    return NROS_RMW_RET_OK;
}

bool nros_stub_rmw_stage_reply_for(int64_t seq, const uint8_t* data, size_t len) {
    if (s_reply_count >= STUB_REPLY_SLOTS) return false;
    if (len > sizeof(s_replies[0].data) || (len > 0 && data == NULL)) return false;
    stub_reply_t* r = &s_replies[s_reply_count];
    if (len > 0) memcpy(r->data, data, len);
    r->len = len;
    r->seq = seq;
    ++s_reply_count;
    return true;
}

bool nros_stub_rmw_stage_reply(const uint8_t* data, size_t len) {
    return nros_stub_rmw_stage_reply_for(s_next_request_seq - 1, data, len);
}

int64_t nros_stub_rmw_last_request_seq(void) {
    return s_next_request_seq - 1;
}

void nros_stub_rmw_set_max_outstanding(uint32_t max) {
    s_max_outstanding = max;
}

void nros_stub_rmw_set_ids_supported(bool supported) {
    s_ids_supported = supported;
}

size_t nros_stub_rmw_staged_replies(void) {
    return s_reply_count;
}

void nros_stub_rmw_reset_client(void) {
    s_inbound_count = 0;
    s_response_next = 0;
    s_reply_count = 0;
    s_outstanding = 0;
    s_max_outstanding = 0;
    s_ids_supported = true;
}

uint32_t nros_stub_rmw_requests_sent(void) {
    return s_requests_sent;
}

/* Phase 124.B.1's wake slot, stored and never called — and the DIFFERENCE that
 * makes is the point (phase-417 W4.e).
 *
 * The runtime asks `supports_wake_callback()`, which is literally "is this slot
 * non-NULL", and a backend that answers NO gets `drive_io(full timeout)`: the
 * executor parks inside the transport, where nothing can reach it. A backend
 * that answers YES gets `node_wake.wait_ms(timeout)` + `drive_io(0)`, and that
 * wait is interruptible — which is what makes a guard condition triggered from
 * another thread WAKE a spin rather than wait it out.
 *
 * So this slot is not a formality: without it `node_guard_condition.c` cannot
 * measure the wake at all, and with it the wall time a spin spends simply moves
 * from the stub's `nanosleep` into the platform's wake primitive, which is
 * where a real async backend spends it too.
 *
 * Issue 1385 — the slot is also READ BACK, by
 * `nros_stub_rmw_wake_cb_installed`, and driven on demand by
 * `nros_stub_rmw_invoke_wake`. Storing a callback nobody can see and nobody
 * can fire made the executor's half of this contract untestable in both
 * directions: whether an install happened at all, and whether the teardown
 * handed it back. */
static void (*s_wake_cb)(void* ctx) = NULL;
static void* s_wake_ctx = NULL;

static rmw_ret_t stub_set_wake_callback(rmw_session_t* session, void (*cb)(void* ctx), void* ctx) {
    (void)session;
    s_wake_cb = cb;
    s_wake_ctx = ctx;
    return NROS_RMW_RET_OK;
}

/* Issue 1631 — in ACCEPT mode the stub honours the four policies every
 * profile states, so a service or client (which requires RELIABLE) can be
 * created and counted. Outside accept mode it honours nothing, which is the
 * answer the NULL slot gave and every refusing probe was written against. */
static rmw_ret_t stub_supported_qos_policies(const rmw_session_t* session, uint32_t* out_mask) {
    (void)session;
    if (out_mask != NULL) {
        *out_mask = s_accept_entities ? NROS_RMW_QOS_POLICY_CORE : NROS_RMW_QOS_POLICY_NONE;
    }
    return NROS_RMW_RET_OK;
}

/* Issue 1940 — the per-entity new-data slots, stored per KIND (the probe
 * creates one entity of each) and driven on demand by
 * `nros_stub_rmw_invoke_entity_wake`, as a real backend's arrival path would.
 * `s_entity_wake_supported == false` makes all three answer UNSUPPORTED — the
 * same verdict a NULL slot carries — so one stub covers both halves of the
 * contract. */
static bool s_entity_wake_supported = true;
static rmw_event_callback_t s_entity_cb[3] = {NULL, NULL, NULL};
static const void* s_entity_user_data[3] = {NULL, NULL, NULL};

static rmw_ret_t stub_store_entity_wake(int kind, rmw_event_callback_t callback,
                                        const void* user_data) {
    if (!s_entity_wake_supported) return NROS_RMW_RET_UNSUPPORTED;
    s_entity_cb[kind] = callback;
    s_entity_user_data[kind] = callback != NULL ? user_data : NULL;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_subscription_set_on_new_message_callback(rmw_subscription_t* subscription,
                                                               rmw_event_callback_t callback,
                                                               const void* user_data) {
    (void)subscription;
    return stub_store_entity_wake(NROS_STUB_ENTITY_SUBSCRIPTION, callback, user_data);
}

static rmw_ret_t stub_service_set_on_new_request_callback(rmw_service_t* service,
                                                          rmw_event_callback_t callback,
                                                          const void* user_data) {
    (void)service;
    return stub_store_entity_wake(NROS_STUB_ENTITY_SERVICE, callback, user_data);
}

static rmw_ret_t stub_client_set_on_new_response_callback(rmw_client_t* client,
                                                          rmw_event_callback_t callback,
                                                          const void* user_data) {
    (void)client;
    return stub_store_entity_wake(NROS_STUB_ENTITY_CLIENT, callback, user_data);
}

void nros_stub_rmw_set_entity_wake_supported(bool supported) {
    s_entity_wake_supported = supported;
}

bool nros_stub_rmw_invoke_entity_wake(int kind) {
    rmw_event_callback_t cb = s_entity_cb[kind];
    if (cb == NULL) return false;
    cb(s_entity_user_data[kind], 1);
    return true;
}

/* Issue 1993 — the status-event slots. One registration per kind, kept
 * after the entity is destroyed (a real backend's listener can fire late, and
 * the runtime must make that a no-op). `s_status_events_supported == false`
 * makes `event_type_is_supported` deny every kind, which is what a backend
 * with no event path answers. */
#define STUB_EVENT_KINDS 9
static bool s_status_events_supported = true;
static rmw_status_event_callback_t s_event_cb[STUB_EVENT_KINDS];
static void* s_event_ctx[STUB_EVENT_KINDS];
static int64_t s_event_deadline[STUB_EVENT_KINDS] = {-1, -1, -1, -1, -1, -1, -1, -1, -1};

static rmw_ret_t stub_store_status_event(rmw_event_type_t kind, uint32_t deadline_ms,
                                         rmw_status_event_callback_t cb, void* user_context) {
    if ((unsigned)kind >= STUB_EVENT_KINDS) return NROS_RMW_RET_INVALID_ARGUMENT;
    if (!s_status_events_supported) return NROS_RMW_RET_UNSUPPORTED;
    s_event_cb[kind] = cb;
    s_event_ctx[kind] = user_context;
    s_event_deadline[kind] = (int64_t)deadline_ms;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_subscription_event_init(const rmw_subscription_t* subscription,
                                              rmw_event_type_t kind, uint32_t deadline_ms,
                                              rmw_status_event_callback_t cb, void* user_context) {
    (void)subscription;
    return stub_store_status_event(kind, deadline_ms, cb, user_context);
}

static rmw_ret_t stub_publisher_event_init(const rmw_publisher_t* publisher, rmw_event_type_t kind,
                                           uint32_t deadline_ms, rmw_status_event_callback_t cb,
                                           void* user_context) {
    (void)publisher;
    return stub_store_status_event(kind, deadline_ms, cb, user_context);
}

static bool stub_event_type_is_supported(rmw_event_type_t kind) {
    return s_status_events_supported && (unsigned)kind < STUB_EVENT_KINDS;
}

void nros_stub_rmw_set_status_events_supported(bool supported) {
    s_status_events_supported = supported;
}

int64_t nros_stub_rmw_status_event_deadline_ms(int kind) {
    if (kind < 0 || kind >= STUB_EVENT_KINDS) return -1;
    return s_event_deadline[kind];
}

bool nros_stub_rmw_fire_count_event(int kind, uint32_t total_count, uint32_t total_count_change) {
    if (kind < 0 || kind >= STUB_EVENT_KINDS || s_event_cb[kind] == NULL) return false;
    rmw_event_payload_t payload;
    memset(&payload, 0, sizeof(payload));
    payload.count.total_count = total_count;
    payload.count.total_count_change = total_count_change;
    s_event_cb[kind]((rmw_event_type_t)kind, &payload, s_event_ctx[kind]);
    return true;
}

bool nros_stub_rmw_fire_liveliness_changed(uint16_t alive_count, uint16_t not_alive_count,
                                           int16_t alive_count_change,
                                           int16_t not_alive_count_change) {
    const int kind = NROS_RMW_EVENT_LIVELINESS_CHANGED;
    if (s_event_cb[kind] == NULL) return false;
    rmw_event_payload_t payload;
    memset(&payload, 0, sizeof(payload));
    payload.liveliness_changed.alive_count = alive_count;
    payload.liveliness_changed.not_alive_count = not_alive_count;
    payload.liveliness_changed.alive_count_change = alive_count_change;
    payload.liveliness_changed.not_alive_count_change = not_alive_count_change;
    s_event_cb[kind]((rmw_event_type_t)kind, &payload, s_event_ctx[kind]);
    return true;
}

/* Issue 2060 — a count event staged to fire from the next subscription
 * `has_data`, the poll point a real backend uses for "nothing arrived"
 * events. -1 = none staged. */
static int s_polled_kind = -1;
static uint32_t s_polled_total = 0;
static uint32_t s_polled_change = 0;

void nros_stub_rmw_stage_polled_count_event(int kind, uint32_t total_count,
                                            uint32_t total_count_change) {
    s_polled_kind = kind;
    s_polled_total = total_count;
    s_polled_change = total_count_change;
}

static void stub_poll_status_from_has_data(void) {
    if (s_polled_kind < 0) return;
    const int kind = s_polled_kind;
    s_polled_kind = -1;
    (void)nros_stub_rmw_fire_count_event(kind, s_polled_total, s_polled_change);
}

static const nros_rmw_vtable_t STUB_VTABLE = {
    .create_session = stub_create_session,
    .destroy_session = stub_destroy_session,
    .drive_io = stub_drive_io,
    .create_publisher = stub_create_publisher,
    .destroy_publisher = stub_destroy_publisher,
    .publish = stub_publish,
    .create_subscription = stub_create_subscription,
    .destroy_subscription = stub_destroy_subscription,
    .take = stub_take,
    .take_with_info = stub_take_with_info,
    .has_data = stub_has_data,
    .create_service = stub_create_service,
    .destroy_service = stub_destroy_service,
    .take_request = stub_take_request,
    .has_request = stub_has_request,
    .send_response = stub_send_response,
    .create_client = stub_create_client,
    .destroy_client = stub_destroy_client,
    .send_request = stub_send_request,
    .take_response = stub_take_response,
    .set_wake_callback = stub_set_wake_callback,
    .supported_qos_policies = stub_supported_qos_policies,
    .publisher_get_actual_qos = stub_publisher_get_actual_qos,
    .subscription_set_on_new_message_callback = stub_subscription_set_on_new_message_callback,
    .service_set_on_new_request_callback = stub_service_set_on_new_request_callback,
    .client_set_on_new_response_callback = stub_client_set_on_new_response_callback,
    .subscription_event_init = stub_subscription_event_init,
    .publisher_event_init = stub_publisher_event_init,
    .event_type_is_supported = stub_event_type_is_supported,
    .take_request_with_info = stub_take_request_with_info,
};

int32_t nros_stub_rmw_register(void) {
    return (int32_t)nros_rmw_cffi_register_named(NROS_STUB_RMW_NAME, &STUB_VTABLE);
}

int32_t nros_stub_rmw_register_as(const char* name) {
    return (int32_t)nros_rmw_cffi_register_named(name, &STUB_VTABLE);
}

/* Issue 2074 — the same stub with NO `take_with_info` slot: the runtime then
 * has no per-sample metadata channel for it, as for XRCE or uORB. Issue 2081 —
 * and no `take_request_with_info` slot either: a request carries no metadata
 * channel, as for a C backend written before that slot. */
static nros_rmw_vtable_t s_no_info_vtable;

int32_t nros_stub_rmw_register_without_message_info(const char* name) {
    s_no_info_vtable = STUB_VTABLE;
    s_no_info_vtable.take_with_info = NULL;
    s_no_info_vtable.take_request_with_info = NULL;
    return (int32_t)nros_rmw_cffi_register_named(name, &s_no_info_vtable);
}

uint32_t nros_stub_rmw_drive_io_calls(void) {
    return s_drive_io_calls;
}

bool nros_stub_rmw_wake_cb_installed(void) {
    return s_wake_cb != NULL;
}

bool nros_stub_rmw_invoke_wake(void) {
    void (*cb)(void*) = s_wake_cb;
    if (cb == NULL) return false;
    cb(s_wake_ctx);
    return true;
}
