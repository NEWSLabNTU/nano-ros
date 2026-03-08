// nros-cpp: Result type for error handling
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_RESULT_HPP
#define NROS_CPP_RESULT_HPP

#include <cstdint>

namespace nros {

/// Error codes returned by nros-cpp functions.
///
/// Values match the Rust `nros_cpp_ret_t` constants in nros-cpp-ffi.
enum class ErrorCode : int32_t {
    Ok              = 0,
    Error           = -1,
    Timeout         = -2,
    InvalidArgument = -3,
    NotInitialized  = -4,
    Full            = -5,
    TransportError  = -100,
};

/// Result type for fallible operations.
///
/// This replaces exceptions in freestanding C++. Use the NROS_TRY macro
/// for early return on error.
class Result {
public:
    constexpr Result() : code_(ErrorCode::Ok) {}
    constexpr Result(ErrorCode code) : code_(code) {}
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
#define NROS_TRY(expr)                     \
    do {                                   \
        ::nros::Result _nros_r = (expr);   \
        if (!_nros_r.ok()) return _nros_r; \
    } while (0)

} // namespace nros

#endif // NROS_CPP_RESULT_HPP
