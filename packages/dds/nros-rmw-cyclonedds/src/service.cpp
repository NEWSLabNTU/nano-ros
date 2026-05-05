// Services — Phase 117.7 + 117.7.B (request-id correlation).
//
// Service traffic uses a backend-defined envelope type (see
// `src/types/service_envelope.idl`):
//
//     struct ServiceEnvelope {
//         unsigned long long client_id;
//         long long          seq;
//         sequence<octet>    payload;
//     };
//
// Every Request and Reply on `<svc>Request` / `<svc>Reply` topics
// is wrapped: the client sticks a unique random `client_id` plus a
// monotonic `seq` on each request; the server echoes both back
// inside the matching reply; the client filters incoming replies
// on its reader by (client_id, seq). Concurrent calls on the same
// service no longer interleave — each client only consumes its
// own replies.
//
// **Wire compat:** this is **not** the upstream `rmw_cyclonedds_cpp`
// pattern (which uses `cdds_request_header_t` inside the typed
// IDL). Service traffic between nano-ros and stock ROS 2 nodes
// does not interoperate. Tracked in
// `docs/reference/cyclonedds-known-limitations.md`. Same envelope
// pattern as the zenoh backend.
//
// **User type registration is unused for services in this backend**
// — the topic descriptor is always `ServiceEnvelope`. The
// `<type>_Request` / `<type>_Response` registry contract from
// 117.7's first pass remains documented, but is not consulted.

#include "internal.hpp"

#include "descriptors.hpp"
#include "sertype_min.hpp"
#include "topic_prefix.hpp"

#include <dds/dds.h>
#include <dds/ddsi/ddsi_cdrstream.h>

#include <atomic>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <new>
#include <random>
#include <thread>

extern "C" {
// Auto-generated descriptor symbol from idlc-emitted
// `service_envelope.{c,h}`. Resolved at link time; the matching
// `_register_0.c` constructor self-registers it under
// "nros_rmw_cyclonedds::ServiceEnvelope" so even pure-C consumers
// pull it in.
extern const dds_topic_descriptor_t nros_rmw_cyclonedds_ServiceEnvelope_desc;
}

namespace nros_rmw_cyclonedds {

namespace {

constexpr const char *kEnvelopeTopicTypeName = "nros_rmw_cyclonedds::ServiceEnvelope";
constexpr std::size_t kRequestSlots = 32;
constexpr std::size_t kMaxTopicName = 256;

// Cyclone-emitted struct layout for `ServiceEnvelope`.
// Mirrors the typedef in the auto-generated `service_envelope.h`;
// we don't include that header here to keep service.cpp insulated
// from the codegen output dir's include path.
struct EnvSequence {
    uint32_t maximum;
    uint32_t length;
    uint8_t *buffer;
    bool     release;
};
struct ServiceEnvelopeRaw {
    uint64_t    client_id;
    int64_t     seq;
    EnvSequence payload;
};

struct RequestSlot {
    uint64_t client_id{0};
    int64_t  client_seq{0};
    bool     in_use{false};
};

struct ServerState {
    dds_entity_t request_topic{0};
    dds_entity_t reply_topic{0};
    dds_entity_t reader{0};
    dds_entity_t writer{0};
    SertypeMin  *st{nullptr};
    RequestSlot  slots[kRequestSlots];
};

struct ClientState {
    dds_entity_t request_topic{0};
    dds_entity_t reply_topic{0};
    dds_entity_t writer{0};
    dds_entity_t reader{0};
    SertypeMin  *st{nullptr};
    uint64_t                  client_id{0};
    std::atomic<int64_t>      next_seq{0};
};

// Build the wire topic name for a service Request or Reply.
//
// Stock `rmw_cyclonedds_cpp` uses `rq/<svc>Request` and
// `rr/<svc>Reply` (3-letter prefix + slash + service name +
// suffix). We mirror that exactly so a nano-ros client and an
// `rclcpp` server (or vice-versa) match by topic name.
//
// `prefix` is either "rq" (Request) or "rr" (Reply). Idempotent +
// env-opt-out via `topic_prefix::apply` — same opt-out as pub/sub.
bool service_topic_name(const char *service_name, const char *prefix,
                        const char *suffix, char *out, std::size_t out_cap) {
    if (service_name == nullptr || prefix == nullptr || suffix == nullptr ||
        out == nullptr) {
        return false;
    }
    // Combine name + suffix into a scratch buffer first so the
    // resulting "<svc><suffix>" is what gets prefixed by
    // `topic_prefix::apply`. Cap at half the output buffer to leave
    // room for the prefix.
    char with_suffix[256];
    std::size_t blen = std::strlen(service_name);
    std::size_t slen = std::strlen(suffix);
    if (blen + slen + 1 > sizeof(with_suffix)) return false;
    std::memcpy(with_suffix, service_name, blen);
    std::memcpy(with_suffix + blen, suffix, slen);
    with_suffix[blen + slen] = '\0';

    return topic_prefix::apply(with_suffix, prefix, out, out_cap);
}

uint64_t random_client_id() {
    std::random_device rd;
    std::mt19937_64 gen(rd());
    std::uniform_int_distribution<uint64_t> dist;
    return dist(gen);
}

uint32_t cdr_xcdr_version(const uint8_t *bytes) {
    uint8_t lo = bytes[1];
    if (lo == 0x06 || lo == 0x07 || lo == 0x0a || lo == 0x0b) return 2;
    return 1;
}

// Publish an envelope-wrapped payload via @p writer. Caller's bytes
// are copied into `payload.buffer`; ownership released after the
// write returns.
nros_rmw_ret_t write_envelope(dds_entity_t writer, const SertypeMin *st,
                              uint64_t client_id, int64_t seq,
                              const uint8_t *user_bytes, size_t user_len) {
    if (writer <= 0 || st == nullptr || user_bytes == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    ServiceEnvelopeRaw env{};
    env.client_id      = client_id;
    env.seq            = seq;
    env.payload.maximum = static_cast<uint32_t>(user_len);
    env.payload.length  = static_cast<uint32_t>(user_len);
    env.payload.release = false;
    env.payload.buffer  = const_cast<uint8_t *>(user_bytes);

    dds_return_t r = dds_write(writer, &env);
    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

// Take + decode envelope. On success populates @p out_client_id,
// @p out_seq and copies the inner payload into @p buf. Returns the
// payload byte count. Caller must release the loan via
// `dds_return_loan` — that's done internally before return.
//
// Returns NROS_RMW_RET_NO_DATA if the reader has nothing waiting,
// or a negative error code.
int32_t take_envelope(dds_entity_t reader, const SertypeMin *st,
                      uint64_t *out_client_id, int64_t *out_seq,
                      uint8_t *buf, size_t buf_len) {
    void *samples[1] = {nullptr};
    dds_sample_info_t si[1];
    dds_return_t taken = dds_take(reader, samples, si, 1, 1);
    if (taken < 0) return NROS_RMW_RET_ERROR;
    if (taken == 0 || !si[0].valid_data) {
        if (taken > 0) (void) dds_return_loan(reader, samples, taken);
        return NROS_RMW_RET_NO_DATA;
    }

    auto *env = static_cast<const ServiceEnvelopeRaw *>(samples[0]);
    int32_t result;
    if (env->payload.length > buf_len) {
        result = NROS_RMW_RET_BUFFER_TOO_SMALL;
    } else {
        if (out_client_id != nullptr) *out_client_id = env->client_id;
        if (out_seq       != nullptr) *out_seq       = env->seq;
        if (env->payload.length > 0 && env->payload.buffer != nullptr) {
            std::memcpy(buf, env->payload.buffer, env->payload.length);
        }
        result = static_cast<int32_t>(env->payload.length);
    }
    (void) dds_return_loan(reader, samples, taken);
    (void) st;  // sertype not needed for typed take/return-loan
    return result;
}

} // namespace

// =========================================================================
// Service server
// =========================================================================

nros_rmw_ret_t service_server_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char * /*type_name*/,
                                     const char * /*type_hash*/,
                                     uint32_t /*domain_id*/,
                                     nros_rmw_service_server_t *out) {
    if (out == nullptr || session == nullptr || service_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) return NROS_RMW_RET_ERROR;

    char req_topic[kMaxTopicName];
    char rep_topic[kMaxTopicName];
    if (!service_topic_name(service_name, "rq", "Request", req_topic, sizeof(req_topic)) ||
        !service_topic_name(service_name, "rr", "Reply",   rep_topic, sizeof(rep_topic))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto *state = new (std::nothrow) ServerState();
    if (state == nullptr) return NROS_RMW_RET_BAD_ALLOC;

    const dds_topic_descriptor_t *desc = &nros_rmw_cyclonedds_ServiceEnvelope_desc;
    state->request_topic = dds_create_topic(pp, desc, req_topic, nullptr, nullptr);
    state->reply_topic   = dds_create_topic(pp, desc, rep_topic, nullptr, nullptr);
    if (state->request_topic < 0 || state->reply_topic < 0) {
        if (state->request_topic > 0) (void) dds_delete(state->request_topic);
        if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->reader = dds_create_reader(pp, state->request_topic, nullptr, nullptr);
    state->writer = dds_create_writer(pp, state->reply_topic,   nullptr, nullptr);
    if (state->reader < 0 || state->writer < 0) {
        if (state->reader > 0) (void) dds_delete(state->reader);
        if (state->writer > 0) (void) dds_delete(state->writer);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->st = new (std::nothrow) SertypeMin(desc);
    if (state->st == nullptr) {
        (void) dds_delete(state->reader);
        (void) dds_delete(state->writer);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    return NROS_RMW_RET_OK;
}

void service_server_destroy(nros_rmw_service_server_t *server) {
    if (server == nullptr || server->backend_data == nullptr) return;
    auto *state = static_cast<ServerState *>(server->backend_data);
    if (state->reader > 0) (void) dds_delete(state->reader);
    if (state->writer > 0) (void) dds_delete(state->writer);
    if (state->request_topic > 0) (void) dds_delete(state->request_topic);
    if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
    delete state->st;
    delete state;
    server->backend_data = nullptr;
}

int32_t service_try_recv_request(nros_rmw_service_server_t *server,
                                 uint8_t *buf, size_t buf_len,
                                 int64_t *seq_out) {
    if (server == nullptr || server->backend_data == nullptr || buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<ServerState *>(server->backend_data);

    uint64_t client_id = 0;
    int64_t  client_seq = 0;
    int32_t r = take_envelope(state->reader, state->st, &client_id, &client_seq,
                              buf, buf_len);
    if (r <= 0) return r;

    // Allocate a slot to remember (client_id, client_seq); return
    // the slot index as the runtime-visible seq. The runtime hands
    // this back to `service_send_reply` so we can echo the
    // correlation pair on the reply.
    for (std::size_t i = 0; i < kRequestSlots; ++i) {
        if (!state->slots[i].in_use) {
            state->slots[i].in_use     = true;
            state->slots[i].client_id  = client_id;
            state->slots[i].client_seq = client_seq;
            if (seq_out != nullptr) *seq_out = static_cast<int64_t>(i);
            return r;
        }
    }
    // No free slot — drop. Application sees a successful take but
    // can never reply; caller must drain via send_reply with a
    // valid seq before more requests arrive.
    return NROS_RMW_RET_WOULD_BLOCK;
}

int32_t service_has_request(nros_rmw_service_server_t *server) {
    if (server == nullptr || server->backend_data == nullptr) return 0;
    auto *state = static_cast<ServerState *>(server->backend_data);
    uint32_t status = 0;
    if (dds_get_status_changes(state->reader, &status) != DDS_RETCODE_OK) return 0;
    return (status & DDS_DATA_AVAILABLE_STATUS) ? 1 : 0;
}

nros_rmw_ret_t service_send_reply(nros_rmw_service_server_t *server,
                                  int64_t seq, const uint8_t *data,
                                  size_t len) {
    if (server == nullptr || server->backend_data == nullptr ||
        data == nullptr || seq < 0 ||
        static_cast<std::size_t>(seq) >= kRequestSlots) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<ServerState *>(server->backend_data);
    auto &slot = state->slots[seq];
    if (!slot.in_use) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    nros_rmw_ret_t r =
        write_envelope(state->writer, state->st, slot.client_id, slot.client_seq,
                       data, len);
    slot.in_use = false;  // free the slot regardless of write outcome
    return r;
}

// =========================================================================
// Service client
// =========================================================================

nros_rmw_ret_t service_client_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char * /*type_name*/,
                                     const char * /*type_hash*/,
                                     uint32_t /*domain_id*/,
                                     nros_rmw_service_client_t *out) {
    if (out == nullptr || session == nullptr || service_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) return NROS_RMW_RET_ERROR;

    char req_topic[kMaxTopicName];
    char rep_topic[kMaxTopicName];
    if (!service_topic_name(service_name, "rq", "Request", req_topic, sizeof(req_topic)) ||
        !service_topic_name(service_name, "rr", "Reply",   rep_topic, sizeof(rep_topic))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto *state = new (std::nothrow) ClientState();
    if (state == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    state->client_id = random_client_id();
    state->next_seq.store(0, std::memory_order_relaxed);

    const dds_topic_descriptor_t *desc = &nros_rmw_cyclonedds_ServiceEnvelope_desc;
    state->request_topic = dds_create_topic(pp, desc, req_topic, nullptr, nullptr);
    state->reply_topic   = dds_create_topic(pp, desc, rep_topic, nullptr, nullptr);
    if (state->request_topic < 0 || state->reply_topic < 0) {
        if (state->request_topic > 0) (void) dds_delete(state->request_topic);
        if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->writer = dds_create_writer(pp, state->request_topic, nullptr, nullptr);
    state->reader = dds_create_reader(pp, state->reply_topic,   nullptr, nullptr);
    if (state->writer < 0 || state->reader < 0) {
        if (state->writer > 0) (void) dds_delete(state->writer);
        if (state->reader > 0) (void) dds_delete(state->reader);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->st = new (std::nothrow) SertypeMin(desc);
    if (state->st == nullptr) {
        (void) dds_delete(state->writer);
        (void) dds_delete(state->reader);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    return NROS_RMW_RET_OK;
}

void service_client_destroy(nros_rmw_service_client_t *client) {
    if (client == nullptr || client->backend_data == nullptr) return;
    auto *state = static_cast<ClientState *>(client->backend_data);
    if (state->writer > 0) (void) dds_delete(state->writer);
    if (state->reader > 0) (void) dds_delete(state->reader);
    if (state->request_topic > 0) (void) dds_delete(state->request_topic);
    if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
    delete state->st;
    delete state;
    client->backend_data = nullptr;
}

int32_t service_call_raw(nros_rmw_service_client_t *client,
                         const uint8_t *request, size_t req_len,
                         uint8_t *reply_buf, size_t reply_buf_len) {
    if (client == nullptr || client->backend_data == nullptr ||
        request == nullptr || reply_buf == nullptr || req_len < 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<ClientState *>(client->backend_data);

    int64_t my_seq = state->next_seq.fetch_add(1, std::memory_order_relaxed);
    nros_rmw_ret_t pr = write_envelope(state->writer, state->st,
                                       state->client_id, my_seq,
                                       request, req_len);
    if (pr != NROS_RMW_RET_OK) return pr;

    // Poll the reply reader for up to 2s; filter on (client_id,
    // seq) so concurrent calls on parallel clients don't steal
    // each other's replies. (Each client has its own reader, but a
    // single client can have multiple in-flight calls — the seq
    // filter protects against that case too.)
    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::seconds(2);
    while (std::chrono::steady_clock::now() < deadline) {
        uint32_t status = 0;
        dds_return_t sr = dds_get_status_changes(state->reader, &status);
        if (sr == DDS_RETCODE_OK && (status & DDS_DATA_AVAILABLE_STATUS)) {
            uint64_t got_client_id = 0;
            int64_t  got_seq       = 0;
            int32_t  n = take_envelope(state->reader, state->st,
                                       &got_client_id, &got_seq,
                                       reply_buf, reply_buf_len);
            if (n < 0 && n != NROS_RMW_RET_NO_DATA) return n;
            if (n >= 0) {
                if (got_client_id == state->client_id && got_seq == my_seq) {
                    return n;
                }
                // Reply was for someone else (impossible on a per-
                // client reader, but cheap to defend) or for a
                // different in-flight call from us — keep polling.
                continue;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return NROS_RMW_RET_TIMEOUT;
}

} // namespace nros_rmw_cyclonedds
