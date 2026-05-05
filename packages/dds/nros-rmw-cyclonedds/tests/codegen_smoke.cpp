// Phase 117.2 / 117.5 codegen + registry smoke.
//
// Exercises the full flow:
//   1. CMake calls `idlc` on `tests/types/test_string.idl` → emits
//      `test_string.{c,h}` with `nros_test_msg_TestString_desc`.
//   2. `nros_rmw_cyclonedds_idlc_compile(... TYPE_NAME ...)` emits a
//      `_register.c` constructor that calls
//      `nros_rmw_cyclonedds_register_descriptor(...)` at static init.
//   3. This test program asks the registry for the type by name and
//      verifies a real Cyclone topic can be created from it.

#include <cstdio>
#include <cstring>

#include <dds/dds.h>

extern "C" const dds_topic_descriptor_t *
nros_rmw_cyclonedds_find_descriptor(const char *type_name);

int main() {
    const char *type_name = "nros_test::msg::TestString";

    const dds_topic_descriptor_t *desc =
        nros_rmw_cyclonedds_find_descriptor(type_name);
    if (desc == nullptr) {
        std::fprintf(stderr, "registry returned NULL for %s\n", type_name);
        return 1;
    }
    if (desc->m_typename == nullptr) {
        std::fprintf(stderr, "descriptor has NULL m_typename\n");
        return 2;
    }
    // Cyclone embeds the IDL-derived FQ name as `m_typename`; for our
    // IDL it should be `nros_test::msg::TestString`.
    if (std::strcmp(desc->m_typename, "nros_test::msg::TestString") != 0) {
        std::fprintf(stderr, "unexpected m_typename: %s\n", desc->m_typename);
        return 3;
    }

    // Round-trip the descriptor through Cyclone: create a participant
    // on a private domain, create a topic from the registered
    // descriptor, then tear everything down.
    dds_entity_t pp = dds_create_participant(99, nullptr, nullptr);
    if (pp < 0) {
        std::fprintf(stderr, "dds_create_participant failed: %d\n",
                     static_cast<int>(pp));
        return 4;
    }

    dds_entity_t topic = dds_create_topic(pp, desc, "rt/test_string",
                                          nullptr, nullptr);
    if (topic < 0) {
        std::fprintf(stderr, "dds_create_topic failed: %d\n",
                     static_cast<int>(topic));
        (void) dds_delete(pp);
        return 5;
    }

    (void) dds_delete(pp);  // cascades to topic
    std::printf("OK type=%s\n", desc->m_typename);
    return 0;
}
