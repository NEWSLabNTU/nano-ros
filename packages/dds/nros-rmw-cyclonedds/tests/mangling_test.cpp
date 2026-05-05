// Phase 117.X.4 — codegen pipeline produces stock-RMW-shape type
// names.
//
// Verifies that the `.msg` / `.srv` → mangled IDL → idlc → registry
// pipeline yields `dds_topic_descriptor_t::m_typename` strings that
// match exactly what `rmw_cyclonedds_cpp` emits for the same input
// types:
//
//   .msg  → `<pkg>::msg::dds_::<Type>_`
//   .srv  → `<pkg>::srv::dds_::<Svc>_Request_`
//          + `<pkg>::srv::dds_::<Svc>_Response_`
//
// Plus: each Request / Response descriptor's first 24 bytes of CDR
// represent the `cdds_request_header_t` (writer_guid[16] +
// sequence_number) — verified by checking m_size includes the
// header (≥ 24 bytes).

#include <cstdio>
#include <cstring>

#include <dds/dds.h>

extern "C" const dds_topic_descriptor_t *
nros_rmw_cyclonedds_find_descriptor(const char *type_name);

namespace {

int fail_count = 0;

void check_descriptor(const char *type_name, std::size_t min_size_bytes) {
    const dds_topic_descriptor_t *desc =
        nros_rmw_cyclonedds_find_descriptor(type_name);
    if (desc == nullptr) {
        std::fprintf(stderr, "FAIL: %s not registered\n", type_name);
        ++fail_count;
        return;
    }
    if (desc->m_typename == nullptr ||
        std::strcmp(desc->m_typename, type_name) != 0) {
        std::fprintf(stderr,
                     "FAIL: %s descriptor->m_typename = %s\n",
                     type_name,
                     desc->m_typename ? desc->m_typename : "(null)");
        ++fail_count;
        return;
    }
    if (desc->m_size < min_size_bytes) {
        std::fprintf(stderr,
                     "FAIL: %s m_size = %u, expected >= %zu\n",
                     type_name, desc->m_size, min_size_bytes);
        ++fail_count;
        return;
    }
}

} // namespace

int main() {
    // .msg → exactly the rosidl-shape mangling.
    check_descriptor("std_msgs::msg::dds_::String_", 0);

    // .srv → both Request and Response, each ≥ 24 bytes (header
    // alone is 24 bytes; AddTwoInts adds two int64s = 16 more).
    check_descriptor("nros_test::srv::dds_::AddTwoInts_Request_", 24 + 16);
    check_descriptor("nros_test::srv::dds_::AddTwoInts_Response_", 24 + 8);

    if (fail_count > 0) {
        std::fprintf(stderr, "%d descriptor mismatch(es)\n", fail_count);
        return 1;
    }
    std::printf("OK 3 descriptors verified\n");
    return 0;
}
