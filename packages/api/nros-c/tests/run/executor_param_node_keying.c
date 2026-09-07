/* phase-426 W5 — the C parameter surface names a NODE, and the store keeps
 * two nodes' parameters apart.
 *
 * The store is one fixed table owned by the executor (there is no allocator to
 * give each node its own), keyed by node since W1. Before W5 the C surface had
 * no way to say WHICH node: every `nros_executor_*_param_*` meant the primary
 * one, so a two-node C image could declare `rate` exactly once and `ros2 param
 * get /listener rate` would answer with `/talker`'s value. The `_on` spellings
 * close that, and this is the test that would have caught it.
 *
 * COMPILE-AND-RUN rather than a signature probe, because the defect is not in
 * the declarations: a `_on` family that resolved every node to slot 0 would
 * compile, link, and pass `param_entry_points.c`, while every assertion below
 * about a sibling's value would fail.
 *
 * The RMW backend here is a STUB, and deliberately: nothing on this path
 * touches the wire. `nros_executor_init` needs a session pointer, and
 * `nros_executor_node_init` needs an executor to build node slots in, so the
 * seventeen slots `nros_rmw_cffi_register_named` requires are filled with
 * refusals and only `create_session` does anything. That keeps the test a
 * source-gate test — no router, no network, no timing — while still driving
 * the real `Executor`, the real `nros_params::ParameterServer` and the real
 * `apply` rules.
 */

#include <nros/nros.h>
#include <nros/rmw_vtable.h>

#include <stdio.h>
#include <string.h>

/* ---- the stub backend ------------------------------------------------- */

static int g_stub_session;

static rmw_ret_t stub_create_session(const char* locator, uint8_t mode, uint32_t domain_id,
                                     const char* node_name, const rmw_session_options_t* options,
                                     rmw_session_t* out) {
    (void)locator;
    (void)mode;
    (void)domain_id;
    (void)node_name;
    (void)options;
    if (out == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    /* Non-NULL is the whole contract this test needs: `nros_executor_init`
     * refuses a NULL session pointer, and nothing below ever publishes. */
    out->backend_data = &g_stub_session;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_destroy_session(rmw_session_t* session) {
    (void)session;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t stub_drive_io(rmw_session_t* session, int32_t timeout_ms) {
    (void)session;
    (void)timeout_ms;
    return NROS_RMW_RET_OK;
}

/* Every remaining REQUIRED slot refuses. A test that reached one of these
 * would fail with `NROS_RMW_RET_UNSUPPORTED` rather than quietly succeeding
 * against a backend that does nothing. */
static rmw_ret_t
stub_create_publisher(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                      const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                      const rmw_publisher_options_t* options, rmw_publisher_t* out) {
    (void)node;
    (void)type_support;
    (void)topic_name;
    (void)domain_id;
    (void)qos;
    (void)options;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_publisher(rmw_publisher_t* publisher) {
    (void)publisher;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_publish(const rmw_publisher_t* publisher, rmw_byte_span_t payload) {
    (void)publisher;
    (void)payload;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t
stub_create_subscription(const rmw_node_t* node, const rmw_message_type_support_t* type_support,
                         const char* topic_name, uint32_t domain_id, const rmw_qos_profile_t* qos,
                         const rmw_subscription_options_t* options, rmw_subscription_t* out) {
    (void)node;
    (void)type_support;
    (void)topic_name;
    (void)domain_id;
    (void)qos;
    (void)options;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_subscription(rmw_subscription_t* subscription) {
    (void)subscription;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_take(const rmw_subscription_t* subscription, rmw_mut_byte_span_t* out,
                           bool* taken) {
    (void)subscription;
    (void)out;
    (void)taken;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_has_data(rmw_subscription_t* subscription, bool* out_has_data) {
    (void)subscription;
    (void)out_has_data;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_create_service(const rmw_node_t* node,
                                     const rmw_service_type_support_t* type_support,
                                     const char* service_name, uint32_t domain_id,
                                     const rmw_qos_profile_t* qos, rmw_service_t* out) {
    (void)node;
    (void)type_support;
    (void)service_name;
    (void)domain_id;
    (void)qos;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_service(rmw_service_t* server) {
    (void)server;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_take_request(const rmw_service_t* server, rmw_mut_byte_span_t* request,
                                   int64_t* seq_out, bool* taken) {
    (void)server;
    (void)request;
    (void)seq_out;
    (void)taken;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_has_request(rmw_service_t* server, bool* out_has_request) {
    (void)server;
    (void)out_has_request;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_send_response(const rmw_service_t* server, int64_t seq,
                                    rmw_byte_span_t response) {
    (void)server;
    (void)seq;
    (void)response;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_create_client(const rmw_node_t* node,
                                    const rmw_service_type_support_t* type_support,
                                    const char* service_name, uint32_t domain_id,
                                    const rmw_qos_profile_t* qos, rmw_client_t* out) {
    (void)node;
    (void)type_support;
    (void)service_name;
    (void)domain_id;
    (void)qos;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

static rmw_ret_t stub_destroy_client(rmw_client_t* client) {
    (void)client;
    return NROS_RMW_RET_UNSUPPORTED;
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
    .has_data = stub_has_data,
    .create_service = stub_create_service,
    .destroy_service = stub_destroy_service,
    .take_request = stub_take_request,
    .has_request = stub_has_request,
    .send_response = stub_send_response,
    .create_client = stub_create_client,
    .destroy_client = stub_destroy_client,
};

/* `nros_support_init_rmw` calls this before opening the session — the RTOS
 * registration seam (phase 155.B.4). A hosted image usually gets the same
 * effect from the backend archive's `.init_array` ctor; here the backend IS
 * this TU, so registering explicitly is both simpler and the path an embedded
 * image would take. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_rmw_cffi_register_named("stub", &STUB_VTABLE);
}

/* ---- assertions ------------------------------------------------------- */

static int g_failures;

#define CHECK(cond, ...)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            ++g_failures;                                                                          \
            printf("FAIL %s:%d: ", __FILE__, __LINE__);                                            \
            printf(__VA_ARGS__);                                                                   \
            printf("\n");                                                                          \
        }                                                                                          \
    } while (0)

int main(void) {
    struct nros_support_t support;
    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    struct nros_node_t talker;
    struct nros_node_t listener;
    struct nros_node_t stranger;
    int64_t got = 0;
    char text[32];

    memset(&support, 0, sizeof(support));
    memset(&talker, 0, sizeof(talker));
    memset(&listener, 0, sizeof(listener));
    memset(&stranger, 0, sizeof(stranger));

    CHECK(nros_support_init_rmw(&support, NULL, 0, "param_node_keying", "stub") == NROS_RET_OK,
          "support init");
    CHECK(nros_executor_init(&executor, &support, 8) == NROS_RET_OK, "executor init");
    CHECK(nros_executor_node_init(&executor, &talker, "talker", NULL) == NROS_RET_OK,
          "talker node init");
    CHECK(nros_executor_node_init(&executor, &listener, "listener", NULL) == NROS_RET_OK,
          "listener node init");
    CHECK(talker.node_id != listener.node_id, "two nodes landed in ONE executor slot (%u == %u)",
          (unsigned)talker.node_id, (unsigned)listener.node_id);
    if (g_failures != 0) {
        printf("executor_param_node_keying: %d setup failure(s)\n", g_failures);
        return 1;
    }

    /* THE acceptance: the same name on two nodes is two parameters. */
    CHECK(nros_executor_declare_param_integer_on(&executor, &talker, "rate", 10) == NROS_RET_OK,
          "declare rate on talker");
    CHECK(nros_executor_declare_param_integer_on(&executor, &listener, "rate", 20) == NROS_RET_OK,
          "a sibling's identical name must be a DIFFERENT parameter, not a collision");

    CHECK(nros_executor_get_param_integer_on(&executor, &talker, "rate", &got) == NROS_RET_OK &&
              got == 10,
          "talker read back %lld, expected 10", (long long)got);
    CHECK(nros_executor_get_param_integer_on(&executor, &listener, "rate", &got) == NROS_RET_OK &&
              got == 20,
          "listener read back %lld, expected 20", (long long)got);

    /* A write on one node does not move its sibling's value. */
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "rate", 11) == NROS_RET_OK,
          "set rate on talker");
    CHECK(nros_executor_get_param_integer_on(&executor, &listener, "rate", &got) == NROS_RET_OK &&
              got == 20,
          "a set on talker moved listener's value to %lld", (long long)got);

    /* `has` answers per node too. */
    CHECK(nros_executor_declare_param_string_on(&executor, &talker, "frame", "base_link") ==
              NROS_RET_OK,
          "declare frame on talker");
    CHECK(nros_executor_has_param_on(&executor, &talker, "frame"), "talker should have `frame`");
    CHECK(!nros_executor_has_param_on(&executor, &listener, "frame"),
          "`frame` leaked from talker to listener");
    CHECK(nros_executor_get_param_string_on(&executor, &talker, "frame", text, sizeof(text)) ==
              NROS_RET_OK,
          "read frame back from talker");
    CHECK(strcmp(text, "base_link") == 0, "talker frame read back as %s", text);
    CHECK(nros_executor_get_param_string_on(&executor, &listener, "frame", text, sizeof(text)) ==
              NROS_RET_NOT_FOUND,
          "listener answered for a parameter only talker declared");

    /* The un-suffixed spellings mean the PRIMARY node — the first node built,
     * which is `talker` here. That is the claim the whole "C keeps its shape"
     * half of W5 rests on, so assert it rather than assuming it. */
    CHECK(talker.node_id == 0, "the first node built should be the primary slot, got %u",
          (unsigned)talker.node_id);
    CHECK(nros_executor_get_param_integer(&executor, "rate", &got) == NROS_RET_OK && got == 11,
          "the un-suffixed getter read %lld, expected the primary node's 11", (long long)got);

    /* The `apply` rules reach the `_on` setters, per node. An undeclared name
     * is refused (issue 1151) until THAT node opts in, and the opt-in does not
     * reach its sibling. */
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "fresh", 1) == NROS_RET_NOT_FOUND,
          "an undeclared name must be refused, not created");
    CHECK(!nros_executor_has_param_on(&executor, &talker, "fresh"),
          "a refused set created the parameter anyway");
    CHECK(nros_executor_allow_undeclared_parameters_on(&executor, &talker, true) == NROS_RET_OK,
          "opt talker into undeclared sets");
    CHECK(nros_executor_set_param_integer_on(&executor, &talker, "fresh", 1) == NROS_RET_OK,
          "talker opted in and the set was still refused");
    CHECK(nros_executor_set_param_integer_on(&executor, &listener, "fresh", 1) ==
              NROS_RET_NOT_FOUND,
          "allow_undeclared leaked from talker to listener");

    /* A node this executor does not own resolves to nothing — never silently
     * to the primary node, which is what an unchecked `uint8_t` slot argument
     * would have given us. */
    CHECK(nros_executor_declare_param_integer_on(&executor, &stranger, "rate", 99) ==
              NROS_RET_INVALID_ARGUMENT,
          "an uninitialised node was accepted (it reads as slot 0)");
    CHECK(nros_executor_declare_param_integer_on(&executor, NULL, "rate", 99) ==
              NROS_RET_INVALID_ARGUMENT,
          "a NULL node was accepted");
    CHECK(!nros_executor_has_param_on(&executor, &stranger, "rate"),
          "an uninitialised node answered with the primary node's parameters");
    CHECK(nros_executor_get_param_integer_on(&executor, &talker, "rate", &got) == NROS_RET_OK &&
              got == 11,
          "a refused declare disturbed talker's value (%lld)", (long long)got);

    if (g_failures != 0) {
        printf("executor_param_node_keying: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("executor_param_node_keying: two nodes, two parameter tables, one store\n");
    return 0;
}
