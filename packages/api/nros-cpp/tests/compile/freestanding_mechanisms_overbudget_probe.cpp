// phase-442 W3 — EXPECTED-FAILURE probe: a capture larger than the budget.
//
// RFC-0096 D5 enumerates the three things that are not drop-in, and this is the
// third: a lambda capture larger than the declared capacity. It must be a
// COMPILE ERROR naming the knob, because the alternatives are a silent heap
// fallback (which defeats the point on a target with no allocator) or a runtime
// failure (which moves a compile-time fact to the field).
//
// `check-cpp-freestanding-mechanisms` compiles this and requires that it FAIL,
// with `NROS_CPP_CALLBACK_CAPACITY` in the diagnostic. A probe nobody compiles
// is a comment; a probe whose failure text is not checked passes on the wrong
// error.

#include "nros/inplace_fn.hpp"

struct Msg {
    int data;
};

// Six pointers of capture against a four-pointer budget.
struct Wide {
    void* a;
    void* b;
    void* c;
    void* d;
    void* e;
    void* f;
};

static Wide g_wide;

nros::InplaceFn<void(const Msg&)> g_over([w = g_wide](const Msg&) { (void)w; });
