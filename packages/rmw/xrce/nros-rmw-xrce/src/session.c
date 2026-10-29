/* Phase 115.K.2 — session lifecycle implementation.
 *
 * Mirrors the Rust `nros-rmw-xrce::XrceRmw::open` shape but in pure
 * C against `uxr_*`. Phase 115.K.2.1 supported UDP only; 115.K.2.4
 * adds a `custom://` locator scheme that routes through the runtime
 * transport vtable bridge in `transport_custom.c`.
 *
 * Allocation: a single `struct xrce_session_state` per session lives
 * on the heap (`malloc`). The pointer is parked in
 * `rmw_session_t::backend_data`. The runtime owns the entity-
 * shell `rmw_session_t` struct itself.
 */

#include <unistd.h>
#include "internal.h"

#include "nros/platform.h"
#include "nros/rmw_ret.h"

#include <uxr/client/client.h>
#if defined(UCLIENT_PROFILE_UDP)
#include <uxr/client/profile/transport/ip/udp/udp_transport.h>
#endif
#if defined(UCLIENT_PROFILE_UDP) && defined(UCLIENT_PLATFORM_POSIX)
#include <uxr/client/profile/transport/ip/udp/udp_transport_posix.h>
#endif
#include <uxr/client/profile/transport/custom/custom_transport.h>
#include <uxr/client/core/session/object_id.h>
#include <uxr/client/util/ping.h>

#include <stdarg.h>
#include <stdint.h>
#include <time.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ---- Helpers ------------------------------------------------------- */

/* issue 1033 — the ONE place this backend renders a diagnostic. Both reporters
 * below funnel through it so there is a single answer to "where does an XRCE
 * capacity failure come out", and adding a third reporter does not add a third
 * spelling of the delivery.
 *
 * The buffer is a local, sized for a name at `XRCE_DDS_NAME_BUF_SIZE` plus the
 * fixed prose. `snprintf` truncates rather than overflowing, and a truncated
 * diagnostic is still strictly more than the nothing these paths printed
 * before. Never called from the topic/request callbacks — only from the create
 * paths and session open — so the stack cost is not on the RX path. */
#if defined(__GNUC__)
__attribute__((format(printf, 1, 2)))
#endif
static void
xrce_log_error(const char* fmt, ...) {
    char msg[352];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(msg, sizeof(msg), fmt, ap);
    va_end(ap);
    if (n < 0) {
        return;
    }
    size_t len = ((size_t)n < sizeof(msg)) ? (size_t)n : sizeof(msg) - 1;
    static const char kLogger[] = "nros_rmw_xrce";
    nros_platform_log_write(XRCE_LOG_ERROR, (const uint8_t*)kLogger, sizeof(kLogger) - 1,
                            (const uint8_t*)msg, (uintptr_t)len);
}

void xrce_report_capacity_exhausted(const char* kind, const char* name, unsigned cap,
                                    const char* knob) {
    xrce_log_error("no free %s slot for '%s': this image was BUILT with %s=%u. "
                   "The default is DERIVED from the entities this image declares "
                   "(nano_ros_node_register(... ENTITIES ...)), so an entity created "
                   "without being declared is not counted. State CONFIG_%s, or declare "
                   "it. Not truncating; the create fails.",
                   kind, (name != NULL) ? name : "(unnamed)", knob, cap, knob);
}

void xrce_report_session_alloc_failed(size_t bytes) {
    xrce_log_error("xrce_session_state_t (%lu bytes) did not fit. Its size is decided "
                   "at BUILD time by NROS_XRCE_MAX_SUBSCRIBERS x "
                   "NROS_XRCE_SUBSCRIBER_RING_DEPTH x NROS_XRCE_SUBSCRIBER_BUFFER_SIZE, "
                   "plus the service server/client slots and two reliable stream buffers "
                   "of NROS_XRCE_TRANSPORT_MTU x NROS_XRCE_STREAM_HISTORY each; it is ONE "
                   "allocation, so a heap larger than the shortfall does not help. Lower a "
                   "cap or raise the platform heap (CONFIG_NROS_ZEPHYR_HEAP_SIZE on Zephyr). "
                   "Declaring this image's endpoints lets the build derive most of these "
                   "(RFC-0100).",
                   (unsigned long)bytes);
}

/* ---- Issue 1734: the session API lock ---------------------------------
 *
 * See `api_lock` in `internal.h`. */

void xrce_session_lock(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    if (st->api_lock_ready) {
        (void)nros_platform_mutex_rec_lock(st->api_lock);
    }
    st->lock_depth++;
}

void xrce_session_unlock(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    /* Read under the lock: only the outermost release of a region in which
     * the last entity left a closed session frees it. */
    bool free_now = false;
    if (st->lock_depth > 0) {
        st->lock_depth--;
        free_now = st->lock_depth == 0 && st->free_pending;
    }
    if (st->api_lock_ready) {
        (void)nros_platform_mutex_rec_unlock(st->api_lock);
    }
    if (free_now) {
        xrce_session_free(st);
    }
}

void xrce_session_free(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    xrce_graph_release(st);
    if (st->api_lock_ready) {
        st->api_lock_ready = false;
        (void)nros_platform_mutex_rec_drop(st->api_lock);
    }
    nros_xrce_free(st);
}

uxrObjectId xrce_alloc_entity_id(xrce_session_state_t* st, uint8_t type) {
    uint16_t id = st->next_entity_id++;
    return uxr_object_id(id, type);
}

rmw_ret_t xrce_confirm_entities(xrce_session_state_t* st, const uint16_t* requests,
                                uint8_t* statuses, size_t count) {
    bool ok = uxr_run_session_until_all_status(&st->session, XRCE_ENTITY_CREATION_TIMEOUT_MS,
                                               requests, statuses, count);
    if (!ok) {
        return NROS_RMW_RET_ERROR;
    }
    for (size_t i = 0; i < count; ++i) {
        if (statuses[i] != UXR_STATUS_OK && statuses[i] != UXR_STATUS_OK_MATCHED) {
            return NROS_RMW_RET_ERROR;
        }
    }
    return NROS_RMW_RET_OK;
}

/* Naming helpers — pure-C ports of `naming.rs`. */
static void copy_truncating(char* out, size_t out_cap, const char* src) {
    if (out == NULL || out_cap == 0) {
        return;
    }
    size_t len = strlen(src);
    if (len + 1 > out_cap) {
        len = out_cap - 1;
    }
    memcpy(out, src, len);
    out[len] = '\0';
}

static void append_truncating(char* out, size_t out_cap, const char* src) {
    if (out == NULL || out_cap == 0) {
        return;
    }
    size_t cur = strlen(out);
    if (cur + 1 >= out_cap) {
        return;
    }
    size_t avail = out_cap - cur - 1;
    size_t add = strlen(src);
    if (add > avail) {
        add = avail;
    }
    memcpy(out + cur, src, add);
    out[cur + add] = '\0';
}

/* Issue 0819 — the one inbound staging path. Contract in `internal.h`. */
bool xrce_stage_inbound(uint8_t* dst, size_t dst_cap, struct ucdrBuffer* ub, size_t len,
                        size_t* out_len) {
    if (out_len == NULL) {
        return false;
    }
    *out_len = 0;
    if (dst == NULL || ub == NULL) {
        return false;
    }
    if (len + XRCE_CDR_HEADER_LEN > dst_cap) {
        return false;
    }
    /* XRCE-DDS interop: the agent delivers the bare CDR-serialized sample
     * WITHOUT the 4-byte encapsulation header — that header belongs to the
     * DDS/RTPS side, which the agent owns. nano-ros's deserializers expect it,
     * so re-prepend it here; symmetric with the publish side, which strips it
     * before `uxr_buffer_topic`. */
    dst[0] = 0x00; /* CDR_LE representation id */
    dst[1] = 0x01;
    dst[2] = 0x00; /* options */
    dst[3] = 0x00;
    /* NOT `memcpy(dst + 4, ub->iterator, len)`. See the header comment: for a
     * fragmented message the iterator addresses only the first fragment, and
     * this call is the one that walks the chain. It reports failure through
     * `ub->error`, which is also how a genuinely short buffer surfaces. */
    if (len > 0 && !ucdr_deserialize_array_uint8_t(ub, dst + XRCE_CDR_HEADER_LEN, len)) {
        return false;
    }
    *out_len = len + XRCE_CDR_HEADER_LEN;
    return true;
}

void xrce_dds_topic_name(const char* topic_name, int avoid_ros_prefix, char* out, size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    const char* src = topic_name;
    if (src && src[0] == '/') {
        src += 1;
    }
    if (!avoid_ros_prefix) {
        copy_truncating(out, out_cap, "rt/");
        append_truncating(out, out_cap, src ? src : "");
    } else {
        copy_truncating(out, out_cap, src ? src : "");
    }
}

/* `rq/` and `rr/` are the ROS namespace conventions on the SERVICE side, the
 * exact counterpart of `rt/` on a topic — so `avoid_ros_namespace_conventions`
 * drops them here too (issue 1329).
 *
 * It did not until then: the flag reached publishers and subscriptions only,
 * and a service that asked for it was admitted and ignored. That is a partial
 * honour, and the per-backend mask this issue introduces cannot express one —
 * a bit is true of the backend or it is not. Refusing the policy outright on
 * XRCE would have cost the pub/sub path something that works; honouring it
 * everywhere costs nothing, because `avoid_ros_namespace_conventions` is false
 * in every profile that ships and the argument only matters to a caller who
 * set it deliberately. */
void xrce_dds_request_topic(const char* service_name, int avoid_ros_prefix, char* out,
                            size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    const char* src = service_name;
    if (src && src[0] == '/') src += 1;
    /* The caller reads `qos->avoid_ros_namespace_conventions` into
     * `avoid_ros_prefix`; the claim is sited there, on the read. */
    if (!avoid_ros_prefix) {
        copy_truncating(out, out_cap, "rq/");
        append_truncating(out, out_cap, src ? src : "");
    } else {
        copy_truncating(out, out_cap, src ? src : "");
    }
    append_truncating(out, out_cap, "Request");
}

void xrce_dds_reply_topic(const char* service_name, int avoid_ros_prefix, char* out,
                          size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    const char* src = service_name;
    if (src && src[0] == '/') src += 1;
    /* As above — the read is at the call site. */
    if (!avoid_ros_prefix) {
        copy_truncating(out, out_cap, "rr/");
        append_truncating(out, out_cap, src ? src : "");
    } else {
        copy_truncating(out, out_cap, src ? src : "");
    }
    append_truncating(out, out_cap, "Reply");
}

/* Insert "Request_" / "Reply_" before a trailing '_' (matches Rust
 * impl: `example_interfaces::srv::dds_::AddTwoInts_` →
 * `example_interfaces::srv::dds_::AddTwoInts_Request_`). */
static void insert_before_trailing_underscore(const char* type_name, const char* insert, char* out,
                                              size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    if (type_name == NULL) {
        copy_truncating(out, out_cap, insert);
        append_truncating(out, out_cap, "_");
        return;
    }
    size_t len = strlen(type_name);
    if (len > 0 && type_name[len - 1] == '_') {
        /* prefix = type_name without the trailing '_' */
        size_t prefix_len = len - 1;
        if (prefix_len + 1 > out_cap) {
            prefix_len = out_cap - 1;
        }
        memcpy(out, type_name, prefix_len);
        out[prefix_len] = '\0';
        append_truncating(out, out_cap, "_");
        append_truncating(out, out_cap, insert);
        append_truncating(out, out_cap, "_");
    } else {
        copy_truncating(out, out_cap, type_name);
        append_truncating(out, out_cap, "_");
        append_truncating(out, out_cap, insert);
        append_truncating(out, out_cap, "_");
    }
}

void xrce_dds_request_type(const char* type_name, char* out, size_t out_cap) {
    insert_before_trailing_underscore(type_name, "Request", out, out_cap);
}

void xrce_dds_reply_type(const char* type_name, char* out, size_t out_cap) {
    /* ROS 2 service reply type is `<Service>_Response_` (the topic keeps the
     * `Reply` suffix, but the type uses `Response`). */
    insert_before_trailing_underscore(type_name, "Response", out, out_cap);
}

uxrQoS_t xrce_map_qos(const rmw_qos_profile_t* qos) {
    uxrQoS_t out;
    if (qos == NULL) {
        out.durability = UXR_DURABILITY_VOLATILE;
        out.reliability = UXR_RELIABILITY_RELIABLE;
        out.history = UXR_HISTORY_KEEP_LAST;
        out.depth = 10;
        return out;
    }
    /* issue 0829 — this IS the SYSTEM_DEFAULT resolution for the XRCE backend,
     * and it needs no new branches: each ternary tests the ONE value that is
     * not the sentinel's answer, so `NROS_RMW_*_SYSTEM_DEFAULT` (0) already
     * lands on VOLATILE / RELIABLE / KEEP_LAST — what `rmw_cyclonedds_cpp` and
     * `rmw_zenoh_cpp` both resolve those three to. That was accidental until
     * now; it is recorded here so a later edit does not flip a test to
     * `== RELIABLE` and quietly send the sentinel the other way, which is
     * exactly what had happened one backend over
     * (`nros-rmw-cyclonedds/src/qos.cpp`, fixed alongside this).
     *
     * The DEPTH deliberately keeps the sentinel instead of resolving it. The
     * client encodes exactly this meaning — `optional_history_depth = false`
     * when `qos.depth == 0` (`create_entities_bin.c:148`), so the field is
     * simply absent from the CREATE submessage and the Agent's DDS layer
     * supplies its own. This backend genuinely has no depth of its own to
     * offer: every default on this path is the Agent's, so deferring is the
     * honest answer rather than a gap. */
    /* nros-qos-honours: DURABILITY_VOLATILE
     * nros-qos-honours: DURABILITY_TRANSIENT_LOCAL
     * nros-qos-honours: RELIABILITY
     * nros-qos-honours: HISTORY
     * nros-qos-honours: DEPTH
     *
     * The four fields `uxrQoS_t` has. They ride the CREATE submessage to the
     * Agent, whose DDS layer applies them (`create_entities_bin.c` sets the
     * `qos_flags` bits and the optional depth). Deadline, lifespan and
     * liveliness have no field here and are NOT claimed — phase-428 W9. */
    out.durability = (qos->durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL)
                         ? UXR_DURABILITY_TRANSIENT_LOCAL
                         : UXR_DURABILITY_VOLATILE;
    out.reliability = (qos->reliability == NROS_RMW_RELIABILITY_BEST_EFFORT)
                          ? UXR_RELIABILITY_BEST_EFFORT
                          : UXR_RELIABILITY_RELIABLE;
    out.history =
        (qos->history == NROS_RMW_HISTORY_KEEP_ALL) ? UXR_HISTORY_KEEP_ALL : UXR_HISTORY_KEEP_LAST;
    out.depth = qos->depth;
    return out;
}

/* Issue 2107 — the QoS the Agent-side entity holds, read back from what this
 * backend sent. The XRCE protocol has no read-back request, so this is derived
 * from the create, and that is only honest because of what was MEASURED
 * against the pinned Agent (2.4.3, Fast-DDS 2.14.6) with a stock
 * `rmw_fastrtps_cpp` peer reading each endpoint's discovered QoS
 * (`get_*_info_by_topic`), issue 2107 "Resolution":
 *
 *  - A DataWriter / DataReader created from BIN holds EXACTLY the four
 *    `uxrQoS_t` fields sent, and Fast-DDS library defaults for the rest
 *    (deadline and lifespan infinite, liveliness AUTOMATIC with an infinite
 *    lease). The Agent copies `DATAWRITER_QOS_DEFAULT` into a LOCAL before
 *    applying the BIN flags (`FastDDSDataWriter::create_by_bin`), and Fast-DDS
 *    substitutes the XML default profile only for the sentinel's ADDRESS — so
 *    an Agent run with an XML default `data_writer` / `data_reader` profile
 *    (BEST_EFFORT, TRANSIENT_LOCAL, deadline 3 s, MANUAL_BY_TOPIC) changed none
 *    of them, while the same file did reach a stock `ros2 topic pub` (positive
 *    control). So these are GRANTED, not merely requested.
 *  - Depth is not on the wire (SEDP carries none). When this backend sent one
 *    it is what the Agent applied; when it sent 0 the field was absent from
 *    the CREATE and the Agent's own default stands, which is left UNKNOWN.
 *  - `avoid_ros_namespace_conventions` is client-side: it chose the DDS topic
 *    names (`xrce_dds_topic_name`), so it is reported as created. */
void xrce_actual_qos_topic(const uxrQoS_t* sent, bool avoid_ros, rmw_qos_profile_t* out) {
    if (sent == NULL || out == NULL) {
        return;
    }
    out->reliability = (sent->reliability == UXR_RELIABILITY_BEST_EFFORT)
                           ? NROS_RMW_RELIABILITY_BEST_EFFORT
                           : NROS_RMW_RELIABILITY_RELIABLE;
    out->durability = (sent->durability == UXR_DURABILITY_TRANSIENT_LOCAL)
                          ? NROS_RMW_DURABILITY_TRANSIENT_LOCAL
                          : NROS_RMW_DURABILITY_VOLATILE;
    out->history = (sent->history == UXR_HISTORY_KEEP_ALL) ? NROS_RMW_HISTORY_KEEP_ALL
                                                           : NROS_RMW_HISTORY_KEEP_LAST;
    if (sent->history != UXR_HISTORY_KEEP_ALL && sent->depth != 0) {
        out->depth = sent->depth;
    }
    /* INFINITE even when a deadline was asked for: issue 2108 emulates the
     * deadline EVENTS client-side, but the Agent's entity — what a peer
     * matches against — still holds none (`uxrQoS_t` cannot carry one). */
    out->deadline_ms = NROS_RMW_DURATION_INFINITE_MS;
    out->lifespan_ms = NROS_RMW_DURATION_INFINITE_MS;
    out->liveliness_kind = (uint8_t)NROS_RMW_LIVELINESS_AUTOMATIC;
    out->liveliness_lease_ms = NROS_RMW_DURATION_INFINITE_MS;
    out->avoid_ros_namespace_conventions = avoid_ros ? 1u : 0u;
}

/* Issue 2107 / 2120 — what a requester's / replier's two endpoints hold.
 *
 * Created from XML (`xrce_service_qos_xml`, the normal path since issue 2120),
 * both endpoints hold the profile the XML carried, exactly as a BIN-created
 * DataWriter / DataReader holds its `uxrQoS_t` — so `applied` is reported the
 * way `xrce_actual_qos_topic` reports a topic entity. Measured, issue 2120
 * "Resolution".
 *
 * Created from BIN (`applied == NULL`, only when the XML did not fit the output
 * stream), the profile is FIXED whatever was asked. micro-XRCE-DDS-Client 3.0.1
 * discards the QoS (`uxr_buffer_create_{requester,replier}_bin`: `(void) qos;`),
 * and the Agent builds both endpoints from fixed attributes
 * (`FastDDSMiddleware::create_{requester,replier}_by_bin`): the writer from a
 * default `PublisherAttributes`, the reader from a default
 * `SubscriberAttributes` with reliability forced RELIABLE. Measured on the
 * pinned Agent (issue 2107): request/reply WRITERS RELIABLE + TRANSIENT_LOCAL,
 * request/reply READERS RELIABLE + VOLATILE, deadline/lifespan infinite,
 * liveliness AUTOMATIC with an infinite lease. History and depth reach no peer
 * and were never sent, so they stay UNKNOWN. */
void xrce_actual_qos_service(bool writer, bool avoid_ros, const uxrQoS_t* applied,
                             rmw_qos_profile_t* out) {
    if (out == NULL) {
        return;
    }
    if (applied != NULL) {
        xrce_actual_qos_topic(applied, avoid_ros, out);
        return;
    }
    out->reliability = NROS_RMW_RELIABILITY_RELIABLE;
    out->durability =
        writer ? NROS_RMW_DURABILITY_TRANSIENT_LOCAL : NROS_RMW_DURABILITY_VOLATILE;
    out->deadline_ms = NROS_RMW_DURATION_INFINITE_MS;
    out->lifespan_ms = NROS_RMW_DURATION_INFINITE_MS;
    out->liveliness_kind = (uint8_t)NROS_RMW_LIVELINESS_AUTOMATIC;
    out->liveliness_lease_ms = NROS_RMW_DURATION_INFINITE_MS;
    out->avoid_ros_namespace_conventions = avoid_ros ? 1u : 0u;
}

/* Issue 2120 — a requester's / replier's QoS, carried as Fast-DDS XML.
 *
 * `uxr_buffer_create_{requester,replier}_bin` drops the QoS on the floor
 * (micro-XRCE-DDS-Client 3.0.1: `(void) qos;`), and the Agent's BIN path builds
 * both endpoints from fixed attributes. Its XML path does not: Agent 2.4.3
 * `FastDDSMiddleware::create_{requester,replier}_by_xml` parses the string with
 * Fast-DDS's own `XMLParser` (`fillDataNode(RequesterAttributes|
 * ReplierAttributes)`) and `FastDDS{Requester,Replier}::create_by_attributes`
 * copies the `<publisher>` / `<subscriber>` reliability, durability and
 * history into the DataWriterQos / DataReaderQos it creates. Measured live,
 * issue 2120 "Resolution".
 *
 * Every element is written, never left to a default: a default-constructed
 * `PublisherAttributes` is TRANSIENT_LOCAL, which is exactly the writer the BIN
 * path produced. `historyMemoryPolicy` keeps the BIN path's
 * PREALLOCATED_WITH_REALLOC, so a reply larger than the preallocated sample
 * still goes out. Depth is omitted when 0, as `uxrQoS_t` omits it.
 *
 * The names are ROS names and DDS type names, which hold no XML
 * metacharacter. Returns the string length, or 0 when it did not fit. */
size_t xrce_service_qos_xml(char* out, size_t cap, bool replier, const char* service,
                            const char* req_type, const char* reply_type,
                            const char* req_topic, const char* reply_topic,
                            const uxrQoS_t* qos) {
    if (out == NULL || cap == 0 || qos == NULL) {
        return 0;
    }
    const char* rel = (qos->reliability == UXR_RELIABILITY_BEST_EFFORT) ? "BEST_EFFORT" : "RELIABLE";
    const char* dur =
        (qos->durability == UXR_DURABILITY_TRANSIENT_LOCAL) ? "TRANSIENT_LOCAL" : "VOLATILE";
    const char* hist = (qos->history == UXR_HISTORY_KEEP_ALL) ? "KEEP_ALL" : "KEEP_LAST";
    char depth[40] = "";
    if (qos->history != UXR_HISTORY_KEEP_ALL && qos->depth != 0) {
        (void)snprintf(depth, sizeof(depth), "<depth>%u</depth>", (unsigned)qos->depth);
    }
    char endpoint[400];
    int e = snprintf(endpoint, sizeof(endpoint),
                     "<historyMemoryPolicy>PREALLOCATED_WITH_REALLOC</historyMemoryPolicy>"
                     "<qos><reliability><kind>%s</kind></reliability>"
                     "<durability><kind>%s</kind></durability></qos>"
                     "<topic><historyQos><kind>%s</kind>%s</historyQos></topic>",
                     rel, dur, hist, depth);
    if (e < 0 || (size_t)e >= sizeof(endpoint)) {
        return 0;
    }
    const char* tag = replier ? "replier" : "requester";
    int n = snprintf(out, cap,
                     "<dds><%s profile_name=\"nros\" service_name=\"%s\" request_type=\"%s\" "
                     "reply_type=\"%s\"><request_topic_name>%s</request_topic_name>"
                     "<reply_topic_name>%s</reply_topic_name><publisher>%s</publisher>"
                     "<subscriber>%s</subscriber></%s></dds>",
                     tag, service, req_type, reply_type, req_topic, reply_topic, endpoint,
                     endpoint, tag);
    if (n < 0 || (size_t)n >= cap) {
        return 0;
    }
    return (size_t)n;
}

/* ---- Issue 2108: client-side DEADLINE emulation ------------------------
 *
 * The Agent's DDS entity has no deadline to report through XRCE (no status
 * submessage exists, and `uxrQoS_t` cannot even set one), but both DEADLINE
 * events are local facts: a subscription missed its deadline when a period
 * ended with no takeable arrival, a publisher when a period ended with no
 * publish. So they are counted here, from the platform's monotonic clock, as
 * the Rust backend this one replaced did (phase 115.K.2). Checked at the end of
 * every `drive_io` — the executor's own thread — and `next_deadline_ms` hands
 * the executor the earliest expiry so a quiet spin wakes for it.
 *
 * What a peer sees is unchanged: the Agent's entity offers / requests an
 * INFINITE deadline, so a stock subscriber requesting a finite deadline still
 * does not match an XRCE publisher. The event is this client's view only. */

uint64_t xrce_now_ms(void) {
    return nros_platform_clock_ns() / 1000000u;
}

int64_t xrce_wall_clock_ns(void) {
    uint64_t ns = nros_platform_time_now_ns();
    return ns > (uint64_t)INT64_MAX ? INT64_MAX : (int64_t)ns;
}

uint32_t xrce_deadline_period(uint32_t requested, uint32_t qos_deadline_ms) {
    uint32_t p = requested != 0 ? requested : qos_deadline_ms;
    return p == NROS_RMW_DURATION_INFINITE_MS ? 0u : p;
}

void xrce_deadline_arm(xrce_deadline* d, uint32_t period_ms, rmw_status_event_callback_t cb,
                       void* ctx, uint64_t now_ms) {
    if (d == NULL) {
        return;
    }
    memset(d, 0, sizeof(*d));
    if (period_ms == 0 || cb == NULL) {
        return; /* disarmed: nothing can be missed */
    }
    d->period_ms = period_ms;
    d->next_expiry_ms = now_ms + period_ms;
    d->cb = cb;
    d->cb_ctx = ctx;
}

void xrce_deadline_touch(xrce_deadline* d, uint64_t now_ms) {
    if (d != NULL && d->period_ms != 0) {
        d->next_expiry_ms = now_ms + d->period_ms;
    }
}

void xrce_deadline_check_one(xrce_deadline* d, rmw_event_type_t kind, uint64_t now_ms) {
    if (d == NULL || d->period_ms == 0 || now_ms < d->next_expiry_ms) {
        return;
    }
    /* Every whole period since the expiry is one more miss — DDS counts each
     * period without a sample, not each check that noticed. */
    uint64_t missed = 1u + (now_ms - d->next_expiry_ms) / d->period_ms;
    d->next_expiry_ms += missed * d->period_ms;
    uint32_t add = missed > UINT32_MAX ? UINT32_MAX : (uint32_t)missed;
    d->total = (UINT32_MAX - d->total < add) ? UINT32_MAX : d->total + add;
    d->change = (UINT32_MAX - d->change < add) ? UINT32_MAX : d->change + add;
    rmw_event_payload_t payload;
    memset(&payload, 0, sizeof(payload));
    payload.count.total_count = d->total;
    payload.count.total_count_change = d->change;
    d->change = 0;
    d->cb(kind, &payload, d->cb_ctx);
}

void xrce_deadline_check(xrce_session_state_t* st, uint64_t now_ms) {
    if (st == NULL) {
        return;
    }
    for (size_t i = 0; i < XRCE_MAX_SUBSCRIBERS; ++i) {
        xrce_subscriber_slot* slot = &st->subscriber_slots[i];
        if (slot->active) {
            xrce_deadline_check_one(&slot->deadline, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED,
                                    now_ms);
        }
    }
    /* `next` is read before the callback, which may destroy this publisher. */
    xrce_publisher_state* ps = st->deadline_publishers;
    while (ps != NULL) {
        xrce_publisher_state* next = ps->deadline_next;
        xrce_deadline_check_one(&ps->deadline, NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED, now_ms);
        ps = next;
    }
}

static void deadline_min(const xrce_deadline* d, uint64_t now_ms, bool* any, uint64_t* best) {
    if (d->period_ms == 0) {
        return;
    }
    uint64_t in = d->next_expiry_ms > now_ms ? d->next_expiry_ms - now_ms : 0u;
    if (!*any || in < *best) {
        *best = in;
        *any = true;
    }
}

bool xrce_deadline_next(const xrce_session_state_t* st, uint64_t now_ms, uint32_t* out_ms) {
    if (st == NULL || out_ms == NULL) {
        return false;
    }
    bool any = false;
    uint64_t best = 0;
    for (size_t i = 0; i < XRCE_MAX_SUBSCRIBERS; ++i) {
        if (st->subscriber_slots[i].active) {
            deadline_min(&st->subscriber_slots[i].deadline, now_ms, &any, &best);
        }
    }
    for (const xrce_publisher_state* ps = st->deadline_publishers; ps != NULL;
         ps = ps->deadline_next) {
        deadline_min(&ps->deadline, now_ms, &any, &best);
    }
    if (any) {
        *out_ms = best > UINT32_MAX ? UINT32_MAX : (uint32_t)best;
    }
    return any;
}

void xrce_deadline_unlist_publisher(xrce_session_state_t* st, xrce_publisher_state* ps) {
    if (st == NULL || ps == NULL || !ps->deadline_listed) {
        return;
    }
    xrce_publisher_state** link = &st->deadline_publishers;
    while (*link != NULL && *link != ps) {
        link = &(*link)->deadline_next;
    }
    if (*link == ps) {
        *link = ps->deadline_next;
    }
    ps->deadline_next = NULL;
    ps->deadline_listed = false;
}

rmw_ret_t xrce_subscription_event_init(const rmw_subscription_t* subscription,
                                       rmw_event_type_t kind, uint32_t deadline_ms,
                                       rmw_status_event_callback_t cb, void* user_context) {
    if (subscription == NULL || subscription->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (kind != NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED) {
        return NROS_RMW_RET_UNSUPPORTED; /* the protocol carries no other status */
    }
    xrce_subscriber_state* ss = (xrce_subscriber_state*)subscription->backend_data;
    if (ss->slot == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_lock(ss->session_state);
    xrce_deadline_arm(&ss->slot->deadline, xrce_deadline_period(deadline_ms, ss->qos_deadline_ms),
                      cb, user_context, xrce_now_ms());
    xrce_session_unlock(ss->session_state);
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_publisher_event_init(const rmw_publisher_t* publisher, rmw_event_type_t kind,
                                    uint32_t deadline_ms, rmw_status_event_callback_t cb,
                                    void* user_context) {
    if (publisher == NULL || publisher->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (kind != NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED) {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    xrce_publisher_state* ps = (xrce_publisher_state*)publisher->backend_data;
    xrce_session_state_t* st = ps->session_state;
    xrce_session_lock(st);
    xrce_deadline_arm(&ps->deadline, xrce_deadline_period(deadline_ms, ps->qos_deadline_ms), cb,
                      user_context, xrce_now_ms());
    if (ps->deadline.period_ms != 0 && !ps->deadline_listed && st != NULL) {
        ps->deadline_next = st->deadline_publishers;
        st->deadline_publishers = ps;
        ps->deadline_listed = true;
    } else if (ps->deadline.period_ms == 0) {
        xrce_deadline_unlist_publisher(st, ps);
    }
    xrce_session_unlock(st);
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_next_deadline_ms(const rmw_session_t* session, uint32_t* out_ms,
                                bool* has_deadline) {
    if (session == NULL || session->backend_data == NULL || out_ms == NULL ||
        has_deadline == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    xrce_session_lock(st);
    uint32_t ms = 0;
    bool any = xrce_deadline_next(st, xrce_now_ms(), &ms);
    xrce_session_unlock(st);
    *has_deadline = any;
    if (any) {
        *out_ms = ms;
    }
    return NROS_RMW_RET_OK;
}

bool xrce_event_type_is_supported(rmw_event_type_t event_type) {
    return event_type == NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED ||
           event_type == NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED;
}

/* issue 1329 — what THIS backend honours.
 *
 * Exactly the fields above plus the topic-name flag: the five `uxrQoS_t`
 * carries to the Agent, and `AVOID_ROS_NAMESPACE_CONVENTIONS`, which reaches
 * `xrce_dds_topic_name` for pub/sub and — since this issue —
 * `xrce_dds_request_topic` / `xrce_dds_reply_topic` for services too.
 *
 * Deadline, lifespan and liveliness are NOT here and this is the change that
 * matters: `uxrQoS_t` has no field for any of them, so a caller asking for a
 * deadline on XRCE used to be admitted by the cffi route's union and then
 * ignored all the way to the wire. It is refused at create now, naming the
 * policy.
 *
 * Liveliness is the one that looks like a regression and is not. Every C and
 * C++ default profile stated `LIVELINESS_AUTOMATIC` where upstream leaves the
 * policy unset, so an honest mask here would have refused every C application
 * on this backend. The fix was to stop DEMANDING a policy nobody asked for —
 * those three surfaces now carry the sentinel, matching upstream and the Rust
 * table — not to let this backend claim a policy it does not implement. */
rmw_ret_t xrce_supported_qos_policies(const rmw_session_t* session, uint32_t* out_mask) {
    (void)session; /* compiled in, not negotiated per session. */
    if (out_mask == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    *out_mask = NROS_RMW_QOS_POLICY_RELIABILITY | NROS_RMW_QOS_POLICY_DURABILITY_VOLATILE |
                NROS_RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL | NROS_RMW_QOS_POLICY_HISTORY |
                NROS_RMW_QOS_POLICY_DEPTH | NROS_RMW_QOS_POLICY_AVOID_ROS_NAMESPACE_CONVENTIONS |
                NROS_RMW_QOS_POLICY_DEADLINE; /* issue 2108 — emulated client-side */
    return NROS_RMW_RET_OK;
}

/* ---- Session-key hashing ------------------------------------------- */

/* djb2 (DJB2_INIT=5381, multiplier=33) over the node name, salted with
 * the process id on POSIX. The XRCE session key must be UNIQUE PER CLIENT
 * SESSION at one agent: the agent keys its session table on it, so a
 * second client presenting the same key REBINDS the session and orphans
 * the first (issue 0150 — both C demo processes defaulted to the
 * phase-266 unified node name "node", djb2("node")=0x7C9B46AB collided,
 * and the listener silently received nothing). Name alone is not enough
 * entropy; the pid restores the uniqueness the pre-266 `nros_{pid}`
 * fallback provided. Embedded builds (one client per device, no pid)
 * keep the plain name hash. */
static uint32_t hash_session_key(const char* s) {
    uint32_t h = 5381u;
    if (s != NULL) {
        for (const unsigned char* p = (const unsigned char*)s; *p; ++p) {
            h = h * 33u + (uint32_t)*p;
        }
    }
#if defined(UCLIENT_PLATFORM_POSIX)
    {
        uint32_t pid = (uint32_t)getpid();
        h = h * 33u + (pid & 0xFFu);
        h = h * 33u + ((pid >> 8) & 0xFFu);
        h = h * 33u + ((pid >> 16) & 0xFFu);
        h = h * 33u + ((pid >> 24) & 0xFFu);
    }
#endif
    /* Ensure non-zero (XRCE-DDS may treat 0 specially). */
    return h == 0u ? 1u : h;
}

/* Parse `host:port`. On failure, returns 0 and leaves outputs
 * unchanged. The caller-provided `host_buf` receives a NUL-terminated
 * copy of the host substring up to its capacity. */
static int parse_host_port(const char* locator, char* host_buf, size_t host_buf_len,
                           uint16_t* port_out) {
    if (locator == NULL || host_buf == NULL || port_out == NULL) {
        return 0;
    }
    const char* colon = strrchr(locator, ':');
    if (colon == NULL) {
        return 0;
    }
    size_t host_len = (size_t)(colon - locator);
    if (host_len == 0 || host_len + 1 > host_buf_len) {
        return 0;
    }
    memcpy(host_buf, locator, host_len);
    host_buf[host_len] = '\0';
    long port = strtol(colon + 1, NULL, 10);
    if (port <= 0 || port > 0xffff) {
        return 0;
    }
    *port_out = (uint16_t)port;
    return 1;
}

/* ---- Session open / close / drive_io ------------------------------- */

/* `udp/host:port` or `udp4://host:port` strip the scheme prefix; bare
 * `host:port` is also accepted. `custom://...` selects the runtime
 * transport vtable bridge. */
static int locator_strip_udp_prefix(const char** locator) {
    static const char* const prefixes[] = {"udp/", "udp4://", "udp://"};
    for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); ++i) {
        size_t plen = strlen(prefixes[i]);
        if (strncmp(*locator, prefixes[i], plen) == 0) {
            *locator = *locator + plen;
            return 1;
        }
    }
    return 0;
}

static int locator_is_custom(const char* locator) {
    if (locator == NULL) return 0;
    return strncmp(locator, "custom://", 9) == 0 || strcmp(locator, "custom") == 0;
}

/* Phase 115.K.2.5.1.5-serial — recognise locator forms that name a
 * serial / pty device:
 *   - `serial://<path>`
 *   - `serial:/<path>`        (some callers omit the second slash)
 *   - `/dev/...`              (bare absolute device path)
 *
 * Returns the substring pointing at the device path on match (caller
 * passes that to `xrce_posix_serial_init`), or NULL on no-match. */
static const char* locator_serial_path(const char* locator) {
    if (locator == NULL) return NULL;
    static const char* const sch_two = "serial://";
    static const char* const sch_one = "serial:/";
    if (strncmp(locator, sch_two, 9) == 0) {
        return locator + 9;
    }
    /* Match `serial:/` *only* if `serial://` did not match — checked
     * via prefix length above (9 vs 8). */
    if (strncmp(locator, sch_one, 8) == 0 && locator[8] != '/') {
        return locator + 8;
    }
    if (strncmp(locator, "/dev/", 5) == 0) {
        return locator;
    }
    return NULL;
}

rmw_ret_t xrce_session_create(const char* locator, uint8_t mode, uint32_t domain_id,
                              const char* node_name, const rmw_session_options_t* options,
                              rmw_session_t* out) {
    /* XRCE has no discovery to restrict and no enclave. */
    (void)mode;
    if (out == NULL || node_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    /* phase-206 W3 — a property this backend cannot honour is REFUSED, not
     * dropped. `mode` and `localhost_only` are hints a backend without the
     * concept may ignore; a configuration PROPERTY is not, because a silently
     * dropped one is indistinguishable from one that took effect and the
     * caller has no other way to find out. XRCE has no run-time option set of
     * its own yet; when it grows one, this becomes a lookup. */
    if (options != NULL && options->property_count != 0) {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    if (out->backend_data != NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        /* issue 1033 — say the SIZE. This is the request that dominated the
         * struct and it is the one an oversized cap kills; a bare BAD_ALLOC
         * names neither the number nor the knobs behind it. */
        xrce_report_session_alloc_failed(sizeof(xrce_session_state_t));
        return NROS_RMW_RET_BAD_ALLOC;
    }
    st->next_entity_id = 2; /* id 1 reserved for the participant */
    /* Issue 1734 — before the state is reachable from anywhere. A port whose
     * recursive mutex cannot be created leaves the lock unready, which is the
     * single-threaded behaviour every image had before this lock existed. */
    st->api_lock_ready = nros_platform_mutex_rec_init(st->api_lock) == 0;

    /* Phase 115.K.2.4 — `custom://...` routes through
     * `xrce_custom_transport_install`. UDP path mirrors K.2.1.
     * Phase 115.K.2.5.1.5-serial — `serial://...` / `/dev/...`
     * routes through `xrce_posix_serial_init` (POSIX hosts only).
     * Phase 127.C.4 — bare host:port locator now uses the
     * platform-blind `xrce_nros_udp_init` path on every target;
     * consumer must link a `nros_platform_udp_*` provider (POSIX
     * net.c auto-linked by xrce-cffi build.rs on libc hosts; Zephyr /
     * bare-metal pull `nros-platform-<rtos>` via cmake glue). */
#if defined(UCLIENT_PLATFORM_POSIX)
    const char* serial_path = locator_serial_path(locator);
#else
    const char* serial_path = NULL;
#endif
    /* #189 — implement the documented contract (`custom://` OR `serial/...`
     * routes through the installed vtable, nros-rmw-xrce-cffi lib.rs doc):
     * on non-POSIX builds a zenoh-style `serial/<device>#...` locator names
     * a UART the board wrapped via `set_custom_transport_ops` — there is no
     * POSIX serial fallback here, and the previous dispatch silently fell
     * through to the bare host:port UDP path (no UDP provider on a
     * serial-only bare-metal image → ConnectionFailed with zero bytes ever
     * written to the UART). Only reroute when a vtable is actually armed so
     * a genuine mis-locator still surfaces as the UDP parse error it always
     * was. POSIX keeps the `serial://<path>` → xrce_posix_serial_init route
     * unchanged (`serial/` vs `serial:/` prefixes are disjoint). */
    int route_custom = locator_is_custom(locator);
#if !defined(UCLIENT_PLATFORM_POSIX)
    if (!route_custom && locator != NULL && strncmp(locator, "serial/", 7) == 0 &&
        xrce_custom_transport_is_armed()) {
        route_custom = 1;
    }
#endif
    if (route_custom) {
        st->use_custom_transport = true;
        rmw_ret_t ret = xrce_custom_transport_install(st, /*framing=*/false);
        if (ret != NROS_RMW_RET_OK) {
            xrce_session_free(st);
            return ret;
        }
        uxr_init_session(&st->session, &st->custom.comm, hash_session_key(node_name));
#if defined(UCLIENT_PLATFORM_POSIX)
    } else if (serial_path != NULL) {
        st->use_custom_transport = true;
        rmw_ret_t sret = xrce_posix_serial_init(st, serial_path);
        if (sret != NROS_RMW_RET_OK) {
            xrce_session_free(st);
            return sret;
        }
        uxr_init_session(&st->session, &st->custom.comm, hash_session_key(node_name));
#endif
    } else {
        /* Issue 0330 — the backend owns its default agent endpoint. Agnostic
         * layers (nros-c's `rclc_support_init`, the `NROS_ENTRY_LOCATOR`
         * macro, the RFC-0045 resolver's bottom rung) now hand us an ABSENT
         * locator instead of restating an XRCE fact — and absent is spelled
         * either NULL (Rust `None`) or `""` (the C/C++ ABI edges, which
         * cannot express NULL through a string-literal macro). Treat both the
         * same: fall back to the local agent host, with the port defaulting
         * to XRCE_DEFAULT_AGENT_PORT just below. */
        const char* addr_locator = (locator != NULL && locator[0] != '\0') ? locator : "127.0.0.1";
        (void)locator_strip_udp_prefix(&addr_locator);

        char host[64];
        uint16_t port = XRCE_DEFAULT_AGENT_PORT;
        if (parse_host_port(addr_locator, host, sizeof(host), &port) == 0) {
            size_t hlen = strlen(addr_locator);
            if (hlen == 0 || hlen + 1 > sizeof(host)) {
                xrce_session_free(st);
                return NROS_RMW_RET_INVALID_ARGUMENT;
            }
            memcpy(host, addr_locator, hlen + 1);
        }
        char port_str[8];
        snprintf(port_str, sizeof(port_str), "%u", (unsigned)port);

        /* Phase 129.NET.3 — UDP via the canonical `nros_platform_udp_*`
         * ABI. Platform-blind: works on any target with a wired
         * platform-provider. */
        st->use_custom_transport = true;
        rmw_ret_t udp_ret = xrce_nros_udp_init(st, host, port_str);
        if (udp_ret != NROS_RMW_RET_OK) {
            xrce_session_free(st);
            return udp_ret;
        }
        uxr_init_session(&st->session, &st->custom.comm, hash_session_key(node_name));
    }

    /* Topic / request / reply callbacks — single registration per
     * session. The session-state pointer is threaded through `args`
     * so the callbacks can find their slot pools without leaning on
     * a module global. */
    uxr_set_topic_callback(&st->session, xrce_topic_callback, st);
    uxr_set_request_callback(&st->session, xrce_request_callback, st);
    uxr_set_reply_callback(&st->session, xrce_reply_callback, st);

    if (!uxr_create_session_retries(&st->session, XRCE_SESSION_CREATION_RETRIES)) {
        /* Issue 1932 — this and the participant confirm below both return
         * NROS_RMW_RET_ERROR, and two clients that failed one of them in a
         * tier-1 sweep could not be told apart afterwards. Name the step and
         * its budget, so the next sighting says which wait ran out. */
        xrce_log_error("XRCE session open: the Agent sent no CREATE_CLIENT status in %d attempt(s) "
                       "x %d ms (issue 1932)",
                       (int)XRCE_SESSION_CREATION_RETRIES,
                       (int)UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL);
        /* Both UDP and `custom://` paths now go through
         * uxrCustomTransport — close via custom_transport. */
        uxr_close_custom_transport(&st->custom);
        xrce_session_free(st);
        return NROS_RMW_RET_ERROR;
    }

    st->output_reliable =
        uxr_create_output_reliable_stream(&st->session, st->output_reliable_buf,
                                          sizeof(st->output_reliable_buf), XRCE_STREAM_HISTORY);
    st->input_reliable = uxr_create_input_reliable_stream(
        &st->session, st->input_reliable_buf, sizeof(st->input_reliable_buf), XRCE_STREAM_HISTORY);

    /* Create the DDS participant. ID 1 is reserved for it. */
    st->participant_oid = uxr_object_id(1, UXR_PARTICIPANT_ID);

    char name_buf[XRCE_PARTICIPANT_NAME_BUF_SIZE];
    copy_truncating(name_buf, sizeof(name_buf), node_name);

    uint16_t req =
        uxr_buffer_create_participant_bin(&st->session, st->output_reliable, st->participant_oid,
                                          (uint16_t)domain_id, name_buf, UXR_REPLACE);

    uint8_t status = 0;
    uint16_t requests[1] = {req};
    uint8_t statuses[1] = {0};
    rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 1);
    (void)status;
    if (cret != NROS_RMW_RET_OK) {
        /* Issue 1932 — status 0 (UXR_STATUS_NONE) is a timeout: the Agent
         * answered the session but not the participant within the budget. */
        xrce_log_error("XRCE session open: participant create on domain %u got status %u "
                       "within %d ms (%s; issue 1932)",
                       (unsigned)domain_id, (unsigned)statuses[0],
                       (int)XRCE_ENTITY_CREATION_TIMEOUT_MS,
                       statuses[0] == UXR_STATUS_NONE ? "no answer" : "refused by the Agent");
        (void)uxr_delete_session(&st->session);
        uxr_close_custom_transport(&st->custom);
        xrce_session_free(st);
        return cret;
    }

    /* Issue 1292 — announce this session's nodes to stock ROS 2 graph caches.
     * The session's own `node_name` names the PARTICIPANT, not a node: nodes
     * arrive through `create_node`, once each (issue 1269's rule, which put a
     * phantom `/node` in Cyclone's graph while it was broken). */
    xrce_graph_open(st);

    out->backend_data = st;
    return NROS_RMW_RET_OK;
}

/* ---- Issue 0847: entity/session lifetime ------------------------------
 *
 * See the long note on `live_entities` in `internal.h` for why this is a
 * refcount rather than a back-pointer sweep. */

void xrce_session_entity_attach(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    xrce_session_lock(st);
    st->live_entities++;
    xrce_session_unlock(st);
}

bool xrce_session_is_closed(const xrce_session_state_t* st) {
    return st == NULL ? true : st->session_closed;
}

void xrce_session_entity_detach(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    xrce_session_lock(st);
    if (st->live_entities > 0) {
        st->live_entities--;
    }
    /* The last entity out of a CLOSED session turns off the lights. An open
     * session keeps its state: entities come and go while it runs. The free
     * itself is the outermost unlock's (issue 1734), so a destroy that detaches
     * inside its own locked region never unlocks freed memory. */
    if (st->session_closed && st->live_entities == 0) {
        st->free_pending = true;
    }
    xrce_session_unlock(st);
}

rmw_ret_t xrce_session_destroy(rmw_session_t* session) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    xrce_session_lock(st);
    (void)uxr_delete_session(&st->session);
    /* All three transport paths (custom://, serial://, udp://) now
     * sit on top of uxrCustomTransport — the K.2.5.1.2.a fix routed
     * UDP through the same surface to match xrce-sys's legacy
     * shape. Close once, regardless of `use_custom_transport`. */
    uxr_close_custom_transport(&st->custom);

    /* Issue 0847 — the lifetime decision, separated from the uxr teardown
     * above so it is reachable by a test. `uxr_delete_session` needs a live
     * transport (it waits on a session status and faults without one), and the
     * bug this fixes has nothing to do with the wire: it is about WHEN the
     * state is freed. `xrce_session_mark_closed` is the function that decides,
     * and it is the one `tests/entity_lifetime.c` drives. */
    session->backend_data = NULL;
    xrce_session_mark_closed(st);
    /* May free `st` (issue 1734): the mark above deferred it to here. */
    xrce_session_unlock(st);
    return NROS_RMW_RET_OK;
}

void xrce_session_mark_closed(xrce_session_state_t* st) {
    if (st == NULL) {
        return;
    }
    /* Issue 0847 — the session is CLOSED here, but it is only FREED once the
     * last entity handle pointing at it is gone. Freeing unconditionally is
     * what made an ordinary teardown order (close, then entities drop at end
     * of scope) a use-after-free.
     *
     * The handles are still valid objects afterwards: their destructors run
     * normally, see `session_closed`, skip the agent-side DELETE_ENTITY that
     * now has nowhere to go, and free themselves. The last one frees this.
     *
     * A deferred free is NOT an error: it is the supported ordering, not a
     * mistake by the caller. */
    xrce_session_lock(st);
    st->session_closed = true;
    if (st->live_entities == 0) {
        st->free_pending = true;
    }
    xrce_session_unlock(st);
}

rmw_ret_t xrce_session_drive_io(rmw_session_t* session, int32_t timeout_ms) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    int t = timeout_ms < 0 ? 0 : (int)timeout_ms;

    /* Issue 1292 — a burst of creates marks the graph dirty many times; the
     * sample goes out once per tick, here, so a node with N endpoints costs
     * one `ros_discovery_info` write rather than N. */
    xrce_session_lock(st);
    xrce_graph_flush(st);
    xrce_session_unlock(st);

    /* `uxr_run_session_time` returns as soon as the reliable output streams
     * are confirmed — so when the session holds a publisher with unconfirmed
     * WRITE_DATA (or a pending heartbeat) it returns almost immediately
     * (~0 us) instead of listening for `t` ms. XRCE is a *poll-based* backend
     * (no `set_wake_callback`): the executor's `spin_once(t)` paces by relying
     * on this call to block for `t`. When it returns early the spin loop
     * free-runs — a pub+sub node burns through a bounded loop in ~1 ms and
     * closes its session (DELETE_CLIENT) before its subscriber finishes DDS
     * discovery, so it never receives. See issue 0026.
     *
     * Drive the session across the whole `t` ms window — each pass services
     * inbound (delivering subscriber samples) — and yield ~1 ms when a pass
     * returns early, so the call consumes ~t ms wall-clock the way the caller
     * expects, without busy-spinning. (Mirrors the zpico_spin_once
     * `z_sleep_ms` fix for multi-threaded platforms.) */
    if (t == 0) {
        xrce_session_lock(st);
        (void)uxr_run_session_time(&st->session, 0);
        xrce_deadline_check(st, xrce_now_ms()); /* issue 2108 */
        xrce_session_unlock(st);
        return NROS_RMW_RET_OK;
    }
    /* Issue 0029 — use the platform clock/sleep, NOT POSIX
     * `clock_gettime(CLOCK_MONOTONIC)` / `nanosleep`. This path compiles for
     * every XRCE target incl. bare-metal Cortex-M (thumbv7m/thumbv7em), where
     * `<time.h>` does not declare `CLOCK_MONOTONIC` and there is no `nanosleep`.
     *
     * Issue 0035 — use the *monotonic* clock `nros_platform_clock_ns`, NOT the
     * *wall* clock `nros_platform_time_now_ns`. The latter is epoch time and
     * returns 0 on any platform without an RTC (Zephyr/native_sim, FreeRTOS,
     * ThreadX — see each provider's `platform.c`). With a 0 clock `elapsed_ms`
     * is always 0, `remaining` never reaches 0, and this loop spins forever
     * inside `uxr_run_session_time` — so `spin_once` never returns to run the
     * executor's readiness scan and buffered subscriber samples are never
     * delivered to callbacks. `nros_platform_clock_ns` is monotonic since
     * boot and is the contract for relative deadline deltas (mirrors
     * `uxr_millis` in platform_aliases.c).
     *
     * Issue 0548 / phase-352 W6 — `clock_ns` is the only exported clock
     * symbol. `clock_ms` was briefly a `static inline` wrapper and is now
     * retired outright, so the millisecond domain is this caller's own
     * division. */
    uint64_t start_ms = nros_platform_clock_ns() / 1000000u;
    for (;;) {
        uint64_t now_ms = nros_platform_clock_ns() / 1000000u;
        int elapsed_ms = (int)(now_ms - start_ms);
        int remaining = t - elapsed_ms;
        if (remaining <= 0) {
            break;
        }
        /* Issue 1734 — the lock is held for ONE pass, never across the yield
         * below, so a create or publish on another thread gets in between
         * passes. A pass is bounded by `remaining`, and a poll-only executor
         * already slices its drive (`POLL_ONLY_WAKE_SLICE_MS`). */
        xrce_session_lock(st);
        (void)uxr_run_session_time(&st->session, remaining);
        xrce_session_unlock(st);
        /* If the run returned well before `remaining`, yield ~1 ms so the next
         * pass picks up freshly-arrived inbound without busy-spinning. */
        uint64_t after_ms = nros_platform_clock_ns() / 1000000u;
        if ((after_ms - now_ms) < 1u) {
            nros_platform_sleep_ms(1);
        }
    }
    /* Issue 2108 — the deadline check runs on the executor's thread, after
     * this drive delivered whatever arrived (each arrival restarted its
     * subscription's period in `xrce_topic_callback`). */
    xrce_session_lock(st);
    xrce_deadline_check(st, xrce_now_ms());
    xrce_session_unlock(st);
    return NROS_RMW_RET_OK;
}

/* Phase 124.F.2 — session-level connectivity probe.
 *
 * micro-XRCE-DDS-Client ships `uxr_ping_agent_session`: a single
 * GET_INFO round-trip over the already-open session that doesn't
 * disturb the rest of the application's streams. One attempt per
 * call — the runtime's `timeout_ms` is the per-attempt budget. */
rmw_ret_t xrce_session_ping(rmw_session_t* session, int32_t timeout_ms) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t* st = (xrce_session_state_t*)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    int t = timeout_ms < 0 ? 0 : (int)timeout_ms;
    xrce_session_lock(st);
    bool ok = uxr_ping_agent_session(&st->session, t, 1);
    xrce_session_unlock(st);
    return ok ? NROS_RMW_RET_OK : NROS_RMW_RET_TIMEOUT;
}
