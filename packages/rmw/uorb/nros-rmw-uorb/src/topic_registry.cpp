// Phase 115.K.4.3 — topic-name → orb_metadata* registry.
//
// uORB has no built-in name-keyed metadata lookup; topics are
// referenced by descriptor pointer (`ORB_ID(name)`). The cffi
// vtable's `create_publisher`/`create_subscription` only receive
// `(topic_name, type_name)` strings, so the host PX4 module must
// register every topic it intends to expose by mapping the
// ROS-style names back to the static descriptor.
//
// Storage is a fixed-capacity array. uORB topic counts in
// production PX4 modules are bounded (~30–60 distinct topics
// per module); the 64 default cap is generous. Tunable via
// `-DNROS_RMW_UORB_REGISTRY_CAPACITY=<N>` at compile time.

#include "nros_rmw_uorb_registry.h"

#include "nros/rmw_ret.h"

#include <cstring>

#ifndef NROS_RMW_UORB_REGISTRY_CAPACITY
#define NROS_RMW_UORB_REGISTRY_CAPACITY 64
#endif

namespace {

constexpr size_t kCapacity = NROS_RMW_UORB_REGISTRY_CAPACITY;

struct Entry {
    const char *topic_name;
    const char *type_name;
    const struct orb_metadata *meta;
};

// Static storage. Zero-initialised at program start; entries with
// `meta == nullptr` are empty slots.
Entry g_table[kCapacity];
size_t g_count = 0;

bool eq(const char *a, const char *b) {
    if (a == nullptr || b == nullptr) {
        return a == b;
    }
    return std::strcmp(a, b) == 0;
}

} // namespace

extern "C" {

nros_rmw_ret_t nros_rmw_uorb_register_topic(const char *topic_name,
                                            const char *type_name,
                                            const struct orb_metadata *meta) {
    if (topic_name == nullptr || type_name == nullptr || meta == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Idempotent: same triple → no-op.
    for (size_t i = 0; i < g_count; ++i) {
        if (eq(g_table[i].topic_name, topic_name)
            && eq(g_table[i].type_name, type_name)
            && g_table[i].meta == meta) {
            return NROS_RMW_RET_OK;
        }
    }
    if (g_count >= kCapacity) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    g_table[g_count++] = Entry{topic_name, type_name, meta};
    return NROS_RMW_RET_OK;
}

const struct orb_metadata *nros_rmw_uorb_lookup_topic(const char *topic_name) {
    if (topic_name == nullptr) {
        return nullptr;
    }
    for (size_t i = 0; i < g_count; ++i) {
        if (eq(g_table[i].topic_name, topic_name)) {
            return g_table[i].meta;
        }
    }
    return nullptr;
}

void nros_rmw_uorb_clear_registry(void) {
    for (size_t i = 0; i < g_count; ++i) {
        g_table[i] = Entry{nullptr, nullptr, nullptr};
    }
    g_count = 0;
}

} // extern "C"
