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
 * declare time. Both are compile-time bounds — no heap, no STL
 * `std::vector` in storage. The value type may be *built* from a
 * `std::vector<T>` under `NROS_CPP_STD`, but the storage is always the
 * fixed pool. Over-`N` and over-pool conditions are rejected with an
 * error code, never UB.
 *
 * **Storage split (issue #226).** The C array-parameter FFI
 * (`nros_param_*_array`) stores a *borrowed* pointer + length — the
 * caller must own stable bytes. This wrapper's inline pool IS that stable
 * owner: element bytes live in the pool (each allocation prefixed by its
 * element capacity), while the RECORDS (name → type → pointer/len) live
 * in the C server like every scalar. No parallel name table, no duplicate
 * lookup — and sequence parameters are visible through `raw()` to the
 * param services like everything else. (The pre-#226 header kept its own
 * record table and hid sequences from the C server entirely.)
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

// Issue #226 — the array-parameter FFI is macro-generated in nros-c
// (paste! expansion), which cbindgen cannot expand into nros_generated.h.
// Declare the six entry points used below locally (client.hpp precedent).
extern "C" {
nros_ret_t nros_param_declare_double_array(nros_param_server_t* server, const char* name,
                                           const double* data, size_t len);
nros_ret_t nros_param_declare_integer_array(nros_param_server_t* server, const char* name,
                                            const int64_t* data, size_t len);
nros_ret_t nros_param_declare_bool_array(nros_param_server_t* server, const char* name,
                                         const bool* data, size_t len);
nros_ret_t nros_param_get_double_array(const nros_param_server_t* server, const char* name,
                                       const double** data, size_t* len);
nros_ret_t nros_param_get_integer_array(const nros_param_server_t* server, const char* name,
                                        const int64_t** data, size_t* len);
nros_ret_t nros_param_get_bool_array(const nros_param_server_t* server, const char* name,
                                     const bool** data, size_t* len);
nros_ret_t nros_param_set_double_array(nros_param_server_t* server, const char* name,
                                       const double* data, size_t len);
nros_ret_t nros_param_set_integer_array(nros_param_server_t* server, const char* name,
                                        const int64_t* data, size_t len);
nros_ret_t nros_param_set_bool_array(nros_param_server_t* server, const char* name,
                                     const bool* data, size_t len);
} // extern "C"

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

namespace detail {} // namespace detail

/// Fixed-capacity, node-local typed parameter server.
///
/// Capacity is compile-time; storage lives inline. No heap allocation.
/// Bool / int64 / double / string scalar types supported, plus
/// fixed-capacity `Seq<T, N>` sequences (`T` = double / int64 / bool).
///
/// @tparam Capacity     Max scalar/string parameters (C-side storage).
/// @tparam SeqSlots     Retained for source compatibility (issue #226):
///                      sequence records now live in the C server, so the
///                      declare count is bounded by `Capacity` like every
///                      scalar; this parameter no longer bounds anything.
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
template <::size_t Capacity, ::size_t SeqSlots /* unused, see above */ = 4,
          ::size_t SeqPoolBytes = 256>
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
        const T* src = nullptr;
        ::size_t len = 0;
        nros_ret_t rc = seq_get_ffi(&server_, name, &src, &len);
        if (rc != NROS_RET_OK) {
            return Result(rc);
        }
        if (len > N) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        out.clear();
        for (::size_t i = 0; i < len; ++i) {
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
        return Result(seq_get_ffi(&server_, name, &data, &len));
    }

#ifdef NROS_CPP_STD
    /// Get a sequence parameter into a `std::vector<T>` (hosted).
    template <typename T> Result get_parameter(const char* name, std::vector<T>& out) const {
        const T* src = nullptr;
        ::size_t len = 0;
        nros_ret_t rc = seq_get_ffi(&server_, name, &src, &len);
        if (rc != NROS_RET_OK) {
            return Result(rc);
        }
        out.assign(src, src + len);
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

    /// Check whether a parameter has been declared (scalar or sequence —
    /// both live in the C server since issue #226).
    bool has_parameter(const char* name) const { return nros_param_has(&server_, name); }

    /// Number of declared parameters (scalars + sequences).
    ::size_t parameter_count() const { return nros_param_server_get_count(&server_); }

    /// Get the underlying C server pointer.
    ///
    /// Useful for handing the server to C-API helpers (e.g. ROS 2
    /// service registration when the `param-services` feature is on).
    /// Since issue #226 sequence parameters are recorded here too.
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
    /* int reads go through the int64_t slot, then narrow — symmetric with the
       int declare_impl/set_impl above (rclcpp nodes declare_parameter<int>). */
    Result get_impl(const char* name, int& out) const {
        int64_t v = 0;
        Result r(nros_param_get_integer(&server_, name, &v));
        if (r.ok()) {
            out = static_cast<int>(v);
        }
        return r;
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
    Result set_impl(const char* name, const ::std::string& v) { return set_impl(name, v.c_str()); }
    Result get_impl(const char* name, ::std::string& out) const {
        char buf[128];
        Result r(nros_param_get_string(&server_, name, buf, sizeof(buf)));
        if (r.ok()) {
            out.assign(buf);
        }
        return r;
    }
#endif // NROS_CPP_STD

    /* -------- sequence parameters (issue #226): bytes in the pool below,
       records in the C server. Each pool allocation is prefixed by a
       uint64_t element-capacity header so `set_parameter` can bounds-check
       without a parallel record table; both the header and the element
       block are 8-aligned (every supported element type has alignment
       <= 8). Pool bytes are permanent for the server's lifetime, exactly
       what the borrow-semantics C array FFI requires of its caller. A
       declare that fails at the FFI (duplicate name racing in, server
       full) does not advance the pool cursor. -------- */

    static ::size_t align_up(::size_t off, ::size_t a) { return (off + (a - 1)) & ~(a - 1); }

    /* C-FFI dispatch by element type (double / int64 / bool). */
    static nros_ret_t seq_declare_ffi(nros_param_server_t* s, const char* n, const double* d,
                                      ::size_t l) {
        return nros_param_declare_double_array(s, n, d, l);
    }
    static nros_ret_t seq_declare_ffi(nros_param_server_t* s, const char* n, const int64_t* d,
                                      ::size_t l) {
        return nros_param_declare_integer_array(s, n, d, l);
    }
    static nros_ret_t seq_declare_ffi(nros_param_server_t* s, const char* n, const bool* d,
                                      ::size_t l) {
        return nros_param_declare_bool_array(s, n, d, l);
    }
    static nros_ret_t seq_get_ffi(const nros_param_server_t* s, const char* n, const double** d,
                                  ::size_t* l) {
        return nros_param_get_double_array(s, n, d, l);
    }
    static nros_ret_t seq_get_ffi(const nros_param_server_t* s, const char* n, const int64_t** d,
                                  ::size_t* l) {
        return nros_param_get_integer_array(s, n, d, l);
    }
    static nros_ret_t seq_get_ffi(const nros_param_server_t* s, const char* n, const bool** d,
                                  ::size_t* l) {
        return nros_param_get_bool_array(s, n, d, l);
    }
    static nros_ret_t seq_set_ffi(nros_param_server_t* s, const char* n, const double* d,
                                  ::size_t l) {
        return nros_param_set_double_array(s, n, d, l);
    }
    static nros_ret_t seq_set_ffi(nros_param_server_t* s, const char* n, const int64_t* d,
                                  ::size_t l) {
        return nros_param_set_integer_array(s, n, d, l);
    }
    static nros_ret_t seq_set_ffi(nros_param_server_t* s, const char* n, const bool* d,
                                  ::size_t l) {
        return nros_param_set_bool_array(s, n, d, l);
    }

    template <typename T>
    Result declare_seq_impl(const char* name, const T* src, ::size_t len, ::size_t cap_n) {
        static_assert(alignof(T) <= 8, "sequence element alignment exceeds pool alignment");
        if (name == nullptr || len > cap_n) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        /* Layout: [uint64_t cap][T x cap_n], both 8-aligned. */
        const ::size_t hdr = align_up(seq_pool_used_, 8);
        const ::size_t base = hdr + sizeof(uint64_t);
        const ::size_t end = base + cap_n * sizeof(T);
        if (end > SeqPoolBytes) {
            return Result(NROS_RET_FULL);
        }
        *reinterpret_cast<uint64_t*>(&seq_pool_[hdr]) = static_cast<uint64_t>(cap_n);
        T* dst = reinterpret_cast<T*>(&seq_pool_[base]);
        for (::size_t i = 0; i < len; ++i) {
            dst[i] = src[i];
        }
        /* The C server owns the record (duplicate-name + capacity checks
           happen there); only commit the pool cursor on success. */
        nros_ret_t rc = seq_declare_ffi(&server_, name, dst, len);
        if (rc != NROS_RET_OK) {
            return Result(rc);
        }
        seq_pool_used_ = end;
        return Result(NROS_RET_OK);
    }

    template <typename T> Result set_seq_impl(const char* name, const T* src, ::size_t len) {
        const T* cur = nullptr;
        ::size_t cur_len = 0;
        /* Existence + element-type validation happen in the C server. */
        nros_ret_t rc = seq_get_ffi(&server_, name, &cur, &cur_len);
        if (rc != NROS_RET_OK) {
            return Result(rc);
        }
        const ::size_t cap = static_cast<::size_t>(reinterpret_cast<const uint64_t*>(cur)[-1]);
        if (len > cap) {
            return Result(NROS_RET_INVALID_ARGUMENT);
        }
        T* dst = const_cast<T*>(cur); /* pool bytes are ours */
        for (::size_t i = 0; i < len; ++i) {
            dst[i] = src[i];
        }
        return Result(seq_set_ffi(&server_, name, dst, len));
    }

    nros_param_server_t server_;
    nros_parameter_t storage_[Capacity];

    alignas(8) unsigned char seq_pool_[SeqPoolBytes];
    ::size_t seq_pool_used_ = 0;
};

} // namespace nros

#endif // NROS_CPP_PARAMETER_HPP
