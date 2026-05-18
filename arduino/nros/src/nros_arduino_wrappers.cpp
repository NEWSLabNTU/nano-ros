// nros_arduino_wrappers.cpp — Phase 23.4.x.
//
// micro-ROS-shaped wrappers around the real nros-c surface. Split
// out of `nros_arduino.cpp` so the host transport-glue smoke
// (`just test-arduino-transport`) can link the WiFi/locator glue
// in isolation, without needing the cross-compiled `libnros_c.a`
// on the host. The Arduino library link path always pulls both
// files via the precompiled `libnanoros.a`.

#include "nros_arduino.h"

#include <cstring>

extern "C" {

static nros_executor_t g_nros_arduino_executor = {};
static bool g_nros_arduino_executor_ready = false;

int nros_init_with_domain(nros_context_t* ctx, uint8_t domain_id) {
    if (!ctx) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    std::memset(ctx, 0, sizeof(*ctx));
    nros_ret_t rc = nros_support_init(ctx, nullptr, domain_id);
    if (rc != NROS_RET_OK) {
        return rc;
    }
    std::memset(&g_nros_arduino_executor, 0, sizeof(g_nros_arduino_executor));
    rc = nros_executor_init(&g_nros_arduino_executor, ctx,
                            NANO_ROS_DEFAULT_MAX_HANDLES);
    if (rc != NROS_RET_OK) {
        nros_support_fini(ctx);
        return rc;
    }
    g_nros_arduino_executor_ready = true;
    return NROS_RET_OK;
}

int nros_init(nros_context_t* ctx) {
    return nros_init_with_domain(ctx, NANO_ROS_DEFAULT_DOMAIN_ID);
}

int nros_fini(nros_context_t* ctx) {
    if (g_nros_arduino_executor_ready) {
        nros_executor_fini(&g_nros_arduino_executor);
        g_nros_arduino_executor_ready = false;
    }
    if (!ctx) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return nros_support_fini(ctx);
}

int nros_node_create_in(nros_node_t* node, nros_context_t* ctx,
                         const char* name, const char* namespace_) {
    if (!node || !ctx || !name) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    std::memset(node, 0, sizeof(*node));
    return nros_node_init(node, ctx, name,
                          namespace_ ? namespace_ : NANO_ROS_DEFAULT_NAMESPACE);
}

int nros_node_create(nros_node_t* node, nros_context_t* ctx,
                      const char* name) {
    return nros_node_create_in(node, ctx, name, NANO_ROS_DEFAULT_NAMESPACE);
}

int nros_node_destroy(nros_node_t* node) {
    if (!node) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return nros_node_fini(node);
}

int nros_publisher_create(nros_publisher_t* pub,
                           const nros_node_t* node,
                           const char* topic_name,
                           const nros_message_type_t* type_info) {
    if (!pub || !node || !topic_name || !type_info) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    std::memset(pub, 0, sizeof(*pub));
    return nros_publisher_init(pub, node, type_info, topic_name);
}

int nros_publisher_destroy(nros_publisher_t* pub) {
    if (!pub) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return nros_publisher_fini(pub);
}

int nros_publish(const nros_publisher_t* pub,
                  const void* data, size_t len) {
    return nros_publish_raw(pub, static_cast<const uint8_t*>(data), len);
}

int nros_subscription_create(nros_subscription_t* sub,
                              const nros_node_t* node,
                              const char* topic_name,
                              const nros_message_type_t* type_info,
                              nros_subscription_callback_t cb,
                              void* user_ctx) {
    if (!sub || !node || !topic_name || !type_info || !cb) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    std::memset(sub, 0, sizeof(*sub));
    return nros_subscription_init(sub, node, type_info, topic_name,
                                  cb, user_ctx);
}

int nros_subscription_destroy(nros_subscription_t* sub) {
    if (!sub) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return nros_subscription_fini(sub);
}

int nros_client_create(nros_client_t* client,
                        const nros_node_t* node,
                        const char* service_name,
                        const nros_service_type_t* type_info) {
    if (!client || !node || !service_name || !type_info) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    std::memset(client, 0, sizeof(*client));
    nros_ret_t rc = nros_client_init(client, node, type_info, service_name);
    if (rc != NROS_RET_OK) {
        return rc;
    }
    if (g_nros_arduino_executor_ready) {
        rc = nros_executor_add_client(&g_nros_arduino_executor, client);
        if (rc != NROS_RET_OK) {
            nros_client_fini(client);
            return rc;
        }
    }
    return NROS_RET_OK;
}

int nros_client_destroy(nros_client_t* client) {
    if (!client) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return nros_client_fini(client);
}

int nros_spin_once(nros_context_t* /*ctx*/, uint32_t timeout_ms) {
    if (!g_nros_arduino_executor_ready) {
        return NROS_RET_NOT_INIT;
    }
    return nros_executor_spin_some(&g_nros_arduino_executor,
                                   static_cast<uint64_t>(timeout_ms) * 1000000ull);
}

}  // extern "C"
