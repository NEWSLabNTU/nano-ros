/*
 * The C half of the graph-query parity probe. Every 100 ms it asks the
 * executor for the graph's node, topic and service counts and for
 * `/chatter`'s publisher count and type. Once `/chatter` has a publisher and
 * the answer has held for five probes, it prints the answer once.
 */
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

#define STABLE_PROBES 5

typedef struct {
    rcl_timer_t timer;
    rclc_executor_t* executor;
    char last[256];
    int stable;
    bool done;
} probe_t;

/* A timer callback carries no context, and this component has one instance. */
static probe_t* g_probe;

typedef struct {
    size_t count;
    char chatter_type[128];
} topics_t;

static bool count_node(void* ctx, const char* name, const char* ns, const char* enclave) {
    (void)name;
    (void)ns;
    (void)enclave;
    (*(size_t*)ctx)++;
    return true;
}

static bool count_topic(void* ctx, const char* name, const char* const* types, size_t n) {
    topics_t* t = (topics_t*)ctx;
    t->count++;
    if (strcmp(name, "/chatter") == 0 && n > 0) {
        snprintf(t->chatter_type, sizeof(t->chatter_type), "%s", types[0]);
    }
    return true;
}

static bool count_service(void* ctx, const char* name, const char* const* types, size_t n) {
    (void)name;
    (void)types;
    (void)n;
    (*(size_t*)ctx)++;
    return true;
}

static void on_probe(rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
    probe_t* self = g_probe;
    if (self == NULL || self->done) {
        return;
    }
    size_t publishers = 0, nodes = 0, services = 0;
    topics_t topics = {0, ""};
    if (rclc_executor_count_publishers(self->executor, "/chatter", &publishers) != RCL_RET_OK ||
        rclc_executor_get_node_names(self->executor, count_node, &nodes) != RCL_RET_OK ||
        rclc_executor_get_topic_names_and_types(self->executor, count_topic, &topics) !=
            RCL_RET_OK ||
        rclc_executor_get_service_names_and_types(self->executor, count_service, &services) !=
            RCL_RET_OK) {
        return;
    }
    char answer[256];
    snprintf(answer, sizeof(answer),
             "nodes=%zu topics=%zu services=%zu chatter_publishers=%zu chatter_type=%s", nodes,
             topics.count, services, publishers, topics.chatter_type);
    if (strcmp(answer, self->last) == 0) {
        self->stable++;
    } else {
        snprintf(self->last, sizeof(self->last), "%s", answer);
        self->stable = 0;
    }
    if (publishers == 0 || self->stable < STABLE_PROBES) {
        return;
    }
    self->done = true;
    printf("GRAPH_QUERY lang=c %s\n", answer);
    fflush(stdout);
}

static rcl_ret_t probe_configure(rcl_node_t* node, rclc_executor_t* executor, probe_t* self) {
    (void)node;
    self->executor = executor;
    self->last[0] = '\0';
    self->stable = 0;
    self->done = false;
    g_probe = self;
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t rc = rclc_timer_init_default(&self->timer, NULL, 100ULL * 1000000ULL, on_probe);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(probe_t, probe_configure)
