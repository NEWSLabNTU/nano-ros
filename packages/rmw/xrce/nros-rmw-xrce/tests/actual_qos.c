/* Issue 2107 — the six `*_get_actual_qos` slots.
 *
 * No agent: the per-entity state holds what the create sent. What each slot
 * reports, and why it is the GRANTED profile rather than the requested one, was
 * measured against the pinned Agent (issue 2107 "Resolution"):
 *
 *   - pub/sub: the four `uxrQoS_t` fields sent, Fast-DDS defaults for the rest
 *     (deadline/lifespan infinite, liveliness AUTOMATIC, infinite lease); depth
 *     only when one was sent, else left as pre-loaded (UNKNOWN);
 *   - service/client created from XML (issue 2120, the normal path): what the
 *     XML carried, reported like a pub/sub entity, on both endpoints;
 *   - service/client that fell back to BIN: the Agent's FIXED requester/replier
 *     profile whatever was asked — writers RELIABLE + TRANSIENT_LOCAL, readers
 *     RELIABLE + VOLATILE, history and depth left UNKNOWN.
 *
 * The caller pre-loads `NROS_RMW_QOS_PROFILE_UNKNOWN`; a field a slot does not
 * know must come back as that pre-load.
 */

#include "internal.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, what)                                                                          \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            printf("  FAIL: %s\n", (what));                                                        \
            failures++;                                                                            \
        } else {                                                                                   \
            printf("  ok    %s\n", (what));                                                        \
        }                                                                                          \
    } while (0)

static const nros_rmw_vtable_t* g_vtable = NULL;
rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    g_vtable = vtable;
    return NROS_RMW_RET_OK;
}

static rmw_qos_profile_t unknown(void) {
    rmw_qos_profile_t q = NROS_RMW_QOS_PROFILE_UNKNOWN;
    return q;
}

static bool infinite_defaults(const rmw_qos_profile_t* q) {
    return q->deadline_ms == NROS_RMW_DURATION_INFINITE_MS &&
           q->lifespan_ms == NROS_RMW_DURATION_INFINITE_MS &&
           q->liveliness_kind == (uint8_t)NROS_RMW_LIVELINESS_AUTOMATIC &&
           q->liveliness_lease_ms == NROS_RMW_DURATION_INFINITE_MS;
}

int main(void) {
    printf("*_get_actual_qos\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL) {
        return 1;
    }
    CHECK(g_vtable->publisher_get_actual_qos != NULL &&
              g_vtable->subscription_get_actual_qos != NULL &&
              g_vtable->client_request_publisher_get_actual_qos != NULL &&
              g_vtable->client_response_subscription_get_actual_qos != NULL &&
              g_vtable->service_request_subscription_get_actual_qos != NULL &&
              g_vtable->service_response_publisher_get_actual_qos != NULL,
          "all six slots are filled");
    if (failures != 0) {
        return 1;
    }
    const rmw_qos_profile_t pre = unknown();

    /* Publisher: BEST_EFFORT, TRANSIENT_LOCAL, KEEP_LAST(7), avoid-ROS. */
    xrce_publisher_state ps;
    memset(&ps, 0, sizeof(ps));
    rmw_qos_profile_t req = NROS_RMW_QOS_PROFILE_DEFAULT;
    req.reliability = NROS_RMW_RELIABILITY_BEST_EFFORT;
    req.durability = NROS_RMW_DURABILITY_TRANSIENT_LOCAL;
    req.history = NROS_RMW_HISTORY_KEEP_LAST;
    req.depth = 7;
    ps.sent_qos = xrce_map_qos(&req);
    ps.avoid_ros = true;
    rmw_publisher_t pub;
    memset(&pub, 0, sizeof(pub));
    pub.backend_data = &ps;
    rmw_qos_profile_t q = unknown();
    CHECK(g_vtable->publisher_get_actual_qos(&pub, &q) == NROS_RMW_RET_OK, "publisher: OK");
    CHECK(q.reliability == NROS_RMW_RELIABILITY_BEST_EFFORT &&
              q.durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL &&
              q.history == NROS_RMW_HISTORY_KEEP_LAST && q.depth == 7,
          "publisher: the four fields the create sent");
    CHECK(infinite_defaults(&q), "publisher: deadline/lifespan/lease infinite, AUTOMATIC");
    CHECK(q.avoid_ros_namespace_conventions == 1, "publisher: avoid-ROS as created");

    /* Subscription: defaults, depth 0 (not sent) — depth stays UNKNOWN. */
    xrce_subscriber_state ss;
    memset(&ss, 0, sizeof(ss));
    rmw_qos_profile_t sreq = NROS_RMW_QOS_PROFILE_DEFAULT;
    sreq.depth = 0;
    ss.sent_qos = xrce_map_qos(&sreq);
    rmw_subscription_t sub;
    memset(&sub, 0, sizeof(sub));
    sub.backend_data = &ss;
    q = unknown();
    CHECK(g_vtable->subscription_get_actual_qos(&sub, &q) == NROS_RMW_RET_OK, "subscription: OK");
    CHECK(q.reliability == NROS_RMW_RELIABILITY_RELIABLE &&
              q.durability == NROS_RMW_DURABILITY_VOLATILE &&
              q.history == NROS_RMW_HISTORY_KEEP_LAST,
          "subscription: SYSTEM_DEFAULT resolved as sent (RELIABLE, VOLATILE, KEEP_LAST)");
    CHECK(q.depth == pre.depth, "subscription: an unsent depth is left UNKNOWN (the pre-load)");
    CHECK(infinite_defaults(&q) && q.avoid_ros_namespace_conventions == 0,
          "subscription: infinite defaults, avoid-ROS off");

    /* KEEP_ALL never reports a depth. */
    rmw_qos_profile_t kreq = NROS_RMW_QOS_PROFILE_DEFAULT;
    kreq.history = NROS_RMW_HISTORY_KEEP_ALL;
    kreq.depth = 9;
    ss.sent_qos = xrce_map_qos(&kreq);
    q = unknown();
    (void)g_vtable->subscription_get_actual_qos(&sub, &q);
    CHECK(q.history == NROS_RMW_HISTORY_KEEP_ALL && q.depth == pre.depth,
          "KEEP_ALL: history reported, no depth");

    /* Service and client created from BIN (the XML did not fit the output
     * stream): the Agent's fixed profile, whatever was asked. */
    xrce_service_server_state srv;
    memset(&srv, 0, sizeof(srv));
    rmw_service_t service;
    memset(&service, 0, sizeof(service));
    service.backend_data = &srv;
    xrce_service_client_state cli;
    memset(&cli, 0, sizeof(cli));
    cli.avoid_ros = true;
    rmw_client_t client;
    memset(&client, 0, sizeof(client));
    client.backend_data = &cli;

    q = unknown();
    CHECK(g_vtable->service_response_publisher_get_actual_qos(&service, &q) == NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_RELIABLE &&
              q.durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL && infinite_defaults(&q),
          "service reply writer: RELIABLE + TRANSIENT_LOCAL (the Agent's fixed replier writer)");
    CHECK(q.history == pre.history && q.depth == pre.depth,
          "service reply writer: history/depth UNKNOWN (never sent, not on the wire)");
    q = unknown();
    CHECK(g_vtable->service_request_subscription_get_actual_qos(&service, &q) ==
                  NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_RELIABLE &&
              q.durability == NROS_RMW_DURABILITY_VOLATILE && q.history == pre.history,
          "service request reader: RELIABLE + VOLATILE, history UNKNOWN");
    q = unknown();
    CHECK(g_vtable->client_request_publisher_get_actual_qos(&client, &q) == NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_RELIABLE &&
              q.durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL &&
              q.avoid_ros_namespace_conventions == 1,
          "client request writer: RELIABLE + TRANSIENT_LOCAL, avoid-ROS as created");
    q = unknown();
    CHECK(g_vtable->client_response_subscription_get_actual_qos(&client, &q) ==
                  NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_RELIABLE &&
              q.durability == NROS_RMW_DURABILITY_VOLATILE && q.depth == pre.depth,
          "client reply reader: RELIABLE + VOLATILE, depth UNKNOWN");

    /* Issue 2120 — created from XML: what the XML carried, on BOTH endpoints.
     * BEST_EFFORT + VOLATILE + KEEP_LAST(3) on the server: the writer is no
     * longer the Agent's TRANSIENT_LOCAL and the reader no longer forced
     * RELIABLE. */
    rmw_qos_profile_t svc_req = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    svc_req.reliability = NROS_RMW_RELIABILITY_BEST_EFFORT;
    svc_req.depth = 3;
    srv.sent_qos = xrce_map_qos(&svc_req);
    srv.qos_applied = true;
    q = unknown();
    CHECK(g_vtable->service_response_publisher_get_actual_qos(&service, &q) == NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_BEST_EFFORT &&
              q.durability == NROS_RMW_DURABILITY_VOLATILE &&
              q.history == NROS_RMW_HISTORY_KEEP_LAST && q.depth == 3 && infinite_defaults(&q),
          "XML service reply writer: BEST_EFFORT + VOLATILE + KEEP_LAST(3), as carried");
    q = unknown();
    CHECK(g_vtable->service_request_subscription_get_actual_qos(&service, &q) ==
                  NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_BEST_EFFORT &&
              q.durability == NROS_RMW_DURABILITY_VOLATILE && q.depth == 3,
          "XML service request reader: BEST_EFFORT (not forced RELIABLE), depth 3");
    rmw_qos_profile_t cli_req = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    cli_req.durability = NROS_RMW_DURABILITY_TRANSIENT_LOCAL;
    cli.sent_qos = xrce_map_qos(&cli_req);
    cli.qos_applied = true;
    q = unknown();
    CHECK(g_vtable->client_response_subscription_get_actual_qos(&client, &q) ==
                  NROS_RMW_RET_OK &&
              q.reliability == NROS_RMW_RELIABILITY_RELIABLE &&
              q.durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL && q.depth == 10 &&
              q.avoid_ros_namespace_conventions == 1,
          "XML client reply reader: TRANSIENT_LOCAL as carried, avoid-ROS as created");

    /* The XML itself: every honoured field is written (a default
     * PublisherAttributes is TRANSIENT_LOCAL), with the BIN path's memory
     * policy, and refused rather than truncated when it does not fit. */
    char xml[XRCE_SERVICE_XML_BUF_SIZE];
    uxrQoS_t xq = xrce_map_qos(&svc_req);
    size_t n = xrce_service_qos_xml(xml, sizeof(xml), true, "add_two_ints",
                                    "example_interfaces::srv::dds_::AddTwoInts_Request_",
                                    "example_interfaces::srv::dds_::AddTwoInts_Response_",
                                    "rq/add_two_intsRequest", "rr/add_two_intsReply", &xq);
    CHECK(n == strlen(xml) && n > 0, "xml: written, length returned");
    CHECK(strncmp(xml, "<dds><replier ", 14) == 0 &&
              strstr(xml, "service_name=\"add_two_ints\"") != NULL &&
              strstr(xml, "<request_topic_name>rq/add_two_intsRequest</request_topic_name>") !=
                  NULL &&
              strstr(xml, "<reply_topic_name>rr/add_two_intsReply</reply_topic_name>") != NULL,
          "xml: a replier with the ROS topic names");
    const char* endpoint =
        "<historyMemoryPolicy>PREALLOCATED_WITH_REALLOC</historyMemoryPolicy>"
        "<qos><reliability><kind>BEST_EFFORT</kind></reliability>"
        "<durability><kind>VOLATILE</kind></durability></qos>"
        "<topic><historyQos><kind>KEEP_LAST</kind><depth>3</depth></historyQos></topic>";
    const char* xpub = strstr(xml, "<publisher>");
    const char* xsub = strstr(xml, "<subscriber>");
    CHECK(xpub != NULL && xsub != NULL && strncmp(xpub + 11, endpoint, strlen(endpoint)) == 0 &&
              strncmp(xsub + 12, endpoint, strlen(endpoint)) == 0,
          "xml: both endpoints carry reliability, durability, history, depth");
    uxrQoS_t keep_all = xq;
    keep_all.history = UXR_HISTORY_KEEP_ALL;
    keep_all.durability = UXR_DURABILITY_TRANSIENT_LOCAL;
    keep_all.reliability = UXR_RELIABILITY_RELIABLE;
    n = xrce_service_qos_xml(xml, sizeof(xml), false, "s", "a", "b", "rq/sRequest",
                             "rr/sReply", &keep_all);
    CHECK(n > 0 && strncmp(xml, "<dds><requester ", 16) == 0 &&
              strstr(xml, "<kind>KEEP_ALL</kind></historyQos>") != NULL &&
              strstr(xml, "<depth>") == NULL &&
              strstr(xml, "<kind>TRANSIENT_LOCAL</kind>") != NULL &&
              strstr(xml, "<kind>RELIABLE</kind>") != NULL,
          "xml: a requester; KEEP_ALL carries no depth");
    CHECK(xrce_service_qos_xml(xml, 64, true, "add_two_ints", "a", "b", "rq/x", "rr/x", &xq) ==
              0,
          "xml: too small a buffer is refused (0), never truncated");

    q = unknown();
    CHECK(g_vtable->publisher_get_actual_qos(NULL, &q) == NROS_RMW_RET_INVALID_ARGUMENT &&
              g_vtable->service_response_publisher_get_actual_qos(&service, NULL) ==
                  NROS_RMW_RET_INVALID_ARGUMENT,
          "NULL arguments are INVALID_ARGUMENT");

    if (failures != 0) {
        printf("actual_qos: %d failure(s)\n", failures);
        return 1;
    }
    printf("actual_qos: OK\n");
    return 0;
}
