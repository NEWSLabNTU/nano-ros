// phase-442 W3 — the two freestanding mechanisms, compiled.
//
// RFC-0096 D2 says the two capabilities the census called "genuinely hard" —
// shared-looking ownership and type-erased callbacks — have freestanding
// answers. This is the translation unit that makes that a measurement rather
// than a claim, and `check-cpp-freestanding-mechanisms` compiles it in all
// three configurations this project ships: hosted, `arm-none-eabi
// -ffreestanding`, and the ThreadX `-nostdinc++` shim.
//
// Its negative half is `freestanding_mechanisms_overbudget_probe.cpp`, which
// must FAIL with a `static_assert` naming the capacity knob.

#include "nros/traits.hpp"
#include "nros/handle.hpp"
#include "nros/inplace_fn.hpp"

namespace tr = nros::tr;
static_assert(tr::is_same<tr::decay<const int&>::type, int>::value, "decay cv-ref");
static_assert(tr::is_same<tr::decay<int[4]>::type, int*>::value, "decay array");
static_assert(tr::is_same<tr::decay<void(int)>::type, void (*)(int)>::value, "decay function");

struct Msg {
    int data;
};
struct Sub {
    int n = 0;
};

static Sub g_sub;

// Handle
static_assert(sizeof(nros::Handle<Sub>) == sizeof(void*), "one pointer");
static void handle_ops() {
    nros::Handle<Sub> a;
    nros::Handle<Sub> b(&g_sub);
    nros::Handle<Sub> c = b;       // copyable
    nros::Handle<const Sub> d = c; // converts to const
    a = c;                         // assignable
    (void)(a == b);
    (void)(a != nullptr);
    if (a) {
        a->n = d->n;
        (*a).n = 1;
    }
    a.reset();
}

// InplaceFn — the shapes W0 measured
static int g_state;
struct Obj {
    void method(const Msg&);
};
static Obj g_obj;

using Cb = nros::InplaceFn<void(const Msg&)>;

static Cb c_empty([](const Msg&) {});
static Cb c_this([sub = &g_sub](const Msg& m) { sub->n += m.data; });
static Cb c_two([sub = &g_sub, st = &g_state](const Msg& m) { sub->n = *st + m.data; });
static Cb c_obj_method([o = &g_obj, mp = &Obj::method](const Msg& m) { (o->*mp)(m); });

extern "C" int nros_w3_probe(const Msg& m) {
    handle_ops();
    c_empty(m);
    c_this(m);
    c_two(m);
    c_obj_method(m);
    Cb moved(static_cast<Cb&&>(c_this));
    if (moved.valid()) {
        moved(m);
    }
    moved.clear();
    return g_sub.n + static_cast<int>(moved.valid());
}
