// nros-cpp: Parameter server wrapper
// Freestanding C++ — no exceptions, no STL required, no heap

/**
 * @file parameter.hpp
 * @ingroup grp_parameter
 * @brief `nros::ParameterServer<Cap>` — node-local typed parameter store.
 *
 * Wraps the C `nros_param_server_t` (see `nros/parameter.h`) with a
 * fixed-capacity storage array and rclcpp-shape `declare_parameter<T>` /
 * `get_parameter<T>` / `set_parameter<T>` template methods.
 *
 * The server is purely local — no ROS 2 service exposure. For
 * service-backed parameters, use the `param-services` Cargo feature on
 * `nros-c` and the `nros_executor_*_param_*` C functions directly.
 *
 * ## Sequence parameters (Phase 242.3 / RFC-0044 Q3)
 *
 * `declare_parameter` / `get_parameter` / `set_parameter` also accept a
 * fixed-capacity sequence value `nros::Seq<T, N>` (`T` = `double`,
 * `int64_t`, or `bool`). This unblocks rclcpp-faithful nodes that declare
 * `std::vector<double>` weight matrices (ASI's MPC) without a heap.
 *
 * **Capacity model (RFC-0044 Q3 — compile-time `N`, no shared dynamic
 * arena).** The per-parameter capacity is the compile-time `N` of the
 * `Seq<T, N>` the caller declares. The server owns the element bytes in
 * an inline, statically-sized pool (`SeqPoolBytes`) bump-allocated at
 * declare time, plus a fixed sequence-record table (`SeqSlots`). Both are
 * compile-time bounds — no heap, no STL `std::vector` in storage. The
 * value type may be *built* from a `std::vector<T>` under `NROS_CPP_STD`,
 * but the storage is always the fixed pool. Over-`N`, over-pool, and
 * over-slot conditions are rejected with an error code, never UB.
 *
 * Sequences are C++-storage-local: they do **not** cross the C FFI. The
 * C array-parameter FFI (`nros_param_*_double_array`) stores a *borrowed*
 * caller pointer + length, which would dangle under the server-owns-the-
 * value semantics rclcpp expects — so sequence storage lives entirely in
 * this wrapper.
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

#include "nros/result.hpp"

#ifdef NROS_CPP_STD
#include <string>
#include <vector>
#endif

extern "C" {
#include "nros/parameter.h"
}

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

namespace detail {

/// Element-type discriminator for stored sequence parameters.
enum class SeqKind : uint8_t { Double, Integer, Bool };

template <typename T> struct seq_kind_of;
template <> struct seq_kind_of<double> {
    static constexpr SeqKind value = SeqKind::Double;
};
template <> struct seq_kind_of<int64_t> {
    static constexpr SeqKind value = SeqKind::Integer;
};
template <> struct seq_kind_of<bool> {
    static constexpr SeqKind value = SeqKind::Bool;
};

} // namespace detail

/// Fixed-capacity, node-local typed parameter server.
///
/// Capacity is compile-time; storage lives inline. No heap allocation.
/// Bool / int64 / double / string scalar types supported, plus
/// fixed-capacity `Seq<T, N>` sequences (`T` = double / int64 / bool).
///
/// @tparam Capacity     Max scalar/string parameters (C-side storage).
/// @tparam SeqSlots     Max sequence parameters (default 4).
/// @tparam SeqPoolBytes Inline byte pool backing all sequence element
///                      storage (default 256 B ≈ 32 doubles).
///
/// Strings are copied into a 128-byte slot inside the server; callers do
/// not need to keep the source buffer alive past the call. Sequence
/// elements are copied into the inline pool — the server owns them; the
/// caller's `Seq` / `std::vector` need not outlive the call.
///
/// Usage:
/// ```cpp
/// nros::ParameterServer<16> params;
/// NROS_TRY(params.declare_parameter<double>("ctrl_period", 0.15));
/// double v = 0.0;
/// NROS_TRY(params.get_parameter<double>("ctrl_period", v));
///
/// // Sequence (MPC weight matrix):
/// NROS_TRY(params.declare_parameter("mpc_weights",
///                                   nros::Seq<double, 8>{1.0, 2.0, 3.0}));
/// nros::Seq<double, 8> w;
/// NROS_TRY(params.get_parameter("mpc_weights", w));
/// ```
template <::size_t Capacity, ::size_t SeqSlots = 4, ::size_t SeqPoolBytes = 256>
class ParameterServer {
  public:
    ParameterServer() : server_(nros_param_server_get_zero_initialized()) {
        nros_param_server_init(&server_, storage_, Capacity);
    }

    ~ParameterServer() { nros_param_server_fini(&server_); }

    ParameterServer(const ParameterServer&) = delete;
    ParameterServer& operator=(const ParameterServer&) = delete;
    ParameterServer(ParameterServer&&) = delete;
    ParameterServer& operator=(ParameterServer&&) = delete;

    /// Declare a parameter with a default value.
    ///
    /// @tparam T  bool, int64_t, double, or const char*.
    /// @param name           Parameter name (null-terminated).
    /// @param default_value  Default value used until overridden.
    template <typename T> Result declare_parameter(const char* name, T default_value) {
        return declare_impl(name, default_value);
    }

    /// Declare a fixed-capacity sequence parameter (Phase 242.3).
    ///
    /// The element bytes are copied into the server's inline pool — the
    /// server owns them; `default_value` need not outlive the call.
    ///
    /// @tparam T  double, int64_t, or bool.
    /// @tparam N  Per-parameter compile-time capacity.
    /// @retval ErrorCode::Ok            on success.
    /// @retval NROS_RET_ALREADY_EXISTS  name already declared.
    /// @retval NROS_RET_FULL            sequence-slot or element-pool full.
    /// @retval NROS_RET_INVALID_ARGUMENT  null name.
    template <typename T, ::size_t N>
    Result declare_parameter(const char* name, const Seq<T, N>& default_value) {
        return declare_seq_impl<T>(name, default_value.data(), default_value.size(), N);
    }

#ifdef NROS_CPP_STD
    /// Declare a sequence parameter from a `std::vector<T>` (hosted).
    /// `N` is the fixed storage capacity and must be given explicitly:
    /// `declare_parameter<double, 8>("w", vec)`. Elements past `N` are
    /// rejected (`NROS_RET_INVALID_ARGUMENT`).
    template <typename T, ::size_t N>
    Result declare_parameter(const char* name, const std::vector<T>& default_value) {
        if (default_value.size() > N) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        return declare_seq_impl<T>(name, default_value.data(), default_value.size(), N);
    }
#endif

    /// Get a parameter value.
    ///
    /// @tparam T  bool, int64_t, or double.
    /// @param name  Parameter name.
    /// @param out   Receives the value on success.
    /// @retval ErrorCode::Ok       on success.
    /// @retval Other code (raw)    NROS_RET_NOT_FOUND if undeclared.
    template <typename T> Result get_parameter(const char* name, T& out) const {
        return get_impl(name, out);
    }

    /// Get a string parameter into a caller-provided buffer.
    ///
    /// @param name      Parameter name.
    /// @param out       Output buffer (null-terminated on success).
    /// @param max_len   Buffer capacity in bytes.
    Result get_parameter(const char* name, char* out, ::size_t max_len) const {
        return Result(nros_param_get_string(&server_, name, out, max_len));
    }

    /// Get a sequence parameter into a caller `Seq<T, N>` (Phase 242.3).
    ///
    /// Bounds-checked: if the stored element count exceeds the caller's
    /// `N`, the value is **not** truncated — `NROS_RET_INVALID_ARGUMENT`
    /// is returned (over-capacity rejected, never UB).
    ///
    /// @retval ErrorCode::Ok            on success.
    /// @retval NROS_RET_NOT_FOUND       no such sequence parameter.
    /// @retval NROS_RET_INVALID_ARGUMENT  element-type mismatch or `out`
    ///                                  too small.
    template <typename T, ::size_t N> Result get_parameter(const char* name, Seq<T, N>& out) const {
        int idx = find_seq(name);
        if (idx < 0) {
            return Result(NROS_RET_NOT_FOUND);
        }
        const SeqRecord& r = seq_records_[static_cast<::size_t>(idx)];
        if (r.kind != detail::seq_kind_of<T>::value) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        if (r.size_elems > N) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        const T* src = reinterpret_cast<const T*>(&seq_pool_[r.offset]);
        out.clear();
        for (::size_t i = 0; i < r.size_elems; ++i) {
            out.push_back(src[i]);
        }
        return Result(NROS_RET_OK);
    }

    /// Borrow a sequence parameter's storage in place (zero-copy).
    ///
    /// `data` points into the server's inline pool and is valid until the
    /// parameter is overwritten or the server is destroyed.
    template <typename T>
    Result get_parameter(const char* name, const T*& data, ::size_t& len) const {
        int idx = find_seq(name);
        if (idx < 0) {
            return Result(NROS_RET_NOT_FOUND);
        }
        const SeqRecord& r = seq_records_[static_cast<::size_t>(idx)];
        if (r.kind != detail::seq_kind_of<T>::value) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        data = reinterpret_cast<const T*>(&seq_pool_[r.offset]);
        len = r.size_elems;
        return Result(NROS_RET_OK);
    }

#ifdef NROS_CPP_STD
    /// Get a sequence parameter into a `std::vector<T>` (hosted).
    template <typename T> Result get_parameter(const char* name, std::vector<T>& out) const {
        int idx = find_seq(name);
        if (idx < 0) {
            return Result(NROS_RET_NOT_FOUND);
        }
        const SeqRecord& r = seq_records_[static_cast<::size_t>(idx)];
        if (r.kind != detail::seq_kind_of<T>::value) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        const T* src = reinterpret_cast<const T*>(&seq_pool_[r.offset]);
        out.assign(src, src + r.size_elems);
        return Result(NROS_RET_OK);
    }
#endif

    /// Set an existing parameter's value.
    template <typename T> Result set_parameter(const char* name, T value) {
        return set_impl(name, value);
    }

    /// Set an existing sequence parameter (Phase 242.3).
    ///
    /// Bounds-checked: if `value` has more than the parameter's declared
    /// capacity, `NROS_RET_INVALID_ARGUMENT` is returned (never UB).
    template <typename T, ::size_t N>
    Result set_parameter(const char* name, const Seq<T, N>& value) {
        return set_seq_impl<T>(name, value.data(), value.size());
    }

#ifdef NROS_CPP_STD
    /// Set an existing sequence parameter from a `std::vector<T>` (hosted).
    template <typename T> Result set_parameter(const char* name, const std::vector<T>& value) {
        return set_seq_impl<T>(name, value.data(), value.size());
    }
#endif

    /// Check whether a parameter has been declared (scalar or sequence).
    bool has_parameter(const char* name) const {
        return nros_param_has(&server_, name) || find_seq(name) >= 0;
    }

    /// Number of declared parameters (scalars + sequences).
    ::size_t parameter_count() const { return nros_param_server_get_count(&server_) + seq_count_; }

    /// Get the underlying C server pointer.
    ///
    /// Useful for handing the server to C-API helpers (e.g. ROS 2
    /// service registration when the `param-services` feature is on).
    /// Sequence parameters are C++-local and not reflected here.
    nros_param_server_t* raw() { return &server_; }
    const nros_param_server_t* raw() const { return &server_; }

  private:
    /* declare overloads dispatch by argument type */
    Result declare_impl(const char* name, bool v) {
        return Result(nros_param_declare_bool(&server_, name, v));
    }
    Result declare_impl(const char* name, int64_t v) {
        return Result(nros_param_declare_integer(&server_, name, v));
    }
    Result declare_impl(const char* name, double v) {
        return Result(nros_param_declare_double(&server_, name, v));
    }
    Result declare_impl(const char* name, const char* v) {
        return Result(nros_param_declare_string(&server_, name, v));
    }
    /* int / uint literals collapse to int64_t */
    Result declare_impl(const char* name, int v) {
        return Result(nros_param_declare_integer(&server_, name, static_cast<int64_t>(v)));
    }

    Result get_impl(const char* name, bool& out) const {
        return Result(nros_param_get_bool(&server_, name, &out));
    }
    Result get_impl(const char* name, int64_t& out) const {
        return Result(nros_param_get_integer(&server_, name, &out));
    }
    Result get_impl(const char* name, double& out) const {
        return Result(nros_param_get_double(&server_, name, &out));
    }

    Result set_impl(const char* name, bool v) {
        return Result(nros_param_set_bool(&server_, name, v));
    }
    Result set_impl(const char* name, int64_t v) {
        return Result(nros_param_set_integer(&server_, name, v));
    }
    Result set_impl(const char* name, double v) {
        return Result(nros_param_set_double(&server_, name, v));
    }
    Result set_impl(const char* name, const char* v) {
        return Result(nros_param_set_string(&server_, name, v));
    }
    Result set_impl(const char* name, int v) {
        return Result(nros_param_set_integer(&server_, name, static_cast<int64_t>(v)));
    }

#ifdef NROS_CPP_STD
    /* 242.7 (fifth wall) — scalar std::string-VALUE params. 242.7 added
       std::string-keyed names + std::vector values; rclcpp also declares
       std::string *values* (ASI's MPC: declare_parameter<std::string>(name,
       "mpc") for the controller-mode / solver-type / slope-source knobs). Copy
       through the existing const char* string slot (128 bytes). */
    Result declare_impl(const char* name, const ::std::string& v) {
        return declare_impl(name, v.c_str());
    }
    Result set_impl(const char* name, const ::std::string& v) {
        return set_impl(name, v.c_str());
    }
    Result get_impl(const char* name, ::std::string& out) const {
        char buf[128];
        Result r(nros_param_get_string(&server_, name, buf, sizeof(buf)));
        if (r.ok()) {
            out.assign(buf);
        }
        return r;
    }
#endif // NROS_CPP_STD

    /* -------- sequence parameter storage (C++-local) -------- */

    static constexpr ::size_t kNameCap = 64; // matches NROS_MAX_PARAM_NAME_LEN

    struct SeqRecord {
        char name[kNameCap];
        detail::SeqKind kind;
        ::size_t offset;     // byte offset into seq_pool_
        ::size_t cap_elems;  // declared compile-time N
        ::size_t size_elems; // current element count
    };

    static bool name_eq(const char* a, const char* b) {
        ::size_t i = 0;
        for (; a[i] != '\0' && b[i] != '\0'; ++i) {
            if (a[i] != b[i]) {
                return false;
            }
        }
        return a[i] == b[i];
    }

    static void name_copy(char* dst, const char* src) {
        ::size_t i = 0;
        for (; i + 1 < kNameCap && src[i] != '\0'; ++i) {
            dst[i] = src[i];
        }
        dst[i] = '\0';
    }

    int find_seq(const char* name) const {
        if (name == nullptr) {
            return -1;
        }
        for (::size_t i = 0; i < seq_count_; ++i) {
            if (name_eq(seq_records_[i].name, name)) {
                return static_cast<int>(i);
            }
        }
        return -1;
    }

    static ::size_t align_up(::size_t off, ::size_t a) { return (off + (a - 1)) & ~(a - 1); }

    template <typename T>
    Result declare_seq_impl(const char* name, const T* src, ::size_t len, ::size_t cap_n) {
        if (name == nullptr) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        if (find_seq(name) >= 0 || nros_param_has(&server_, name)) {
            return Result(NROS_RET_ALREADY_EXISTS);
        }
        if (seq_count_ >= SeqSlots) {
            return Result(NROS_RET_FULL);
        }
        const ::size_t base = align_up(seq_pool_used_, alignof(T));
        const ::size_t end = base + cap_n * sizeof(T);
        if (end > SeqPoolBytes) {
            return Result(NROS_RET_FULL);
        }
        SeqRecord& r = seq_records_[seq_count_];
        name_copy(r.name, name);
        r.kind = detail::seq_kind_of<T>::value;
        r.offset = base;
        r.cap_elems = cap_n;
        r.size_elems = len;
        T* dst = reinterpret_cast<T*>(&seq_pool_[base]);
        for (::size_t i = 0; i < len; ++i) {
            dst[i] = src[i];
        }
        seq_pool_used_ = end;
        ++seq_count_;
        return Result(NROS_RET_OK);
    }

    template <typename T> Result set_seq_impl(const char* name, const T* src, ::size_t len) {
        int idx = find_seq(name);
        if (idx < 0) {
            return Result(NROS_RET_NOT_FOUND);
        }
        SeqRecord& r = seq_records_[static_cast<::size_t>(idx)];
        if (r.kind != detail::seq_kind_of<T>::value) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        if (len > r.cap_elems) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        T* dst = reinterpret_cast<T*>(&seq_pool_[r.offset]);
        for (::size_t i = 0; i < len; ++i) {
            dst[i] = src[i];
        }
        r.size_elems = len;
        return Result(NROS_RET_OK);
    }

    nros_param_server_t server_;
    nros_parameter_t storage_[Capacity];

    SeqRecord seq_records_[SeqSlots];
    alignas(8) unsigned char seq_pool_[SeqPoolBytes];
    ::size_t seq_pool_used_ = 0;
    ::size_t seq_count_ = 0;
};

} // namespace nros

#endif // NROS_CPP_PARAMETER_HPP
