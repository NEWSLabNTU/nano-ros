// nros-cpp: the fixed-capacity sequence VALUE used by array parameters
// Freestanding C++ - no exceptions, no STL required, no heap

/**
 * @file parameter.hpp
 * @ingroup grp_parameter
 * @brief `nros::Seq<T, N>` - the fixed-capacity, inline sequence VALUE a
 *        freestanding node declares an array parameter with.
 *
 * ## What this file used to be, and why it is not that any more (phase-426 W4)
 *
 * Until now this header also defined `nros::ParameterServer<Capacity,
 * SeqSlots, SeqPoolBytes>`: a second parameter STORE, inline on the caller,
 * over the caller-storage C `nros_parameter_server_t`. The first half of W4
 * deleted the two node-owned stores (`rclcpp::Node`'s inline member and
 * `ComponentNode`'s facade) so a parameter declared in C++ lands in the one
 * `nros_params::ParameterServer` the six `rcl_interfaces/srv/*` servers read
 * - and left this one standing, still shipped and still the whole of
 * `examples/native/cpp/parameters`. Its parameters were invisible to
 * `ros2 param get`, which is the exact defect phase-426 exists to remove.
 *
 * A caller-owned store with no executor turned out to have no consumer:
 * nothing in the tree constructed one except the example that documented it.
 * RFC-0019/0020 and RFC-0089 §"Parameters" put storage, typing, the
 * read-only and range rules and the node keying in Rust and leave the C++
 * API a call-shape adapter, so the class is ABSENT now - RFC-0089's fourth
 * disposition, and the honest one here because rclcpp never had the name, so
 * no ported alias is load-bearing. A node's parameters are
 * `rclcpp::Node::declare_parameter<T>` / `get_parameter<T>` /
 * `set_parameter<T>` / `has_parameter`, forwarded by
 * `nros/node_parameters.hpp`.
 *
 * ## What did NOT go, and why this file survives
 *
 * `Seq<T, N>` is a VALUE, not a store, and it was the only way a
 * freestanding (`-nostdinc++`) caller could express an array parameter:
 * `std::vector<T>` reaches the array FFI only under `NROS_CPP_STD`. Deleting
 * the store without it would have dropped that capability quietly, which is
 * the failure mode W4's ordering rule exists to prevent. So `Seq` moved ONTO
 * the store instead - `node_parameters.hpp` declares and reads it through
 * `nros_cpp_node_{declare,get}_param_{double,integer,bool}_array`, and a
 * sequence parameter is now visible to `ros2 param get` like every scalar.
 * That is strictly more than the deleted class had: the pre-#226 header hid
 * sequences from the C server entirely, and the post-#226 one exposed them
 * only through a `raw()` accessor documented for C helpers that do not
 * exist.
 *
 * ## Capacity model (RFC-0044 Q3) - unchanged
 *
 * The per-parameter capacity is the compile-time `N` of the `Seq<T, N>` the
 * caller declares. The ELEMENTS are owned by the Rust store now (a
 * `heapless::Vec` in its slot), not by an inline bump pool here, so there is
 * no borrow for the caller to keep alive. Over-`N` construction truncates
 * and records `overflowed()`; a read into a too-small `Seq` is rejected with
 * an error code, never UB.
 */

#ifndef NROS_CPP_PARAMETER_HPP
#define NROS_CPP_PARAMETER_HPP

#include <cstddef>
#include <cstdint>
#include <initializer_list>
// Freestanding C++ (`-ffreestanding`) often only puts `size_t` in the
// global namespace via `<stddef.h>`; include it so `::size_t` (used
// below instead of `std::size_t`) is always resolvable.
#include <stddef.h>

#ifdef NROS_CPP_STD
#include <vector>
#endif

namespace nros {

/// Fixed-capacity, inline sequence value — the `no_std` stand-in for
/// `std::vector<T>` at the parameter API surface.
///
/// Storage is `N` inline `T` slots; no heap. `size()` is the current
/// element count (`0 <= size() <= N`). Over-capacity `push_back` /
/// construction is rejected (returns `false` / truncates to `N` with a
/// recorded overflow flag), never UB.
///
/// `T` is `double`, `int64_t`, or `bool` for parameter use, but the type
/// itself is element-agnostic.
template <typename T, ::size_t N> class Seq {
  public:
    Seq() : size_(0) {}

    /// Construct from a brace-init list. Elements past `N` are dropped
    /// and `overflowed()` returns true.
    Seq(std::initializer_list<T> il) : size_(0) {
        for (const T& v : il) {
            if (!push_back(v)) {
                overflow_ = true;
            }
        }
    }

    /// Construct from a raw pointer + length. Length past `N` is dropped.
    Seq(const T* src, ::size_t n) : size_(0) {
        for (::size_t i = 0; i < n; ++i) {
            if (!push_back(src[i])) {
                overflow_ = true;
            }
        }
    }

#ifdef NROS_CPP_STD
    /// Build from a `std::vector<T>` (hosted convenience). The *value* is
    /// copied into the inline storage; the vector is not retained. Length
    /// past `N` is dropped and `overflowed()` returns true.
    explicit Seq(const std::vector<T>& v) : size_(0) {
        for (const T& e : v) {
            if (!push_back(e)) {
                overflow_ = true;
            }
        }
    }

    /// Copy the current elements into a `std::vector<T>` (hosted).
    std::vector<T> to_vector() const {
        std::vector<T> out;
        out.reserve(size_);
        for (::size_t i = 0; i < size_; ++i) {
            out.push_back(data_[i]);
        }
        return out;
    }
#endif

    /// Maximum number of elements (compile-time `N`).
    static constexpr ::size_t capacity() { return N; }
    /// Current element count.
    ::size_t size() const { return size_; }
    bool empty() const { return size_ == 0; }
    bool full() const { return size_ == N; }
    /// True if a construction / `push_back` dropped elements past `N`.
    bool overflowed() const { return overflow_; }

    /// Append an element. Returns false (no-op) if already at capacity.
    bool push_back(const T& v) {
        if (size_ >= N) {
            return false;
        }
        data_[size_++] = v;
        return true;
    }

    void clear() {
        size_ = 0;
        overflow_ = false;
    }

    /// Unchecked element access (caller ensures `i < size()`).
    T& operator[](::size_t i) { return data_[i]; }
    const T& operator[](::size_t i) const { return data_[i]; }

    /// Pointer to the inline storage (valid for `size()` elements).
    T* data() { return data_; }
    const T* data() const { return data_; }

    const T* begin() const { return data_; }
    const T* end() const { return data_ + size_; }

  private:
    T data_[N];
    ::size_t size_;
    bool overflow_ = false;
};

} // namespace nros

#endif // NROS_CPP_PARAMETER_HPP
