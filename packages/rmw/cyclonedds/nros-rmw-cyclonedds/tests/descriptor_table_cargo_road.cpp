// Issue 2704 follow-up -- a cargo-road image fits the table its count sizes.
//
// The cargo road registers at run time: `TypeRegistry::get_or_build_raw`
// builds a descriptor from the schema and hands it to
// `try_register_descriptor` under the schema's ROS-form `TYPE_NAME`
// (`pkg/msg/Name`), while the built descriptor's `m_typename` is the DDS form
// (`pkg::msg::dds_::Name_`). `derive_max_descriptor_types` sizes the table at
// ONE row per type. A registration that held both names took two rows, so an
// image of just over half the table's types met `kTryRegisterFull` (and the
// entity create failed with `DescriptorTableFull`) on a table its own count
// had sized.
//
// This registers exactly that image: half the image rows plus one, each in the
// cargo-road shape (ROS-form name, DDS-form `m_typename`). Every one must be
// held, take one row, and resolve under BOTH names -- the DDS form the
// `publisher.cpp` / `subscriber.cpp` / `service.cpp` paths ask by, and the ROS
// form a direct caller may pass. Against the two-key table this fails at the
// first registration past the half.

#include <stdio.h>
#include <string.h>

#include "descriptors.hpp"

using namespace nros_rmw_cyclonedds;

namespace {

constexpr int kMaxImage = 4096;

// Names and descriptors must outlive the table: it stores pointers.
char g_ros[kMaxImage][40];
char g_dds[kMaxImage][48];
dds_topic_descriptor_t g_desc[kMaxImage] = {};

int failures = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        fprintf(stderr, "FAIL: %s\n", what);
        ++failures;
    }
}

} // namespace

int main() {
    const size_t backend_own =
        sizeof(kBackendOwnDescriptorTypes) / sizeof(kBackendOwnDescriptorTypes[0]);
    const size_t rows = max_registered_descriptors();
    // `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES`: the rows the image's count sized.
    const size_t image_rows = rows - backend_own;
    const size_t n = image_rows / 2 + 1;
    const size_t base = registered_descriptor_count();
    if (n > (size_t)kMaxImage || base != 0) {
        fprintf(stderr, "descriptor_table_cargo_road: unexpected table (rows %zu, held %zu)\n",
                rows, base);
        return 1;
    }

    for (size_t i = 0; i < n; ++i) {
        snprintf(g_ros[i], sizeof(g_ros[i]), "pkg/msg/M%zu", i);
        snprintf(g_dds[i], sizeof(g_dds[i]), "pkg::msg::dds_::M%zu_", i);
        // What `dynamic_type_builder.cpp` writes: the fields are `const`, set
        // once on a fresh object.
        *const_cast<const char**>(&g_desc[i].m_typename) = g_dds[i];
        if (try_register_descriptor(g_ros[i], &g_desc[i]) != kTryRegisterOk) {
            fprintf(stderr, "FAIL: registration %zu of %zu refused (table rows %zu)\n", i + 1, n,
                    rows);
            ++failures;
            break;
        }
    }

    check(registered_descriptor_count() == n, "one row per registered type");
    for (size_t i = 0; i < n; ++i) {
        if (find_descriptor(g_dds[i]) != &g_desc[i]) {
            check(false, "a type does not resolve by its DDS form");
            break;
        }
        if (find_descriptor(g_ros[i]) != &g_desc[i]) {
            check(false, "a type does not resolve by its ROS form");
            break;
        }
    }

    // Idempotent under either name: a retry (the parameter-services reconcile
    // retries every spin) and the static-init road's DDS-form registration of
    // the same type take no further row.
    check(try_register_descriptor(g_ros[0], &g_desc[0]) == kTryRegisterOk,
          "re-registering a held type succeeds");
    register_descriptor(g_dds[0], &g_desc[0]);
    check(registered_descriptor_count() == n, "re-registration takes no row");

    // The backend's own types have their rows on top of the image's: after the
    // image fills its rows, the graph type still fits.
    for (size_t i = n; registered_descriptor_count() < image_rows && i < (size_t)kMaxImage; ++i) {
        snprintf(g_dds[i], sizeof(g_dds[i]), "pkg::msg::dds_::F%zu_", i);
        *const_cast<const char**>(&g_desc[i].m_typename) = g_dds[i];
        check(try_register_descriptor(g_dds[i], &g_desc[i]) == kTryRegisterOk,
              "the image fills its own rows");
    }
    static dds_topic_descriptor_t graph = {};
    *const_cast<const char**>(&graph.m_typename) = kBackendOwnDescriptorTypes[0];
    check(try_register_descriptor(kBackendOwnDescriptorTypes[0], &graph) == kTryRegisterOk,
          "the backend's own type has a row past a full image");

    if (failures != 0) {
        fprintf(stderr, "descriptor_table_cargo_road: %d failure(s)\n", failures);
        return 1;
    }
    printf("descriptor_table_cargo_road: OK (%zu types in %zu image rows, %zu backend row(s))\n", n,
           image_rows, backend_own);
    return 0;
}
