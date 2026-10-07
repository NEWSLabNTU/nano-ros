/*
 * phase-481 W4 — what this image's runtime build compiled in, on one line:
 *
 *   max_handles   NROS_EXECUTOR_MAX_HANDLES, written by nros-c's build script
 *                 from NROS_EXECUTOR_MAX_CBS;
 *   info_enabled  whether INFO passes a logger set to DEBUG at run time — 0
 *                 exactly when nros-log was built with a ceiling above INFO
 *                 (NROS_LOG_MAX_LEVEL);
 *   warn_enabled  the same for WARN, so info_enabled=0 cannot mean "off".
 */
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>
#include <nros/nros.h>

typedef struct {
    int32_t unused;
} probe_t;

static nros_ret_t probe_configure(const nros_cpp_node_t* node, void* executor, probe_t* self) {
    (void)node;
    (void)executor;
    (void)self;
    setvbuf(stdout, NULL, _IOLBF, 0);
    nros_logger_t logger = nros_log_get_logger("image_config_probe");
    nros_logger_set_level(logger, NROS_LOG_SEVERITY_DEBUG);
    printf("image-config-probe: max_handles=%d info_enabled=%d warn_enabled=%d\n",
           (int)NROS_EXECUTOR_MAX_HANDLES,
           nros_logger_is_enabled(logger, NROS_LOG_SEVERITY_INFO) ? 1 : 0,
           nros_logger_is_enabled(logger, NROS_LOG_SEVERITY_WARN) ? 1 : 0);
    return 0;
}

NROS_C_COMPONENT(probe_t, probe_configure)
