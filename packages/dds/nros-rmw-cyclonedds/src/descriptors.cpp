// Type-descriptor registry — Phase 117.5.
//
// Maps a string type name (e.g. "nros_test::msg::TestString") to the
// `dds_topic_descriptor_t *` produced by Cyclone's `idlc`. The
// matching IDL-derived translation unit (generated alongside via
// `nros_rmw_cyclonedds_idlc_compile(... TYPE_NAME ...)` in
// `cmake/NrosRmwCycloneddsTypeSupport.cmake`) self-registers via
// `__attribute__((constructor))` at static-init time.
//
// Storage is a small fixed-capacity array — a single nano-ros
// process exercises a handful of message types, not hundreds. No
// heap, no `<map>` dependency — keeps the backend usable on
// alloc-only RTOS targets where Cyclone itself is fine but heap
// allocations during topic creation aren't desirable.
//
// Concurrency: the registry is populated entirely from static-init
// constructors (single-threaded, before `main`). Lookups happen from
// `publisher_create` / `subscriber_create` after `main` starts but
// always under the runtime's executor lock — concurrent registration
// is not supported.

#include "descriptors.hpp"

#include <cstring>

namespace nros_rmw_cyclonedds {

namespace {

constexpr std::size_t kMaxRegisteredTypes = 64;

struct Entry {
    const char *type_name;
    const dds_topic_descriptor_t *descriptor;
};

Entry g_entries[kMaxRegisteredTypes] = {};
std::size_t g_count = 0;

} // namespace

void register_descriptor(const char *type_name,
                         const dds_topic_descriptor_t *descriptor) {
    if (type_name == nullptr || descriptor == nullptr) {
        return;
    }
    // Idempotent: if `type_name` is already registered, leave the
    // existing entry alone. Re-registration with a different
    // descriptor under the same name is silently ignored — this
    // mirrors how Cyclone itself dedupes topic-create with the same
    // name + descriptor.
    for (std::size_t i = 0; i < g_count; ++i) {
        if (std::strcmp(g_entries[i].type_name, type_name) == 0) {
            return;
        }
    }
    if (g_count >= kMaxRegisteredTypes) {
        // Static cap exceeded. The link-time registration can't
        // signal an error from a constructor, so this is dropped on
        // the floor; downstream `publisher_create` will fail with
        // NROS_RMW_RET_UNSUPPORTED for the missing type and the
        // operator will see the failure at runtime.
        return;
    }
    g_entries[g_count++] = Entry{type_name, descriptor};
}

const dds_topic_descriptor_t *find_descriptor(const char *type_name) {
    if (type_name == nullptr) {
        return nullptr;
    }
    for (std::size_t i = 0; i < g_count; ++i) {
        if (std::strcmp(g_entries[i].type_name, type_name) == 0) {
            return g_entries[i].descriptor;
        }
    }
    return nullptr;
}

std::size_t registered_descriptor_count() {
    return g_count;
}

bool action_topic_type(const char *topic_name, const char *type_name,
                       char *out, std::size_t out_cap) {
    std::size_t nlen = topic_name != nullptr ? std::strlen(topic_name) : 0;
    const char *feedback_suffix = "/_action/feedback";
    std::size_t flen = std::strlen(feedback_suffix);
    bool is_feedback =
        nlen >= flen && std::strcmp(topic_name + nlen - flen, feedback_suffix) == 0;
    std::size_t blen = std::strlen(type_name);
    if (!is_feedback) {
        if (blen + 1 > out_cap) return false;
        std::memcpy(out, type_name, blen + 1);
        return true;
    }
    // Action feedback: bare base `<A>_` → `<A>_FeedbackMessage_`. Strip
    // the single trailing `_`, append `_FeedbackMessage_`.
    if (blen > 0 && type_name[blen - 1] == '_') --blen;
    const char *infix = "_FeedbackMessage_";
    std::size_t ilen = std::strlen(infix);
    if (blen + ilen + 1 > out_cap) return false;
    std::memcpy(out, type_name, blen);
    std::memcpy(out + blen, infix, ilen);
    out[blen + ilen] = '\0';
    return true;
}

} // namespace nros_rmw_cyclonedds

// C entry point used by IDL-derived registration TUs. Lives outside
// the namespace so the auto-generated `_register.c` constructor
// (compiled as C) can find it via plain symbol lookup.
//
// Phase 212.K.7.7 — also alias the descriptor under its own
// `m_typename` (the mangled `pkg::msg::dds_::Name_` form) when the
// caller-supplied `type_name` differs. The Rust runtime registry
// (`nros_rmw_cyclonedds::register::<M>()`) passes the unmangled
// `nros_serdes::Message::TYPE_NAME` (`pkg/msg/Name`), but
// `publisher_create` / `subscriber_create` look up by the mangled
// `RosMessage::TYPE_NAME` baked at codegen time. Registering both
// keys keeps both call sites working without forcing every consumer
// to memorise which form lands in which table.
extern "C" void nros_rmw_cyclonedds_register_descriptor(
    const char *type_name,
    const dds_topic_descriptor_t *descriptor) {
    nros_rmw_cyclonedds::register_descriptor(type_name, descriptor);
    if (descriptor != nullptr && descriptor->m_typename != nullptr &&
        (type_name == nullptr ||
         std::strcmp(descriptor->m_typename, type_name) != 0)) {
        nros_rmw_cyclonedds::register_descriptor(descriptor->m_typename,
                                                 descriptor);
    }
}

extern "C" const dds_topic_descriptor_t *
nros_rmw_cyclonedds_find_descriptor(const char *type_name) {
    return nros_rmw_cyclonedds::find_descriptor(type_name);
}
