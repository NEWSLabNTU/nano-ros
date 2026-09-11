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
#include "nros/owned.hpp"

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

// --- Owned<T>: an entity kept BY VALUE, spoken to as a pointer (W8) ---------
//
// The stand-in carries the same move contract the real entities do: a
// relocation that the C ABI documents as `ptr::read` + `ptr::write`, counted
// here so the probe measures the move rather than assuming it.
static int g_relocations = 0;

class FakePublisher {
  public:
    FakePublisher() : initialized_(false) { storage_[0] = 0; }
    FakePublisher(FakePublisher&& o) : initialized_(o.initialized_) {
        if (o.initialized_) {
            ++g_relocations;
            storage_[0] = o.storage_[0];
            o.initialized_ = false;
        }
    }
    FakePublisher& operator=(FakePublisher&& o) {
        if (this != &o) {
            initialized_ = o.initialized_;
            if (o.initialized_) {
                ++g_relocations;
                storage_[0] = o.storage_[0];
                o.initialized_ = false;
            }
        }
        return *this;
    }
    FakePublisher(const FakePublisher&) = delete;
    FakePublisher& operator=(const FakePublisher&) = delete;

    void publish(const Msg&) {}
    void mark_live() { initialized_ = true; }

    using SharedPtr = nros::Owned<FakePublisher>;

  private:
    alignas(8) unsigned char storage_[64];
    bool initialized_;
};

// The ported shape: a member holds the entity, and the factory returns it.
class PortedNode {
  public:
    PortedNode() { pub_ = make(); }
    void tick() {
        Msg m{1};
        pub_->publish(m);
        if (pub_) {
            (*pub_).publish(m);
        }
    }

  private:
    static FakePublisher::SharedPtr make() {
        FakePublisher p;
        p.mark_live();
        return FakePublisher::SharedPtr(static_cast<FakePublisher&&>(p));
    }
    FakePublisher::SharedPtr pub_;
};

static_assert(!nros::tr::is_same<FakePublisher::SharedPtr, FakePublisher*>::value,
              "Owned is not a raw pointer");

extern "C" int nros_w8_owned_probe() {
    PortedNode n;
    n.tick();
    nros::Owned<FakePublisher> empty;
    if (empty != nullptr) {
        return -1;
    }
    empty.reset();
    return g_relocations;
}
