// phase-329 W5 — build-stage form of the platform_header_matrix
// `baremetal/cpp/heap-has-malloc` cell (#38 fix gate). Opting into
// NROS_PLATFORM_HAS_MALLOC exposes malloc/free over alloc/dealloc, so the heap
// containers MUST compile. See tests/platform_header_compile.rs.
#define NROS_PLATFORM_BAREMETAL
#define NROS_PLATFORM_HAS_MALLOC
#include <nros/heap_string.hpp>
#include <nros/heap_sequence.hpp>
namespace {
void use_it() {
    nros::HeapString s;
    (void)s;
    nros::HeapSequence<int> q;
    q.reserve(4);
    q.push_back(1);
    (void)q;
}
} // namespace
