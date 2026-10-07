// issue 1656 — build-stage form of the platform-header #38 NEGATIVE cell
// (`cxx-syntax-verdict` row). The bare-metal default is NROS_NO_DYNAMIC_MEMORY,
// so the canonical malloc/free are ABSENT and the heap containers MUST NOT
// compile: `HeapString`'s dtor references `nros_platform_free`,
// `HeapSequence<int>::reserve/push_back` references `nros_platform_malloc`.
// The positive twin is `platform_hdr_baremetal_has_malloc.cpp`, identical but
// for NROS_PLATFORM_HAS_MALLOC. See tests/platform_header_compile.rs.
#define NROS_PLATFORM_BAREMETAL
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
