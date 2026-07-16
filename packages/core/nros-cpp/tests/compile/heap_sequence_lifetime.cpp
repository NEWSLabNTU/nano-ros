// issue #201 — RUNTIME lifetime probe for nros::HeapSequence element
// destructor semantics (compiled AND executed by `just check-cpp`).
//
// Verifies, with a counting allocator, that a two-level heap shape — a heap
// sequence whose elements own heap memory (HeapString + nested HeapSequence,
// the hand-written stand-in for a generated message struct with heap fields)
// — releases EVERY allocation across: destructor, move-assign, clear(), and
// reserve() growth (byte relocation of owning elements). Exits non-zero with
// a message on any imbalance.

#include <cstdint>
#include <cstdio>
#include <cstdlib>

#include <nros/heap_sequence.hpp>
#include <nros/heap_string.hpp>

// `nros_platform_malloc`/`free` are static-inline forwards to the
// `nros_platform_alloc`/`dealloc` funnel (RFC-0034 D6), which platform.h only
// DECLARES — this test provides counting implementations so no platform
// library is needed.
static long g_live = 0;
extern "C" void* nros_platform_alloc(size_t size) {
    ++g_live;
    return std::malloc(size);
}
extern "C" void nros_platform_dealloc(void* ptr) {
    if (ptr != nullptr) --g_live;
    std::free(ptr);
}

namespace {

// Stand-in for a generated message struct with heap fields (the two-level
// `mode = "heap"` shape: DiagnosticStatus-like).
struct Inner {
    nros::HeapString name;
    nros::HeapSequence<int32_t> values;
};

int fail(const char* what, long live) {
    std::fprintf(stderr, "heap_sequence_lifetime: %s leaked (live allocations: %ld)\n", what,
                 live);
    return 1;
}

int check_destructor() {
    {
        nros::HeapSequence<Inner> outer;
        for (int k = 0; k < 3; ++k) {
            Inner* e = outer.emplace_back();
            if (e == nullptr) return fail("emplace_back alloc", g_live);
            e->name.assign("motor_left", 10);
            for (int32_t v = 0; v < 8; ++v) e->values.push_back(v);
        }
    }
    return g_live != 0 ? fail("destructor", g_live) : 0;
}

int check_move_assign() {
    {
        nros::HeapSequence<Inner> a;
        Inner* e = a.emplace_back();
        e->name.assign("x", 1);
        e->values.push_back(42);

        nros::HeapSequence<Inner> b;
        Inner* f = b.emplace_back();
        f->name.assign("overwritten", 11);
        f->values.push_back(7);

        b = static_cast<nros::HeapSequence<Inner>&&>(a); // must tear down b's old elements
        if (b.length() != 1 || b[0].values[0] != 42) {
            std::fprintf(stderr, "heap_sequence_lifetime: move-assign lost contents\n");
            return 1;
        }
    }
    return g_live != 0 ? fail("move-assign", g_live) : 0;
}

int check_clear() {
    {
        nros::HeapSequence<Inner> outer;
        Inner* e = outer.emplace_back();
        e->name.assign("cleared", 7);
        outer.clear();
        if (g_live != 0) return fail("clear()", g_live);
    }
    return g_live != 0 ? fail("clear scope-exit", g_live) : 0;
}

int check_reserve_relocation() {
    {
        nros::HeapSequence<Inner> outer;
        // Force several growth relocations past the initial capacity of 4.
        for (int k = 0; k < 33; ++k) {
            Inner* e = outer.emplace_back();
            if (e == nullptr) return fail("emplace_back growth alloc", g_live);
            e->name.assign("grow", 4);
            e->values.push_back(k);
        }
        // Elements must survive relocation intact (owning pointers moved, not
        // destructed).
        for (int k = 0; k < 33; ++k) {
            if (outer[static_cast<size_t>(k)].values[0] != k) {
                std::fprintf(stderr, "heap_sequence_lifetime: relocation corrupted element %d\n",
                             k);
                return 1;
            }
        }
    }
    return g_live != 0 ? fail("reserve relocation", g_live) : 0;
}

int check_pod_push_back() {
    {
        nros::HeapSequence<uint8_t> pixels;
        for (int k = 0; k < 100; ++k) pixels.push_back(static_cast<uint8_t>(k));
        if (pixels.length() != 100) {
            std::fprintf(stderr, "heap_sequence_lifetime: POD push_back size wrong\n");
            return 1;
        }
    }
    return g_live != 0 ? fail("POD sequence", g_live) : 0;
}

} // namespace

int main() {
    int rc = 0;
    rc |= check_destructor();
    rc |= check_move_assign();
    rc |= check_clear();
    rc |= check_reserve_relocation();
    rc |= check_pod_push_back();
    if (rc == 0) std::puts("heap_sequence_lifetime: OK");
    return rc;
}
