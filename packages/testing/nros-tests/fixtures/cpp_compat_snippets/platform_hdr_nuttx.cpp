// phase-329 W5 — build-stage form of the platform_header_matrix `nuttx/cpp/heap`
// cell (#42 root-cause #5). Heap-capable by default → heap containers MUST compile.
// See tests/platform_header_compile.rs.
#define NROS_PLATFORM_NUTTX
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
