// nros-cpp: the fixed-capacity type-erased callable
// Freestanding C++ — no exceptions, no STL required

/**
 * @file inplace_fn.hpp
 * @ingroup grp_support
 * @brief `nros::InplaceFn<Sig, Cap>` — a capturing lambda, with no heap.
 */

#ifndef NROS_CPP_INPLACE_FN_HPP
#define NROS_CPP_INPLACE_FN_HPP

#include <new> // the PLACEMENT forms only -- freestanding-guaranteed

#include "nros/traits.hpp"

/// How many bytes of capture a callback may carry — phase-442 W0.
///
/// MEASURED, not chosen. Every capture in this tree and in the porting corpus
/// is a whole number of POINTERS, and the widest is three:
///
///   capture          x86_64   cortex-m3   riscv64
///   []                    1           1         1
///   [this]                8           4         8
///   [this, state]        16           8        16
///   [obj, method]        24          12        24
///
/// `[obj, method]` is `diagnostic_updater`'s, and it is wide because a
/// pointer-to-member-function is TWO words on the Itanium ABI. The default is
/// that widest shape plus one pointer of headroom.
///
/// SPELLED IN POINTERS, and that is the whole reason it is a `sizeof`
/// expression rather than a number: a byte count is correct on one word size
/// and wrong on the other, which is how a 32-bit target ends up either failing
/// to compile idiomatic code or paying 64-bit prices for it.
///
/// Cost, measured on eight registered callbacks at `-Os`, freestanding:
/// `align8(Cap) + 8` bytes of `.bss` each — 24 on cortex-m3 at this default, 40
/// on riscv64 — and `.text` FLAT across every candidate capacity (256 -> 276
/// bytes over capacities 8 through 48). Capacity is a `.bss` knob; raising it
/// does not grow code.
#ifndef NROS_CPP_CALLBACK_CAPACITY
#define NROS_CPP_CALLBACK_CAPACITY (4 * sizeof(void*))
#endif

namespace nros {

template <typename Sig, tr::size_type Cap = NROS_CPP_CALLBACK_CAPACITY> class InplaceFn;

/// A callable with INLINE storage and a compile-time capacity — RFC-0096 D2.
///
/// WHAT IT IS FOR
///
/// The tree already had every non-owning form of a callback: a raw
/// `void(*)(void*)` plus a context, typed function-pointer aliases, and the
/// member-pointer-as-template-parameter `bind_*` family. What was missing is
/// the CAPTURING LAMBDA, which is what ported rclcpp source actually writes:
///
/// ```cpp
/// sub_ = this->create_subscription<Msg>("chatter", 10,
///     [this](const Msg& m) { this->count_ += m.data; });
/// ```
///
/// `std::function` supplies that hosted, at the price of a heap allocation and
/// `<functional>`. This supplies it with neither: the capture is copied into
/// storage that lives inside the entity, and dispatch is one indirect call
/// through a function pointer that knows the capture's type.
///
/// AN OVER-LARGE CAPTURE IS A COMPILE ERROR, DELIBERATELY
///
/// The alternative designs are a silent heap fallback (which defeats the point
/// on a target with no allocator, and hides the cost on one that has) or a
/// runtime failure (which moves a compile-time fact to the field). A
/// `static_assert` naming the knob is the mechanical edit RFC-0089 asks for:
/// the compiler points at the exact callback and says which macro to raise.
///
/// NOT A `std::function`, AND NOT PRETENDING TO BE
///
/// It does not allocate, it does not throw, it has no `target()` and no
/// `target_type()`, and an empty `InplaceFn` invoked is undefined rather than
/// throwing `bad_function_call` — `valid()` is how a caller asks. Nothing in
/// this API invokes one without having stored one.
template <typename R, typename... A, tr::size_type Cap> class InplaceFn<R(A...), Cap> {
  public:
    /// The capacity this instantiation was built with, so a `static_assert` in
    /// a consumer can name a number rather than re-deriving it.
    static constexpr tr::size_type capacity = Cap;

    constexpr InplaceFn() : invoke_(nullptr), destroy_(nullptr), storage_() {}
    constexpr InplaceFn(decltype(nullptr)) : InplaceFn() {}

    /// Store `f`. Constrained off the two special members so that a copy does
    /// not bind here — an unconstrained template constructor is greedier than
    /// the copy constructor and would recurse.
    template <typename F,
              typename tr::enable_if<!tr::is_same<typename tr::decay<F>::type, InplaceFn>::value,
                                     int>::type = 0>
    InplaceFn(F&& f)
        : invoke_(&invoke_impl<typename tr::decay<F>::type>),
          destroy_(&destroy_impl<typename tr::decay<F>::type>) {
        using D = typename tr::decay<F>::type;
        static_assert(sizeof(D) <= Cap,
                      "callback capture too large -- raise NROS_CPP_CALLBACK_CAPACITY");
        static_assert(alignof(D) <= alignof(MaxAlign),
                      "callback capture is over-aligned for the inline storage");
        new (static_cast<void*>(&storage_)) D(static_cast<F&&>(f));
    }

    ~InplaceFn() { clear(); }

    InplaceFn(const InplaceFn&) = delete;
    InplaceFn& operator=(const InplaceFn&) = delete;

    /// Moves are a BYTE COPY of the storage plus a source reset, which is legal
    /// here and not in general: the only things stored are lambda closures over
    /// pointers and small trivially-copyable state, and a type whose move is
    /// not a byte copy cannot fit the capacity in the first place. Stated
    /// rather than assumed, because "trivially relocatable" is a property this
    /// class cannot check in C++14.
    InplaceFn(InplaceFn&& other) : invoke_(other.invoke_), destroy_(other.destroy_) {
        copy_storage(other);
        other.release();
    }

    InplaceFn& operator=(InplaceFn&& other) {
        if (this != &other) {
            clear();
            invoke_ = other.invoke_;
            destroy_ = other.destroy_;
            copy_storage(other);
            other.release();
        }
        return *this;
    }

    R operator()(A... args) const {
        return invoke_(const_cast<Storage*>(&storage_), static_cast<A&&>(args)...);
    }

    /// Whether a callable is stored. An empty one must not be invoked.
    constexpr bool valid() const { return invoke_ != nullptr; }
    explicit constexpr operator bool() const { return invoke_ != nullptr; }

    void clear() {
        if (destroy_ != nullptr) {
            destroy_(&storage_);
        }
        release();
    }

  private:
    /// The widest fundamental alignment a small capture can need. `long long`
    /// and `double` rather than `max_align_t`, which lives in `<cstddef>` and
    /// is not what every shim provides.
    union MaxAlign {
        long long a;
        double b;
        void* c;
        void (*d)();
    };

    struct alignas(MaxAlign) Storage {
        unsigned char bytes[Cap];
    };

    template <typename D> static R invoke_impl(Storage* s, A... args) {
        return (*reinterpret_cast<D*>(s))(static_cast<A&&>(args)...);
    }

    template <typename D> static void destroy_impl(Storage* s) { reinterpret_cast<D*>(s)->~D(); }

    void copy_storage(const InplaceFn& other) {
        for (tr::size_type i = 0; i < Cap; ++i) {
            storage_.bytes[i] = other.storage_.bytes[i];
        }
    }

    void release() {
        invoke_ = nullptr;
        destroy_ = nullptr;
    }

    R (*invoke_)(Storage*, A...);
    void (*destroy_)(Storage*);
    Storage storage_;
};

} // namespace nros

#endif // NROS_CPP_INPLACE_FN_HPP
