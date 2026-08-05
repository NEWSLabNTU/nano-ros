// phase-329 W5 — build-stage form of the platform_header_matrix
// `baremetal/cpp/core-no-malloc` cell. The non-heap platform surface (atomics)
// MUST compile without a heap — proves the negative heap cell fails on the
// MISSING malloc, not a broken header. See tests/platform_header_compile.rs.
#define NROS_PLATFORM_BAREMETAL
#include <nros/platform.h>
namespace {
bool roundtrip(bool* p) {
    nros_platform_atomic_store_bool(p, true);
    return nros_platform_atomic_load_bool(p);
}
} // namespace
