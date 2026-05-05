// nros-cpp: Result type for error handling
// Freestanding C++ — no exceptions, no STL required

/**
 * @file result.hpp
 * @ingroup grp_errors
 * @brief `nros::Result`, `nros::ErrorCode`, and the `NROS_TRY` macro.
 *
 * See @ref error_codes for the full code table and recovery guidance.
 */

#ifndef NROS_CPP_RESULT_HPP
#define NROS_CPP_RESULT_HPP

#include <cstdint>

namespace nros {

/// Error codes returned by nros-cpp functions.
///
/// Values match the C `nros_cpp_ret_t` enum in `<nros/nros_cpp_generated.h>`.
enum class ErrorCode : int32_t {
    /// Success.
    Ok = 0,
    /// Generic failure not covered by a more specific code.
    Error = -1,
    /// Operation deadline elapsed before completion.
    Timeout = -2,
    /// Null pointer, empty topic name, or out-of-range value.
    InvalidArgument = -3,
    /// `nros::init()` was never called or the entity is in a default
    /// state. See `is_valid()` on entity classes.
    NotInitialized = -4,
    /// Static pool exhausted (executor slots, subscription buffers, …).
    Full = -5,
    /// Transient — no data ready yet (non-blocking take). Retry later.
    TryAgain = -6,
    /// A blocking call was made from inside a callback.
    Reentrant = -7,
    /// Underlying zenoh-pico / DDS transport rejected the operation.
    TransportError = -100,
};

/// Result type for fallible operations.
///
/// This replaces exceptions in freestanding C++. Use the NROS_TRY macro
/// for early return on error.
class Result {
  public:
    /// Default-construct a success.
    constexpr Result() : code_(ErrorCode::Ok) {}
    /// Construct from a typed code.
    constexpr Result(ErrorCode code) : code_(code) {}
    /// Construct from a raw FFI return value (`int32_t`).
    constexpr Result(int32_t raw) : code_(static_cast<ErrorCode>(raw)) {}

    /// Returns true if the operation succeeded.
    bool ok() const { return code_ == ErrorCode::Ok; }

    /// Explicit bool conversion — allows `if (result) { ... }`.
    explicit operator bool() const { return ok(); }

    /// Get the underlying error code.
    ErrorCode code() const { return code_; }

    /// Get the raw integer code (for FFI interop).
    int32_t raw() const { return static_cast<int32_t>(code_); }

    /// Named constructors.
    static constexpr Result success() { return Result(ErrorCode::Ok); }

  private:
    ErrorCode code_;
};

/// Early-return macro for error propagation (replaces try/catch).
///
/// Usage:
/// ```cpp
/// nros::Result do_stuff() {
///     NROS_TRY(nros::init());
///     NROS_TRY(node.create_publisher(pub, "/topic"));
///     return nros::Result::success();
/// }
/// ```
#define NROS_TRY(expr)                                                                             \
    do {                                                                                           \
        ::nros::Result _nros_r = (expr);                                                           \
        if (!_nros_r.ok()) return _nros_r;                                                         \
    } while (0)

/// Like NROS_TRY but for callers that need a custom return value
/// (e.g. `int main` examples returning 1 on failure).
///
/// By default the failure is silent — nros-cpp is freestanding and
/// must not pull in `<cstdio>` from a public header. Override
/// `NROS_TRY_LOG(file, line, expr, ret)` before including this header
/// to attach a logger (`std::fprintf`, Zephyr's `LOG_ERR`, semihosting,
/// etc.).
#ifndef NROS_TRY_LOG
#define NROS_TRY_LOG(file, line, expr, ret) ((void)(file), (void)(line), (void)(expr), (void)(ret))
#endif

#define NROS_TRY_RET(expr, retval)                                                                 \
    do {                                                                                           \
        ::nros::Result _nros_r = (expr);                                                           \
        if (!_nros_r.ok()) {                                                                       \
            NROS_TRY_LOG(__FILE__, __LINE__, #expr, _nros_r.raw());                                \
            return (retval);                                                                       \
        }                                                                                          \
    } while (0)

} // namespace nros

#endif // NROS_CPP_RESULT_HPP
