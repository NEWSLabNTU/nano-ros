// phase-329 W5 — build-stage form of the platform_header_matrix `posix/cpp/heap`
// cell. The -D defines are baked as #define (the shared cxx-syntax builder takes
// no per-row defines). Heap containers over the canonical POSIX platform header
// MUST compile (nros_platform_malloc/free are declared). See
// tests/platform_header_compile.rs.
#define NROS_PLATFORM_POSIX
#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE
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
