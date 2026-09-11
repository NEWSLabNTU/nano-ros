// phase-444 W3 — the graph READ side: the eleven slots beyond `get_node_names`.
//
// Cyclone was VISIBLE in the ROS graph and could not read it: `graph.cpp`
// published `ros_discovery_info` and (since phase-381 W5) read back node names,
// and every other graph slot was NULL — so `count_publishers`, "what types are
// on this topic" and "what does node X publish" answered UNSUPPORTED on the one
// backend that meets real ROS 2 peers.
//
// TWO SOURCES, and neither alone is enough:
//
//   * **The DDS builtin topics** (`DCPSPublication` / `DCPSSubscription`) know
//     every discovered endpoint — its topic, its type, its QoS and its GUID —
//     and know nothing about ROS nodes, because DDS has none.
//   * **`ros_discovery_info`** is where the node layer lives: one
//     `ParticipantEntitiesInfo` per participant, each listing its nodes and the
//     endpoint GIDs belonging to each. That is the ONLY place the
//     endpoint -> node edge exists, which is why the `*_by_node` slots and the
//     `node_name` on an endpoint info are attributed through it.
//
// The contract (RFC-0036 "Static CONNECTION, dynamic GRAPH"): these report what
// has been DISCOVERED and never block; empty means "nobody seen yet", never
// "nobody exists"; and a slot that cannot answer says UNSUPPORTED, which stays
// distinct from empty. A graph with no `ros_discovery_info` writer (the
// descriptor missing) is inactive, and inactive is UNSUPPORTED.
//
// **No allocator is assumed.** Every string handed to a visitor is borrowed for
// that call: names point into the loaned builtin sample or into a small stack
// buffer. The one heap use is the transient sample batch, from
// `ddsrt_malloc` — never libc, because the RTOS heap is separate
// (cyclonedds-known-limitations.md).

#include "internal.hpp"

#include <cstddef>
#include <cstring>

#include <dds/dds.h>

#include "dds/ddsrt/heap.h"
#include "dds/ddsrt/string.h"
#include "graph.hpp"
#include "qos.hpp"
#include "topic_prefix.hpp"

namespace nros_rmw_cyclonedds {

namespace {

/// How many endpoints one query examines.
///
/// A bound rather than a loop, because Cyclone's reader API has no cursor: a
/// second `dds_read` returns the same first N instances, so pagination is not
/// available and the alternative to a bound is an unbounded stack/heap batch on
/// a target that may have neither. 32 endpoints is well past what an embedded
/// image's own graph holds, and a host-side query that hits the ceiling reports
/// the first 32 — which is what "reports what has been discovered" already
/// admits. Raising it costs `kScanMax * (sizeof(void*) + sizeof(sample_info))`
/// of transient heap per query.
constexpr int kScanMax = 32;

/// One demangled name or type. `string<256>` is the `NodeEntitiesInfo` bound
/// and comfortably past a ROS name.
constexpr std::size_t kNameCap = 256;

/// Distinct types reported for ONE name. ROS allows several (the same topic
/// published with two types is a real, diagnosable state) and in practice it is
/// one; a name with more than this reports the first few rather than dropping
/// the name.
constexpr int kMaxTypesPerName = 4;

/// A batch of builtin-topic samples, held for the life of one query so the slot
/// bodies can index it twice — once to pick the next undreported name, once to
/// collect that name's types. `dds_read` (not take) leaves the history intact,
/// so a later query sees the same graph rather than draining it.
class EndpointBatch {
  public:
    EndpointBatch() = default;
    ~EndpointBatch() { release(); }
    EndpointBatch(const EndpointBatch&) = delete;
    EndpointBatch& operator=(const EndpointBatch&) = delete;

    /// Create (once per session) and read the builtin reader for writers or
    /// readers. False only when the reader cannot be created — an EMPTY graph
    /// is a successful read of nothing.
    bool read(GraphState* g, dds_entity_t participant, bool writers) {
        release();
        if (g == nullptr || participant <= 0) return false;
        dds_entity_t* slot = writers ? &g->builtin_pub_reader : &g->builtin_sub_reader;
        if (*slot <= 0) {
            // Created on FIRST USE, like the `ros_discovery_info` reader: an
            // image that never asks pays no reader. The builtin subscriber
            // supplies its own QoS; passing NULL takes it.
            const dds_entity_t topic =
                writers ? DDS_BUILTIN_TOPIC_DCPSPUBLICATION : DDS_BUILTIN_TOPIC_DCPSSUBSCRIPTION;
            dds_entity_t r = dds_create_reader(participant, topic, nullptr, nullptr);
            if (r < 0) return false;
            *slot = r;
        }
        reader_ = *slot;
        raw_ = static_cast<void**>(ddsrt_malloc(sizeof(void*) * kScanMax));
        info_ = static_cast<dds_sample_info_t*>(ddsrt_malloc(sizeof(dds_sample_info_t) * kScanMax));
        if (raw_ == nullptr || info_ == nullptr) {
            release();
            return false;
        }
        std::memset(raw_, 0, sizeof(void*) * kScanMax);
        n_ = dds_read(reader_, raw_, info_, kScanMax, kScanMax);
        if (n_ < 0) n_ = 0;
        return true;
    }

    int count() const { return static_cast<int>(n_); }

    /// The sample at `i`, or nullptr where the sample carries no data (a
    /// disposed endpoint leaves an instance behind with `valid_data == false` —
    /// reporting it would list endpoints that have gone away).
    const dds_builtintopic_endpoint_t* at(int i) const {
        if (i < 0 || i >= count() || !info_[i].valid_data) return nullptr;
        return static_cast<const dds_builtintopic_endpoint_t*>(raw_[i]);
    }

    void release() {
        if (reader_ > 0 && n_ > 0 && raw_ != nullptr) {
            (void)dds_return_loan(reader_, raw_, n_);
        }
        if (raw_ != nullptr) ddsrt_free(raw_);
        if (info_ != nullptr) ddsrt_free(info_);
        raw_ = nullptr;
        info_ = nullptr;
        n_ = 0;
        reader_ = 0;
    }

  private:
    dds_entity_t reader_{0};
    void** raw_{nullptr};
    dds_sample_info_t* info_{nullptr};
    int32_t n_{0};
};

/* ---- ROS 2 name mangling, the read direction ------------------------------
 *
 * `topic_prefix.hpp` owns the WRITE direction (`rt/` + name). These are its
 * inverse, and they must agree with stock `rmw_cyclonedds_cpp` rather than only
 * with us: what comes back is whatever a real ROS 2 peer put on the wire.
 */

/// `"rt/chatter"` -> `"/chatter"`, borrowed from the sample. nullptr when the
/// DDS topic is not a ROS TOPIC — a service's `rq/`/`rr/` topic, or an
/// unprefixed one like `ros_discovery_info` itself, which stock `ros2 topic
/// list` does not show either.
const char* demangle_topic(const char* dds_name) {
    if (dds_name == nullptr) return nullptr;
    if (dds_name[0] == 'r' && dds_name[1] == 't' && dds_name[2] == '/') return dds_name + 2;
    return nullptr;
}

/// `"rq/add_two_intsRequest"` / `"rr/add_two_intsReply"` -> `"/add_two_ints"`.
/// False when the name is not a service topic of the requested half.
bool demangle_service(const char* dds_name, bool request_half, char* out, std::size_t cap) {
    if (dds_name == nullptr || out == nullptr) return false;
    const char want = request_half ? 'q' : 'r';
    if (dds_name[0] != 'r' || dds_name[1] != want || dds_name[2] != '/') return false;
    const char* body = dds_name + 2; // keep the '/' as the ROS leading slash
    const std::size_t len = std::strlen(body);
    // Stock spells the reply half `Reply`; `Response` is accepted because the
    // .srv member is named that and a hand-built peer may use it.
    static const char* const kSuffixes[] = {"Request", "Response", "Reply"};
    for (const char* suffix : kSuffixes) {
        const std::size_t slen = std::strlen(suffix);
        if (len > slen && std::strcmp(body + len - slen, suffix) == 0) {
            if (len - slen + 1 > cap) return false;
            std::memcpy(out, body, len - slen);
            out[len - slen] = '\0';
            return true;
        }
    }
    if (len + 1 > cap) return false;
    std::memcpy(out, body, len + 1);
    return true;
}

/// `"std_msgs::msg::dds_::String_"` -> `"std_msgs/msg/String"`, and a service
/// type's `_Request` / `_Response` tail dropped so both halves of a service
/// report the type ROS names.
bool demangle_type(const char* dds_type, char* out, std::size_t cap) {
    if (dds_type == nullptr || out == nullptr || cap == 0) return false;
    std::size_t w = 0;
    const char* p = dds_type;
    bool first = true;
    while (*p != '\0') {
        const char* sep = std::strstr(p, "::");
        const std::size_t seg_len =
            (sep != nullptr) ? static_cast<std::size_t>(sep - p) : std::strlen(p);
        // The `dds_` segment is the mangling's own marker, not part of the type.
        const bool is_dds_marker = (seg_len == 4 && std::strncmp(p, "dds_", 4) == 0);
        if (seg_len > 0 && !is_dds_marker) {
            if (!first) {
                if (w + 1 >= cap) return false;
                out[w++] = '/';
            }
            if (w + seg_len >= cap) return false;
            std::memcpy(out + w, p, seg_len);
            w += seg_len;
            first = false;
        }
        if (sep == nullptr) break;
        p = sep + 2;
    }
    out[w] = '\0';
    // The trailing `_` idlc appends to every struct name.
    if (w > 0 && out[w - 1] == '_') {
        out[--w] = '\0';
    }
    static const char* const kHalves[] = {"_Request", "_Response", "_Reply"};
    for (const char* half : kHalves) {
        const std::size_t hlen = std::strlen(half);
        if (w > hlen && std::strcmp(out + w - hlen, half) == 0) {
            w -= hlen;
            out[w] = '\0';
            break;
        }
    }
    return w > 0;
}

/// What a query is asking for. The DDS topic prefix plus the endpoint side is
/// what separates a service SERVER from its CLIENT: both use `rq/` and `rr/`,
/// and a server SUBSCRIBES to requests while a client PUBLISHES them.
enum class Want {
    Topic,         ///< `rt/`, either side (which side is the batch's business)
    ServiceServer, ///< reader on `rq/`
    ServiceClient, ///< writer on `rq/`
};

/// The name this endpoint reports under, or nullptr when it is not what `want`
/// asked for. `scratch` backs a service name; a topic name borrows the sample.
const char* wanted_name(const dds_builtintopic_endpoint_t* ep, Want want, bool no_demangle,
                        char* scratch, std::size_t cap) {
    if (ep == nullptr || ep->topic_name == nullptr) return nullptr;
    if (want == Want::Topic) {
        // `no_demangle` is upstream's "give me the DDS names": every topic, raw.
        if (no_demangle) return ep->topic_name;
        return demangle_topic(ep->topic_name);
    }
    return demangle_service(ep->topic_name, /*request_half=*/true, scratch, cap) ? scratch
                                                                                 : nullptr;
}

/// Node attribution for one endpoint GUID, through `ros_discovery_info`.
struct OwnerQuery {
    const uint8_t* want_gid; ///< 16 GUID bytes
    bool want_writer;
    char name[kNameCap];
    char ns[kNameCap];
    bool found;
};

bool owner_visit(void* ctx, const char* node_name, const char* node_ns, const uint8_t gid[24],
                 bool is_writer) {
    auto* q = static_cast<OwnerQuery*>(ctx);
    if (is_writer != q->want_writer || std::memcmp(gid, q->want_gid, 16) != 0) {
        return true;
    }
    ddsrt_strlcpy(q->name, node_name != nullptr ? node_name : "", sizeof(q->name));
    ddsrt_strlcpy(q->ns, node_ns != nullptr ? node_ns : "/", sizeof(q->ns));
    q->found = true;
    return false; // stop: an endpoint belongs to one node
}

/// Fill `name`/`ns` with the node that owns `ep`, or empty strings when the
/// graph does not say. Empty is honest: the endpoint was discovered through
/// DDS, and no participant has claimed it in `ros_discovery_info` — a raw-DDS
/// peer, or a ROS node whose graph sample has not arrived yet.
void owner_of(GraphState* g, const dds_builtintopic_endpoint_t* ep, bool writer, char* name,
              char* ns, std::size_t cap) {
    name[0] = '\0';
    ns[0] = '\0';
    if (g == nullptr || ep == nullptr) return;
    OwnerQuery q;
    q.want_gid = ep->key.v;
    q.want_writer = writer;
    q.name[0] = '\0';
    q.ns[0] = '\0';
    q.found = false;
    (void)graph_visit_endpoints(g, &q, owner_visit);
    if (q.found) {
        ddsrt_strlcpy(name, q.name, cap);
        ddsrt_strlcpy(ns, q.ns, cap);
    }
}

bool same_node(const char* a_name, const char* a_ns, const char* b_name, const char* b_ns) {
    if (a_name == nullptr || b_name == nullptr) return false;
    const char* an = (a_ns != nullptr && a_ns[0] != '\0') ? a_ns : "/";
    const char* bn = (b_ns != nullptr && b_ns[0] != '\0') ? b_ns : "/";
    return std::strcmp(a_name, b_name) == 0 && std::strcmp(an, bn) == 0;
}

/// One side of the graph, as the name-and-types slots see it.
struct Side {
    EndpointBatch batch;
    bool writers;
};

/// The shared body of all six names-and-types slots.
///
/// `node_name == nullptr` means "every node" (the session-wide forms);
/// otherwise only endpoints the graph attributes to that node are reported.
/// Dedup is positional — a name is reported by its FIRST endpoint, and a later
/// endpoint carrying the same name only contributes its type — which needs no
/// table and therefore no allocator.
rmw_ret_t names_and_types(const rmw_session_t* session, Want want, bool writers, bool no_demangle,
                          const char* node_name, const char* node_namespace,
                          rmw_names_and_types_visitor_t visitor) {
    if (session == nullptr || visitor.visit == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    GraphState* g = session_graph(const_cast<rmw_session_t*>(session));
    const dds_entity_t pp = session_participant(session);
    if (g == nullptr || !g->active || pp <= 0) {
        // No graph publisher means no node layer, and every answer here is
        // about nodes even when the question is not. UNSUPPORTED, not empty.
        return NROS_RMW_RET_UNSUPPORTED;
    }

    EndpointBatch batch;
    if (!batch.read(g, pp, writers)) {
        return NROS_RMW_RET_ERROR;
    }

    char name_buf[kNameCap];
    char other_buf[kNameCap];
    char owner_name[kNameCap];
    char owner_ns[kNameCap];
    char types[kMaxTypesPerName][kNameCap];

    for (int i = 0; i < batch.count(); ++i) {
        const dds_builtintopic_endpoint_t* ep = batch.at(i);
        const char* name = wanted_name(ep, want, no_demangle, name_buf, sizeof(name_buf));
        if (name == nullptr) continue;
        if (node_name != nullptr) {
            owner_of(g, ep, writers, owner_name, owner_ns, kNameCap);
            if (!same_node(owner_name, owner_ns, node_name, node_namespace)) continue;
        }

        // Already reported by an earlier endpoint?
        bool seen = false;
        for (int j = 0; j < i && !seen; ++j) {
            const dds_builtintopic_endpoint_t* pe = batch.at(j);
            const char* pn = wanted_name(pe, want, no_demangle, other_buf, sizeof(other_buf));
            if (pn == nullptr || std::strcmp(pn, name) != 0) continue;
            if (node_name != nullptr) {
                char pon[kNameCap];
                char pons[kNameCap];
                owner_of(g, pe, writers, pon, pons, kNameCap);
                if (!same_node(pon, pons, node_name, node_namespace)) continue;
            }
            seen = true;
        }
        if (seen) continue;

        // Collect this name's distinct types.
        int n_types = 0;
        for (int j = 0; j < batch.count() && n_types < kMaxTypesPerName; ++j) {
            const dds_builtintopic_endpoint_t* te = batch.at(j);
            const char* tn = wanted_name(te, want, no_demangle, other_buf, sizeof(other_buf));
            if (tn == nullptr || std::strcmp(tn, name) != 0) continue;
            if (node_name != nullptr) {
                char ton[kNameCap];
                char tons[kNameCap];
                owner_of(g, te, writers, ton, tons, kNameCap);
                if (!same_node(ton, tons, node_name, node_namespace)) continue;
            }
            char one[kNameCap];
            const char* type = nullptr;
            if (no_demangle) {
                type = te->type_name;
            } else if (demangle_type(te->type_name, one, sizeof(one))) {
                type = one;
            }
            if (type == nullptr) continue;
            bool dup = false;
            for (int k = 0; k < n_types; ++k) {
                if (std::strcmp(types[k], type) == 0) dup = true;
            }
            if (!dup) {
                ddsrt_strlcpy(types[n_types], type, kNameCap);
                ++n_types;
            }
        }

        const char* type_ptrs[kMaxTypesPerName];
        for (int k = 0; k < n_types; ++k)
            type_ptrs[k] = types[k];
        if (!visitor.visit(visitor.ctx, name, type_ptrs, static_cast<std::size_t>(n_types))) {
            return NROS_RMW_RET_OK; // the caller stopped; that is not an error
        }
    }
    return NROS_RMW_RET_OK;
}

/// Shared body of `count_publishers` / `count_subscribers`.
rmw_ret_t count_on_topic(const rmw_session_t* session, const char* topic_name, bool writers,
                         std::size_t* count) {
    if (session == nullptr || topic_name == nullptr || count == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    GraphState* g = session_graph(const_cast<rmw_session_t*>(session));
    const dds_entity_t pp = session_participant(session);
    if (g == nullptr || !g->active || pp <= 0) {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    // Mangle the ASKED name once and compare raw, rather than demangling every
    // endpoint — the same direction `publisher_create` writes, so a topic this
    // session could publish is a topic this count can find.
    char mangled[kNameCap];
    if (!topic_prefix::apply(topic_name, "rt", mangled, sizeof(mangled))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    EndpointBatch batch;
    if (!batch.read(g, pp, writers)) {
        return NROS_RMW_RET_ERROR;
    }
    std::size_t n = 0;
    for (int i = 0; i < batch.count(); ++i) {
        const dds_builtintopic_endpoint_t* ep = batch.at(i);
        if (ep != nullptr && ep->topic_name != nullptr &&
            std::strcmp(ep->topic_name, mangled) == 0) {
            ++n;
        }
    }
    *count = n;
    return NROS_RMW_RET_OK;
}

/// Shared body of `get_publishers_info_by_topic` / `get_subscriptions_info_by_topic`.
rmw_ret_t endpoint_info_by_topic(const rmw_session_t* session, const char* topic_name,
                                 bool no_mangle, bool writers,
                                 rmw_topic_endpoint_info_visitor_t visitor) {
    if (session == nullptr || topic_name == nullptr || visitor.visit == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    GraphState* g = session_graph(const_cast<rmw_session_t*>(session));
    const dds_entity_t pp = session_participant(session);
    if (g == nullptr || !g->active || pp <= 0) {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    char mangled[kNameCap];
    if (no_mangle) {
        ddsrt_strlcpy(mangled, topic_name, sizeof(mangled));
    } else if (!topic_prefix::apply(topic_name, "rt", mangled, sizeof(mangled))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    EndpointBatch batch;
    if (!batch.read(g, pp, writers)) {
        return NROS_RMW_RET_ERROR;
    }
    char owner_name[kNameCap];
    char owner_ns[kNameCap];
    char type_buf[kNameCap];
    for (int i = 0; i < batch.count(); ++i) {
        const dds_builtintopic_endpoint_t* ep = batch.at(i);
        if (ep == nullptr || ep->topic_name == nullptr) continue;
        if (std::strcmp(ep->topic_name, mangled) != 0) continue;

        owner_of(g, ep, writers, owner_name, owner_ns, kNameCap);
        rmw_topic_endpoint_info_t info;
        std::memset(&info, 0, sizeof(info));
        info.node_name = owner_name;
        info.node_namespace = owner_ns[0] != '\0' ? owner_ns : "";
        if (no_mangle || !demangle_type(ep->type_name, type_buf, sizeof(type_buf))) {
            info.topic_type = ep->type_name != nullptr ? ep->type_name : "";
        } else {
            info.topic_type = type_buf;
        }
        info.endpoint_type = writers ? RMW_ENDPOINT_PUBLISHER : RMW_ENDPOINT_SUBSCRIPTION;
        // Cyclone's GUID is 16 bytes; `rmw_gid_t::data` is 24, zero-padded —
        // the same derivation `graph.cpp` uses, so a GID from here compares
        // equal to the one `ros_discovery_info` lists for the same endpoint.
        std::memcpy(info.endpoint_gid.data, ep->key.v, sizeof(ep->key.v));
        info.endpoint_gid.implementation_identifier = "cyclonedds";
        // The GRANTED profile, read off the remote's own discovery sample —
        // not the local entity's request. One mapping, shared with
        // `read_entity_qos` (`qos.hpp`).
        qos_from_dds(ep->qos, &info.qos_profile);
        if (!visitor.visit(visitor.ctx, &info)) {
            return NROS_RMW_RET_OK;
        }
    }
    return NROS_RMW_RET_OK;
}

} // namespace

/* ---- the slots ---------------------------------------------------------- */

rmw_ret_t graph_get_topic_names_and_types(const rmw_session_t* session, bool no_demangle,
                                          rmw_names_and_types_visitor_t visitor) {
    // Both sides: a topic with only a subscriber is still a topic, and
    // upstream's `rmw_get_topic_names_and_types` reports it.
    rmw_ret_t r = names_and_types(session, Want::Topic, /*writers=*/true, no_demangle, nullptr,
                                  nullptr, visitor);
    if (r != NROS_RMW_RET_OK) return r;
    // The reader pass re-reports a name the writer pass already visited: the
    // two batches cannot see each other, and a visitor that must dedup is the
    // contract upstream has too (`rmw_names_and_types_t` is a set the CALLER
    // merges). Reporting a name twice is recoverable; dropping a
    // subscriber-only topic is not.
    return names_and_types(session, Want::Topic, /*writers=*/false, no_demangle, nullptr, nullptr,
                           visitor);
}

rmw_ret_t graph_get_service_names_and_types(const rmw_session_t* session,
                                            rmw_names_and_types_visitor_t visitor) {
    // A service exists where a SERVER does, and a server is the side that
    // subscribes to `rq/`. Counting clients here would report a service that
    // nobody offers.
    return names_and_types(session, Want::ServiceServer, /*writers=*/false, /*no_demangle=*/false,
                           nullptr, nullptr, visitor);
}

rmw_ret_t graph_get_publisher_names_and_types_by_node(const rmw_session_t* session,
                                                      const char* node_name,
                                                      const char* node_namespace, bool no_demangle,
                                                      rmw_names_and_types_visitor_t visitor) {
    if (node_name == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    return names_and_types(session, Want::Topic, /*writers=*/true, no_demangle, node_name,
                           node_namespace, visitor);
}

rmw_ret_t graph_get_subscriber_names_and_types_by_node(const rmw_session_t* session,
                                                       const char* node_name,
                                                       const char* node_namespace, bool no_demangle,
                                                       rmw_names_and_types_visitor_t visitor) {
    if (node_name == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    return names_and_types(session, Want::Topic, /*writers=*/false, no_demangle, node_name,
                           node_namespace, visitor);
}

rmw_ret_t graph_get_service_names_and_types_by_node(const rmw_session_t* session,
                                                    const char* node_name,
                                                    const char* node_namespace,
                                                    rmw_names_and_types_visitor_t visitor) {
    if (node_name == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    return names_and_types(session, Want::ServiceServer, /*writers=*/false,
                           /*no_demangle=*/false, node_name, node_namespace, visitor);
}

rmw_ret_t graph_get_client_names_and_types_by_node(const rmw_session_t* session,
                                                   const char* node_name,
                                                   const char* node_namespace,
                                                   rmw_names_and_types_visitor_t visitor) {
    if (node_name == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    // The client is the side that WRITES `rq/` — the mirror of the server rule.
    return names_and_types(session, Want::ServiceClient, /*writers=*/true, /*no_demangle=*/false,
                           node_name, node_namespace, visitor);
}

rmw_ret_t graph_get_publishers_info_by_topic(const rmw_session_t* session, const char* topic_name,
                                             bool no_mangle,
                                             rmw_topic_endpoint_info_visitor_t visitor) {
    return endpoint_info_by_topic(session, topic_name, no_mangle, /*writers=*/true, visitor);
}

rmw_ret_t graph_get_subscriptions_info_by_topic(const rmw_session_t* session,
                                                const char* topic_name, bool no_mangle,
                                                rmw_topic_endpoint_info_visitor_t visitor) {
    return endpoint_info_by_topic(session, topic_name, no_mangle, /*writers=*/false, visitor);
}

rmw_ret_t graph_count_publishers(const rmw_session_t* session, const char* topic_name,
                                 std::size_t* count) {
    return count_on_topic(session, topic_name, /*writers=*/true, count);
}

rmw_ret_t graph_count_subscribers(const rmw_session_t* session, const char* topic_name,
                                  std::size_t* count) {
    return count_on_topic(session, topic_name, /*writers=*/false, count);
}

} // namespace nros_rmw_cyclonedds
