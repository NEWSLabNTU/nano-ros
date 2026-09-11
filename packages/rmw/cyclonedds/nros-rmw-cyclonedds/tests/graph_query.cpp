// phase-444 W3 — the graph READ slots, asked ACROSS a participant boundary.
//
// A second session runs every query, so what it answers came off the wire —
// the DDS builtin topics for "what endpoints exist, on what topic, with what
// type and QoS", and the producer's `ros_discovery_info` for "which node owns
// this endpoint". A test that queried the producing session would pass on a
// backend that only reported its own local entities, which is the failure this
// shape exists to exclude.
//
// What each assertion is really about:
//
//   * the name and type are DEMANGLED (`rt/x` -> `/x`,
//     `pkg::msg::dds_::T_` -> `pkg/msg/T`), and `no_demangle` gives the raw DDS
//     spelling instead;
//   * counts MOVE with the graph, and a topic nobody publishes counts 0 rather
//     than erroring — empty is a legitimate answer;
//   * an endpoint carries its OWNING NODE, which only `ros_discovery_info`
//     knows, plus the GRANTED QoS read off the remote's own discovery sample
//     (asserted on a value that differs from the profile default, so an
//     implementation returning zeros or echoing a request fails);
//   * the `*_by_node` forms filter by node identity INCLUDING namespace, and a
//     node that does not exist yields an empty OK, never UNSUPPORTED.
//
// The service half is compiled only where the generated AddTwoInts sources
// exist (they need `rosidl_adapter`), so it drives the REAL `service_create` /
// `client_create` path rather than hand-built DDS endpoints shaped like one.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

#include <dds/dds.h>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;
int g_bad = 0;

void expect(bool ok, const char* what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        g_bad = 1;
    }
}

/// One `(name, types)` the graph reported.
struct NameTypes {
    std::string name;
    std::vector<std::string> types;
};

bool collect_names(void* ctx, const char* name, const char* const* types, size_t types_count) {
    auto* out = static_cast<std::vector<NameTypes>*>(ctx);
    NameTypes nt;
    nt.name = name != nullptr ? name : "";
    for (size_t i = 0; i < types_count; ++i) {
        nt.types.push_back(types[i] != nullptr ? types[i] : "");
    }
    out->push_back(nt);
    return true;
}

/// A flattened endpoint info. Copied, because every string is borrowed for the
/// duration of the visit.
struct Endpoint {
    std::string node_name;
    std::string node_namespace;
    std::string topic_type;
    rmw_endpoint_type_t kind;
    std::string gid;
    std::string identifier;
    rmw_qos_profile_t qos;
};

bool collect_endpoints(void* ctx, const rmw_topic_endpoint_info_t* info) {
    auto* out = static_cast<std::vector<Endpoint>*>(ctx);
    Endpoint e;
    e.node_name = info->node_name != nullptr ? info->node_name : "";
    e.node_namespace = info->node_namespace != nullptr ? info->node_namespace : "";
    e.topic_type = info->topic_type != nullptr ? info->topic_type : "";
    e.kind = info->endpoint_type;
    e.gid.assign(reinterpret_cast<const char*>(info->endpoint_gid.data), RMW_GID_STORAGE_SIZE);
    e.identifier = info->endpoint_gid.implementation_identifier != nullptr
                       ? info->endpoint_gid.implementation_identifier
                       : "";
    e.qos = info->qos_profile;
    out->push_back(e);
    return true;
}

const NameTypes* find_name(const std::vector<NameTypes>& v, const char* name) {
    for (const auto& e : v) {
        if (e.name == name) return &e;
    }
    return nullptr;
}

bool has_type(const NameTypes* nt, const char* type) {
    if (nt == nullptr) return false;
    for (const auto& t : nt->types) {
        if (t == type) return true;
    }
    return false;
}

std::vector<NameTypes> topic_names(const rmw_session_t* s, bool no_demangle) {
    std::vector<NameTypes> out;
    rmw_names_and_types_visitor_t v{collect_names, &out};
    (void)g_vt->get_topic_names_and_types(s, no_demangle, v);
    return out;
}

std::vector<NameTypes> by_node(rmw_ret_t (*slot)(const rmw_session_t*, const char*, const char*,
                                                 bool, rmw_names_and_types_visitor_t),
                               const rmw_session_t* s, const char* node, const char* ns,
                               rmw_ret_t* rc_out) {
    std::vector<NameTypes> out;
    rmw_names_and_types_visitor_t v{collect_names, &out};
    const rmw_ret_t rc = slot(s, node, ns, false, v);
    if (rc_out != nullptr) *rc_out = rc;
    return out;
}

std::vector<NameTypes> by_node_srv(rmw_ret_t (*slot)(const rmw_session_t*, const char*, const char*,
                                                     rmw_names_and_types_visitor_t),
                                   const rmw_session_t* s, const char* node, const char* ns,
                                   rmw_ret_t* rc_out) {
    std::vector<NameTypes> out;
    rmw_names_and_types_visitor_t v{collect_names, &out};
    const rmw_ret_t rc = slot(s, node, ns, v);
    if (rc_out != nullptr) *rc_out = rc;
    return out;
}

std::vector<Endpoint> endpoints(rmw_ret_t (*slot)(const rmw_session_t*, const char*, bool,
                                                  rmw_topic_endpoint_info_visitor_t),
                                const rmw_session_t* s, const char* topic) {
    std::vector<Endpoint> out;
    rmw_topic_endpoint_info_visitor_t v{collect_endpoints, &out};
    (void)slot(s, topic, false, v);
    return out;
}

bool gid_nonzero(const std::string& gid) {
    for (char c : gid) {
        if (c != '\0') return true;
    }
    return false;
}

constexpr const char* kTopic = "gq_chatter";
constexpr const char* kMsgType = "nros_test::msg::TestString";
constexpr const char* kRosMsgType = "nros_test/msg/TestString";
#ifdef NROS_TEST_HAS_SERVICE
constexpr const char* kService = "gq_add";
constexpr const char* kSrvType = "nros_test::srv::dds_::AddTwoInts";
constexpr const char* kRosSrvType = "nros_test/srv/AddTwoInts";
#endif

/// Has the producer's graph reached the observer yet? Discovery is
/// asynchronous, so the whole battery is retried rather than sampled once.
bool converged(const rmw_session_t* obs) {
    const std::vector<NameTypes> topics = topic_names(obs, false);
    const NameTypes* t = find_name(topics, "/gq_chatter");
    if (!has_type(t, kRosMsgType)) return false;
    size_t pubs = 0;
    size_t subs = 0;
    if (g_vt->count_publishers(obs, kTopic, &pubs) != NROS_RMW_RET_OK || pubs != 1) return false;
    if (g_vt->count_subscribers(obs, kTopic, &subs) != NROS_RMW_RET_OK || subs != 1) return false;
    const std::vector<Endpoint> pe = endpoints(g_vt->get_publishers_info_by_topic, obs, kTopic);
    const std::vector<Endpoint> se = endpoints(g_vt->get_subscriptions_info_by_topic, obs, kTopic);
    if (pe.size() != 1 || se.size() != 1) return false;
    // Attribution arrives with the producer's ros_discovery_info sample, which
    // may land after its endpoints do.
    if (pe[0].node_name != "talker" || se[0].node_name != "listener") return false;
#ifdef NROS_TEST_HAS_SERVICE
    std::vector<NameTypes> svc;
    rmw_names_and_types_visitor_t v{collect_names, &svc};
    if (g_vt->get_service_names_and_types(obs, v) != NROS_RMW_RET_OK) return false;
    if (find_name(svc, "/gq_add") == nullptr) return false;
#endif
    return true;
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }
    // Precondition, not a skip: an unfilled slot here means the test would be
    // asserting against UNSUPPORTED, which is the state this work item ends.
    if (g_vt->get_topic_names_and_types == nullptr ||
        g_vt->get_service_names_and_types == nullptr ||
        g_vt->get_publisher_names_and_types_by_node == nullptr ||
        g_vt->get_subscriber_names_and_types_by_node == nullptr ||
        g_vt->get_service_names_and_types_by_node == nullptr ||
        g_vt->get_client_names_and_types_by_node == nullptr ||
        g_vt->get_publishers_info_by_topic == nullptr ||
        g_vt->get_subscriptions_info_by_topic == nullptr || g_vt->count_publishers == nullptr ||
        g_vt->count_subscribers == nullptr || g_vt->create_node == nullptr) {
        std::fprintf(stderr, "FAIL: a graph slot under test is NULL\n");
        return 2;
    }

    const uint32_t domain = nros_test_domain(95);

    rmw_session_t producer{};
    producer.node_name = "graph_query_producer";
    producer.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, domain, producer.node_name, nullptr, &producer) !=
        NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_session_t observer{};
    observer.node_name = "graph_query_observer";
    observer.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, domain, observer.node_name, nullptr, &observer) !=
        NROS_RMW_RET_OK) {
        (void)g_vt->destroy_session(&producer);
        return 3;
    }

    // Two nodes, one namespaced, so the by-node filter has to compare both
    // halves of a node's identity.
    rmw_node_t talker{};
    talker.name = "talker";
    talker.namespace_ = "/";
    talker.session = &producer;
    rmw_node_t listener{};
    listener.name = "listener";
    listener.namespace_ = "/robot";
    listener.session = &producer;
    expect(g_vt->create_node(&producer, talker.name, talker.namespace_, &talker) == NROS_RMW_RET_OK,
           "create_node(talker)");
    expect(g_vt->create_node(&producer, listener.name, listener.namespace_, &listener) ==
               NROS_RMW_RET_OK,
           "create_node(listener)");

    // TRANSIENT_LOCAL + depth 7: both differ from the profile defaults, so the
    // QoS assertion below fails an implementation that reports zeros or echoes
    // the asking side's request.
    rmw_qos_profile_t qos{};
    qos.reliability = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability = NROS_RMW_DURABILITY_TRANSIENT_LOCAL;
    qos.history = NROS_RMW_HISTORY_KEEP_LAST;
    qos.depth = 7;

    rmw_publisher_t pub{};
    pub.topic_name = kTopic;
    pub.type_name = kMsgType;
    const rmw_message_type_support_t mts{kMsgType, ""};
    if (g_vt->create_publisher(&talker, &mts, kTopic, domain, &qos, nullptr, &pub) !=
        NROS_RMW_RET_OK) {
        (void)g_vt->destroy_session(&observer);
        (void)g_vt->destroy_session(&producer);
        return 4;
    }
    rmw_subscription_t sub{};
    sub.topic_name = kTopic;
    sub.type_name = kMsgType;
    if (g_vt->create_subscription(&listener, &mts, kTopic, domain, &qos, nullptr, &sub) !=
        NROS_RMW_RET_OK) {
        (void)g_vt->destroy_publisher(&pub);
        (void)g_vt->destroy_session(&observer);
        (void)g_vt->destroy_session(&producer);
        return 4;
    }

#ifdef NROS_TEST_HAS_SERVICE
    // The real service path: a server on one node, a client on another, so the
    // server/client split (which side READS `rq/`) is observable.
    rmw_node_t adder{};
    adder.name = "adder";
    adder.namespace_ = "/";
    adder.session = &producer;
    rmw_node_t caller{};
    caller.name = "caller";
    caller.namespace_ = "/";
    caller.session = &producer;
    expect(g_vt->create_node(&producer, adder.name, adder.namespace_, &adder) == NROS_RMW_RET_OK,
           "create_node(adder)");
    expect(g_vt->create_node(&producer, caller.name, caller.namespace_, &caller) == NROS_RMW_RET_OK,
           "create_node(caller)");
    rmw_service_t server{};
    server.service_name = kService;
    server.type_name = kSrvType;
    const rmw_service_type_support_t sts{kSrvType, ""};
    expect(g_vt->create_service(&adder, &sts, kService, domain, nullptr, &server) ==
               NROS_RMW_RET_OK,
           "create_service");
    rmw_client_t client{};
    client.service_name = kService;
    client.type_name = kSrvType;
    expect(g_vt->create_client(&caller, &sts, kService, domain, nullptr, &client) ==
               NROS_RMW_RET_OK,
           "create_client");
#endif

    // Poll to convergence: these slots report what has ALREADY been discovered
    // and never block, so one sample right after creation is legitimately
    // partial (RFC-0036).
    bool ok = false;
    for (int i = 0; i < 150 && !ok; ++i) {
        ok = converged(&observer);
        if (!ok) std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    expect(ok, "the observer must discover the producer's topic, counts, endpoints and node "
               "attribution");

    // ---- names and types, demangled and raw ------------------------------
    const std::vector<NameTypes> topics = topic_names(&observer, false);
    const NameTypes* chatter = find_name(topics, "/gq_chatter");
    expect(chatter != nullptr, "the ROS topic name must be demangled from `rt/gq_chatter`");
    expect(has_type(chatter, kRosMsgType),
           "the type must be demangled from `nros_test::msg::TestString`");
    expect(find_name(topics, "ros_discovery_info") == nullptr,
           "an unprefixed DDS topic is not a ROS topic and must not be listed");

    const std::vector<NameTypes> raw = topic_names(&observer, true);
    expect(find_name(raw, "rt/gq_chatter") != nullptr, "no_demangle must report the DDS spelling");
    expect(find_name(raw, "/gq_chatter") == nullptr,
           "no_demangle must NOT also report the demangled spelling");

    // ---- counts ----------------------------------------------------------
    size_t n = 0;
    expect(g_vt->count_publishers(&observer, kTopic, &n) == NROS_RMW_RET_OK && n == 1,
           "count_publishers must see the one publisher");
    expect(g_vt->count_subscribers(&observer, kTopic, &n) == NROS_RMW_RET_OK && n == 1,
           "count_subscribers must see the one subscription");
    // The leading slash is the same ROS topic; both spellings mangle to one
    // DDS name, exactly as `create_publisher` mangles them.
    expect(g_vt->count_publishers(&observer, "/gq_chatter", &n) == NROS_RMW_RET_OK && n == 1,
           "a leading slash names the same topic");
    expect(g_vt->count_publishers(&observer, "gq_nobody_here", &n) == NROS_RMW_RET_OK && n == 0,
           "a topic nobody publishes counts 0 — empty is an answer, not an error");

    // ---- endpoint info ---------------------------------------------------
    const std::vector<Endpoint> pubs =
        endpoints(g_vt->get_publishers_info_by_topic, &observer, kTopic);
    expect(pubs.size() == 1, "one publisher on the topic");
    if (pubs.size() == 1) {
        const Endpoint& e = pubs[0];
        expect(e.node_name == "talker", "the publisher's OWNING NODE, from ros_discovery_info");
        expect(e.node_namespace == "/", "the owning node's namespace");
        expect(e.topic_type == kRosMsgType, "the endpoint's type, demangled");
        expect(e.kind == RMW_ENDPOINT_PUBLISHER, "endpoint_type must say publisher");
        expect(gid_nonzero(e.gid), "a gid of all zeroes identifies nothing");
        expect(e.identifier == "cyclonedds", "a gid without its identifier cannot be compared");
        expect(e.qos.durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL,
               "the GRANTED durability, read off the remote's discovery sample");
        expect(e.qos.depth == 7, "the granted history depth");
        expect(e.qos.reliability == NROS_RMW_RELIABILITY_RELIABLE, "the granted reliability");
    }
    const std::vector<Endpoint> subs =
        endpoints(g_vt->get_subscriptions_info_by_topic, &observer, kTopic);
    expect(subs.size() == 1, "one subscription on the topic");
    if (subs.size() == 1) {
        expect(subs[0].node_name == "listener", "the subscription's owning node");
        expect(subs[0].node_namespace == "/robot", "a namespaced node reports its namespace");
        expect(subs[0].kind == RMW_ENDPOINT_SUBSCRIPTION, "endpoint_type must say subscription");
    }

    // ---- by node ---------------------------------------------------------
    rmw_ret_t rc = NROS_RMW_RET_ERROR;
    std::vector<NameTypes> t_pub =
        by_node(g_vt->get_publisher_names_and_types_by_node, &observer, "talker", "/", &rc);
    expect(rc == NROS_RMW_RET_OK, "publisher_names_and_types_by_node returned non-OK");
    expect(find_name(t_pub, "/gq_chatter") != nullptr, "talker publishes /gq_chatter");
    std::vector<NameTypes> t_sub =
        by_node(g_vt->get_subscriber_names_and_types_by_node, &observer, "talker", "/", &rc);
    expect(rc == NROS_RMW_RET_OK && t_sub.empty(), "talker subscribes to nothing");
    std::vector<NameTypes> l_sub =
        by_node(g_vt->get_subscriber_names_and_types_by_node, &observer, "listener", "/robot", &rc);
    expect(rc == NROS_RMW_RET_OK, "subscriber_names_and_types_by_node returned non-OK");
    expect(find_name(l_sub, "/gq_chatter") != nullptr, "listener subscribes to /gq_chatter");
    // The namespace is part of the identity, not decoration.
    std::vector<NameTypes> wrong_ns =
        by_node(g_vt->get_subscriber_names_and_types_by_node, &observer, "listener", "/", &rc);
    expect(rc == NROS_RMW_RET_OK && wrong_ns.empty(),
           "a node is (name, namespace) — the same name in another namespace is another node");
    std::vector<NameTypes> absent =
        by_node(g_vt->get_publisher_names_and_types_by_node, &observer, "no_such_node", "/", &rc);
    expect(rc == NROS_RMW_RET_OK && absent.empty(),
           "a node nobody has seen is EMPTY, never UNSUPPORTED");

#ifdef NROS_TEST_HAS_SERVICE
    std::vector<NameTypes> svc;
    rmw_names_and_types_visitor_t sv{collect_names, &svc};
    expect(g_vt->get_service_names_and_types(&observer, sv) == NROS_RMW_RET_OK,
           "get_service_names_and_types returned non-OK");
    const NameTypes* add = find_name(svc, "/gq_add");
    expect(add != nullptr, "the service name must be demangled from `rq/gq_addRequest`");
    expect(has_type(add, kRosSrvType),
           "the service type drops the _Request half: nros_test/srv/AddTwoInts");
    std::vector<NameTypes> a_srv =
        by_node_srv(g_vt->get_service_names_and_types_by_node, &observer, "adder", "/", &rc);
    expect(rc == NROS_RMW_RET_OK, "service_names_and_types_by_node returned non-OK");
    expect(find_name(a_srv, "/gq_add") != nullptr, "adder SERVES /gq_add");
    std::vector<NameTypes> a_cli =
        by_node_srv(g_vt->get_client_names_and_types_by_node, &observer, "adder", "/", &rc);
    expect(rc == NROS_RMW_RET_OK && a_cli.empty(), "the server is not a client of its own service");
    std::vector<NameTypes> c_cli =
        by_node_srv(g_vt->get_client_names_and_types_by_node, &observer, "caller", "/", &rc);
    expect(rc == NROS_RMW_RET_OK, "client_names_and_types_by_node returned non-OK");
    expect(find_name(c_cli, "/gq_add") != nullptr, "caller CALLS /gq_add");
    (void)g_vt->destroy_client(&client);
    (void)g_vt->destroy_service(&server);
#endif

    (void)g_vt->destroy_subscription(&sub);
    (void)g_vt->destroy_publisher(&pub);
    (void)g_vt->destroy_session(&observer);
    (void)g_vt->destroy_session(&producer);

    if (g_bad == 0) {
        std::printf("graph_query: OK (topics, types, counts, endpoint info + node attribution, "
                    "by-node filters)\n");
    }
    return g_bad;
}
