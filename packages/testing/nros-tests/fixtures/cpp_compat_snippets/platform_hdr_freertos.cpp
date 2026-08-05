// phase-329 W5 — build-stage form of the platform_header_matrix `freertos/cpp/heap`
// cell (#42 root-cause #5). A non-bare-metal target is heap-capable by default, so
// the heap containers over the canonical header MUST compile. See
// tests/platform_header_compile.rs.
#define NROS_PLATFORM_FREERTOS
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
