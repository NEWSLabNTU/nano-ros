/* Issue 3023 -- two nodes of one C component must get two instances.
 *
 * `RCLC_C_COMPONENT` used to keep ONE static state struct and ONE `rcl_node_t`
 * per package, and `create` returned that one address every time, so a launch
 * file naming the same package twice configured the second node over the
 * first: one timer ran, and nothing reported it. This program expands the
 * macro the way a component TU does, stubs the three runtime calls it reaches,
 * and checks the per-instance table: distinct instances, a node bound into
 * each, and a refusal (NULL, one error line) past the cap rather than reuse.
 *
 * The runtime half the macro reaches is defined below; the archive is linked
 * only for the symbols the per-build config header anchors. */

#define NROS_PKG_NAME probe_pkg
#define RCLC_C_COMPONENT_MAX_INSTANCES 2

#include <rclc/component.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* --- runtime stubs ------------------------------------------------------- */

static int g_bind_calls;
static void* g_bound[4];
static int g_errors;

int32_t nros_cpp_node_bind_c(const nros_cpp_node_t* node, void* node_out, void** executor_out) {
    (void)node;
    if (g_bind_calls < 4) g_bound[g_bind_calls] = node_out;
    g_bind_calls++;
    *executor_out = NULL;
    return 0;
}

rclc_logger_t rclc_log_get_logger(const char* name) {
    (void)name;
    return (rclc_logger_t)1;
}

void rclc_log_emit_fmt(rclc_logger_t logger, rcl_log_severity_t severity, const char* fmt, ...) {
    (void)logger;
    if (severity == RCLC_LOG_SEVERITY_ERROR && strstr(fmt, "RCLC_C_COMPONENT_MAX_INSTANCES")) {
        g_errors++;
    }
}

/* --- the component under test ------------------------------------------- */

typedef struct {
    int configured;
    rcl_node_t* node;
} probe_t;

static rcl_ret_t probe_configure(rcl_node_t* node, rclc_executor_t* executor, probe_t* self) {
    (void)executor;
    self->configured++;
    self->node = node;
    return RCL_RET_OK;
}

RCLC_C_COMPONENT(probe_t, probe_configure)

/* --- checks -------------------------------------------------------------- */

#define CHECK(cond)                                                                                \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);                        \
            return EXIT_FAILURE;                                                                   \
        }                                                                                          \
    } while (0)

int main(void) {
    void* a = __nros_c_component_probe_pkg_create();
    void* b = __nros_c_component_probe_pkg_create();
    CHECK(a != NULL);
    CHECK(b != NULL);
    CHECK(a != b); /* the defect: both nodes got one address */

    CHECK(__nros_c_component_probe_pkg_configure(NULL, NULL, a) == RCL_RET_OK);
    CHECK(__nros_c_component_probe_pkg_configure(NULL, NULL, b) == RCL_RET_OK);
    probe_t* pa = (probe_t*)a;
    probe_t* pb = (probe_t*)b;
    /* Each configured once, on its OWN node: the second did not overwrite the first. */
    CHECK(pa->configured == 1);
    CHECK(pb->configured == 1);
    CHECK(pa->node != NULL && pb->node != NULL);
    CHECK(pa->node != pb->node);
    CHECK(g_bind_calls == 2);
    CHECK(g_bound[0] == (void*)pa->node && g_bound[1] == (void*)pb->node);

    /* Past the cap: refused, said once, never a reused instance. */
    CHECK(g_errors == 0);
    void* c = __nros_c_component_probe_pkg_create();
    CHECK(c == NULL);
    CHECK(g_errors == 1);
    CHECK(__nros_c_component_probe_pkg_configure(NULL, NULL, NULL) == RCL_RET_INVALID_ARGUMENT);

    puts("component_instances: OK");
    return EXIT_SUCCESS;
}
