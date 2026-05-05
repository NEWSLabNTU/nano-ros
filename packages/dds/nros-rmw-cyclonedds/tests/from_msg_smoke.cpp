// Phase 117.X.1 smoke: .msg → mangled IDL → idlc → registry, asks
// the registry for the stock-RMW-shape type name and verifies a
// real Cyclone topic can be created against it.

#include <cstdio>
#include <cstring>

#include <dds/dds.h>

extern "C" const dds_topic_descriptor_t *
nros_rmw_cyclonedds_find_descriptor(const char *type_name);

int main() {
    const char *type_name = "std_msgs::msg::dds_::String_";

    const dds_topic_descriptor_t *desc =
        nros_rmw_cyclonedds_find_descriptor(type_name);
    if (desc == nullptr) {
        std::fprintf(stderr, "registry returned NULL for %s\n", type_name);
        return 1;
    }
    if (std::strcmp(desc->m_typename, type_name) != 0) {
        std::fprintf(stderr,
                     "descriptor m_typename=%s, expected %s\n",
                     desc->m_typename, type_name);
        return 2;
    }

    dds_entity_t pp = dds_create_participant(99, nullptr, nullptr);
    if (pp < 0) return 3;
    dds_entity_t topic = dds_create_topic(pp, desc, "rt/from_msg_smoke",
                                          nullptr, nullptr);
    if (topic < 0) {
        (void) dds_delete(pp);
        return 4;
    }
    (void) dds_delete(pp);
    std::printf("OK %s\n", desc->m_typename);
    return 0;
}
