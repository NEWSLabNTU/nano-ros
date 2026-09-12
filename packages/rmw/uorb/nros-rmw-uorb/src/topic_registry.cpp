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
// `-DNROS_RMW_UORB_REGISTRY_CAPACITY=<N>` at compile time, and
// phase-454 W6.d DERIVES it: the image's distinct topic count,
// over the publishers and subscriptions its contract declares,
// deduplicated by topic name. `NrosRmwUorbSizing.cmake` is the
// one producer; an image with no sizing descriptor keeps the 64.

#include "nros_rmw_uorb_registry.h"

#include "nros/rmw_ret.h"

#include <cstring>

#ifndef NROS_RMW_UORB_REGISTRY_CAPACITY
#define NROS_RMW_UORB_REGISTRY_CAPACITY 64
#endif

/* issue 1131 / phase-454 W6.d — this knob sizes `Entry g_table[N]`, and 0 is
 * not a legal size here for the same LANGUAGE reason as the `Slot g_pool[N]`
 * next door: ISO C++ has no zero-size array, and this TU is built `-Wpedantic`
 * inside a `-Werror` PX4. So the question "does the runtime survive an empty
 * registry?" is not the whole question — the image does not COMPILE.
 *
 * It matters now because the capacity is derived. Before W6.d the only way to
 * reach 0 was a person typing `-D...=0`; now an image whose contract declares
 * no publishers and no subscriptions derives exactly 0 distinct topics, and
 * that is a legitimate DEMAND (RFC-0100 D7) which `_nros_c_array_pool_floor`
 * raises at the consumer. This is the backstop that binds a producer the floor
 * never reached.
 *
 * Below the `#ifndef`, never above: an undefined identifier reads as 0 in
 * `#if`, so a guard above its own default fires on every build (issue 1167). */
#if NROS_RMW_UORB_REGISTRY_CAPACITY < 1
#error "NROS_RMW_UORB_REGISTRY_CAPACITY must be >= 1: ISO C++ forbids a zero-size array (1131)"
#endif

namespace {

struct Entry {
    const char *topic_name;
    const char *type_name;
    const struct orb_metadata *meta;
};

// Static storage. Zero-initialised at program start; entries with
// `meta == nullptr` are empty slots.
//
// The extent is the MACRO and not a `constexpr` alias of it, deliberately.
// `check-c-array-pool-floors` discovers a knob-sized array by intersecting the
// `#ifndef` knobs with the ALL-CAPS array extents in the file, so the
// `constexpr size_t kCapacity = ...` this line used to read through laundered
// the macro straight past the gate: the audit reported 24 knob-sized arrays
// over a tree that had 25, and this was the one it could not see.
Entry g_table[NROS_RMW_UORB_REGISTRY_CAPACITY];
constexpr size_t kCapacity = NROS_RMW_UORB_REGISTRY_CAPACITY;
size_t g_count = 0;

bool eq(const char *a, const char *b) {
    if (a == nullptr || b == nullptr) {
        return a == b;
    }
    return std::strcmp(a, b) == 0;
}

} // namespace

extern "C" {

rmw_ret_t nros_rmw_uorb_register_topic(const char *topic_name,
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
