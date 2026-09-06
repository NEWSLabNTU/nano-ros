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
#include <utility>
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio>
#endif

/// `NROS_NODISCARD` — `[[nodiscard]]` where the compiler has it, and nothing
/// where it does not.
///
/// WHERE IT LIVES, and why here. The only things this tree marks are the two
/// result types below, and `result.hpp` is the lowest C++ header we own — it
/// includes `<cstdint>` and `<utility>` and nothing of ours, and every other
/// `nros/*.hpp` reaches it. A header of its own would be one more file and one
/// more include for one macro.
///
/// It is NOT a capability gate. It asks the COMPILER what attribute spelling it
/// has, not the target what library it ships, and it never gates a MEMBER — so
/// `sizeof(Result)` and `sizeof(ResultOf<T>)` are the same number in every
/// configuration. That is the rule `scripts/check-cpp-capability-layout.sh`
/// measures, both types are named in its list, and it is why phase-427 W8
/// forbids this header a third capability gate. The two it has stay two:
/// `<cstdio>` for the default `NROS_TRY_LOG`, and the body of that macro.
///
/// The spelling is measured, not assumed (gcc 12.3 and clang 14, `-std=c++14`,
/// which is the standard the `cpp` lane compiles these headers with):
///
///   * `[[nodiscard]]` on a CLASS works in C++14 mode on both compilers and
///     fires `-Wunused-result` at a discarding call site. GCC says nothing
///     about it being a C++17 attribute; clang reports
///     `-Wc++17-attribute-extensions`, but only under `-pedantic`.
///   * `__attribute__((warn_unused_result))` on a CLASS is a clang-only
///     spelling. GCC 12 rejects it — "warn_unused_result attribute only
///     applies to function types" `[-Wattributes]` — and the class then
///     carries no attribute at all, which is the silent-nothing outcome this
///     macro exists to avoid.
///
/// So: the GNU spelling on clang below C++17, where it costs no `-pedantic`
/// diagnostic and clang honours it on types; `[[nodiscard]]` everywhere else
/// the compiler advertises it; empty otherwise.
///
/// The outer `#ifndef` is there so a toolchain that already defines the name is
/// not redefined out from under itself. It is not an invitation: defining it
/// away is a decision to let failed operations be dropped in silence, and the
/// `cpp` lane compiles a TU that proves the attribute reaches callers.
#ifndef NROS_NODISCARD
#if defined(__has_cpp_attribute)
#if __has_cpp_attribute(nodiscard) >= 201603L
#if defined(__clang__) && __cplusplus < 201703L
#define NROS_NODISCARD __attribute__((warn_unused_result))
#else
#define NROS_NODISCARD [[nodiscard]]
#endif
#endif
#endif
#endif
#ifndef NROS_NODISCARD
#define NROS_NODISCARD
#endif

namespace nros {

/// Error codes returned by nros-cpp functions.
///
/// Values match the C `nros_cpp_ret_t` enum in `<nros/nros_cpp_generated.h>`.
/// Issue #229 — value-identical to the C `NROS_RET_*` codes AND the
/// `NROS_CPP_RET_*` FFI codes (one numbering across all three spaces), so
/// `Result(<any C-ABI return>)` is correct by identity. The static_assert
/// pin tables below and in parameter.hpp fail the build on re-divergence.
enum class ErrorCode : int32_t {
    /// Success.
    Ok = 0,
    /// Generic failure not covered by a more specific code.
    Error = -1,
    /// Operation deadline elapsed before completion.
    Timeout = -2,
    /// Null pointer, empty topic name, or out-of-range value.
    InvalidArgument = -3,
    /// Entity not found (topic, parameter, service…).
    NotFound = -4,
    /// Already exists (duplicate declare/register).
    AlreadyExists = -5,
    /// Static pool exhausted (executor slots, subscription buffers, …).
    Full = -6,
    /// `nros::init()` was never called or the entity is in a default
    /// state. See `is_valid()` on entity classes.
    NotInitialized = -7,
    /// Operation invalid in the current state (bad call sequence).
    BadSequence = -8,
    /// Service request/reply failed.
    ServiceFailed = -9,
    /// Publish failed.
    PublishFailed = -10,
    /// Subscription create/take failed.
    SubscriptionFailed = -11,
    /// Operation not allowed for this entity/backend.
    NotAllowed = -12,
    /// Request was rejected — the peer considered it and declined.
    /// A goal rejected by an action server, or a QoS/ABI
    /// incompatibility. Distinct from `Error`, which means the
    /// request never got that far (issue 0868).
    Rejected = -13,
    /// Transient — no data ready yet (non-blocking take). Retry later.
    TryAgain = -14,
    /// A blocking call was made from inside a callback.
    Reentrant = -15,
    /// Operation not implemented by the active backend.
    Unsupported = -16,
    /// Underlying zenoh-pico / DDS transport rejected the operation.
    TransportError = -100,
};

// Issue #229 pin (self-consistency half): the values above ARE the shared
// numbering. The cross-space asserts against the real C constants live in
// parameter.hpp (vs NROS_RET_*) and node.hpp (vs NROS_CPP_RET_*), where
// those headers are visible.
static_assert(static_cast<int32_t>(ErrorCode::NotFound) == -4 &&
                  static_cast<int32_t>(ErrorCode::AlreadyExists) == -5 &&
                  static_cast<int32_t>(ErrorCode::Full) == -6 &&
                  static_cast<int32_t>(ErrorCode::NotInitialized) == -7 &&
                  static_cast<int32_t>(ErrorCode::TryAgain) == -14 &&
                  static_cast<int32_t>(ErrorCode::Reentrant) == -15 &&
                  static_cast<int32_t>(ErrorCode::Unsupported) == -16,
              "ErrorCode numbering must match nros_ret_t (issue #229)");

/// The result of a fallible operation: `Result` when it produces nothing,
/// `ResultOf<T>` when it produces a value. This is the whole error channel —
/// RFC-0018 forbids exceptions, so nothing here throws, and RFC-0089
/// §"The error channel, settled" is the rule for which one an API picks.
/// Use the NROS_TRY macro for early return on error.
///
/// ONE TEMPLATE, TWO SPELLINGS — and the second spelling is forced by the
/// language rather than chosen. RFC-0089 asks for one template named `Result`,
/// the value-less case as `Result<void>`, and `Result` as its alias. C++
/// cannot express that: an identifier in a scope is a class, a class template,
/// or an alias, never two of those. Measured on gcc 12.3 and clang 14, every
/// route is ill-formed —
///
///     template <typename T = void> class Result;  Result f();
///         "invalid use of template-name 'Result' without an argument list"
///         under -std=c++14, and "deduced class type 'Result' in function
///         return type" under -std=c++17, where CTAD gets close enough to
///         change the message and no closer.
///     template <typename T> class Result;  class Result { };
///         "class template 'Result' redeclared as non-template"
///     template <typename T> class Result;  using Result = Result<void>;
///         "redeclared as different kind of entity"
///
/// So the STRUCTURE the RFC asks for is here in full — one template, the
/// value-less case IS its `void` specialization, `Result` is an alias for that
/// specialization, and there are no longer two unrelated types to learn. Only
/// the template's own spelling differs, because `Result` is what 1100+ call
/// sites and the entire public API already write for the void case, and a bare
/// name is the one worth keeping bare. phase-427 W8.
///
/// The primary template is DECLARED here and defined below `Result`, because
/// the value-less specialization is what `ResultOf<T>::error(const Result&)`
/// names. `NROS_NODISCARD` sits on the two DEFINITIONS and not on this
/// declaration: repeating it would be legal and redundant, and a reader looking
/// at a class body should be able to see whether its result may be dropped.
template <typename T> class ResultOf;

/// `Result` — a fallible operation that produces no value.
///
/// `NROS_NODISCARD`: a discarded result is a failure nobody was told about,
/// which is the one outcome this channel exists to prevent. That attribute is
/// a signal to a reader and to a `-Werror` consumer, not an enforced gate in
/// this tree (see `NROS_NODISCARD`'s own doc above for the measured
/// warning-vs-error spelling); where a failure must not be ignorable, the
/// refusal has to be at RUNTIME — which is what
/// `rclcpp::detail::require_created` does for the `create_*` verbs, and what
/// `rclcpp::init(argc, argv)` already does for `--ros-args`.
template <> class NROS_NODISCARD ResultOf<void> {
  public:
    /// Default-construct a success.
    constexpr ResultOf() : code_(ErrorCode::Ok) {}
    /// Construct from a typed code.
    constexpr ResultOf(ErrorCode code) : code_(code) {}
    /// Construct from a raw FFI return value (`int32_t`).
    constexpr ResultOf(int32_t raw) : code_(static_cast<ErrorCode>(raw)) {}

    /// Returns true if the operation succeeded.
    bool ok() const { return code_ == ErrorCode::Ok; }

    /// Explicit bool conversion — allows `if (result) { ... }`.
    explicit operator bool() const { return ok(); }

    /// Get the underlying error code.
    ErrorCode code() const { return code_; }

    /// Get the raw integer code (for FFI interop).
    int32_t raw() const { return static_cast<int32_t>(code_); }

    /// Named constructors.
    static constexpr ResultOf success() { return ResultOf(ErrorCode::Ok); }

  private:
    ErrorCode code_;
};

/// The name every caller writes for the value-less case, and the name this
/// header's macros expand to.
using Result = ResultOf<void>;

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
/// Phase 123.B.1 — when `NROS_CPP_STD` is defined (POSIX / Zephyr
/// native_sim / threadx-linux + any host with `<cstdio>`), the
/// default logger writes `[nros] <file>:<line> <expr> -> <ret>` to
/// `stderr` so first-time users see failures immediately. Embedded
/// builds without stdio fall through to the silent default.
///
/// Override `NROS_TRY_LOG(file, line, expr, ret)` before including
/// this header to attach a custom logger (Zephyr's `LOG_ERR`,
/// semihosting, defmt, etc.). Opt out entirely with
/// `#define NROS_TRY_LOG(file, line, expr, ret) ((void)0)`.
#ifndef NROS_TRY_LOG
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    ::std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))
#else
#define NROS_TRY_LOG(file, line, expr, ret) ((void)(file), (void)(line), (void)(expr), (void)(ret))
#endif
#endif

#define NROS_TRY_RET(expr, retval)                                                                 \
    do {                                                                                           \
        ::nros::Result _nros_r = (expr);                                                           \
        if (!_nros_r.ok()) {                                                                       \
            NROS_TRY_LOG(__FILE__, __LINE__, #expr, _nros_r.raw());                                \
            return (retval);                                                                       \
        }                                                                                          \
    } while (0)

/// Like NROS_TRY but for void-returning callers (RTOS `app_main(void)`,
/// task entry points, …). Logs the failure via the same `NROS_TRY_LOG`
/// hook as `NROS_TRY_RET` and bails with a bare `return;`.
#define NROS_CHECK(expr)                                                                           \
    do {                                                                                           \
        ::nros::Result _nros_r = (expr);                                                           \
        if (!_nros_r.ok()) {                                                                       \
            NROS_TRY_LOG(__FILE__, __LINE__, #expr, _nros_r.raw());                                \
            return;                                                                                \
        }                                                                                          \
    } while (0)

/// `ResultOf<T>` — a fallible operation that produces a value (phase 123.B.4;
/// this was `Expected<T>` until phase-427 W8 folded both halves of the error
/// channel into one template).
///
/// Lets factory functions return constructed entities by value
/// instead of forcing the out-param + Result idiom. Trade-off:
/// requires `T` to be default-constructible and move-constructible
/// (Node, Publisher, Subscription all satisfy both today). Storage
/// is direct (the value lives inline, no allocation) — when the
/// result holds an error the value member is default-constructed
/// and idle.
///
/// Usage:
/// ```cpp
/// auto node_r = nros::Node::make("my_node");
/// if (!node_r.ok()) return node_r.error_as_result();
/// auto& node = node_r.value();
/// ```
///
/// Out-param `create_node(node, "name")` remains the canonical
/// zero-cost API for embedded / strictly-no-alloc code; `make()`
/// is a hosted-friendly convenience that closes the rclcpp /
/// rclrs idiom gap.
template <typename T> class NROS_NODISCARD ResultOf {
  public:
    static ResultOf ok(T value) {
        ResultOf e;
        e.ok_ = true;
        e.value_ = ::std::move(value);
        return e;
    }
    static ResultOf error(ErrorCode code) {
        ResultOf e;
        e.ok_ = false;
        e.error_ = code;
        return e;
    }
    static ResultOf error(const Result& r) { return error(r.code()); }

    bool ok() const { return ok_; }
    explicit operator bool() const { return ok_; }

    T& value() & { return value_; }
    const T& value() const& { return value_; }
    T&& value() && { return ::std::move(value_); }

    ErrorCode error() const { return error_; }
    Result error_as_result() const { return Result(error_); }

  private:
    ResultOf() : ok_(false), error_(ErrorCode::Error), value_() {}

    bool ok_;
    ErrorCode error_;
    T value_;
};

/// `Expected<T>` — the old spelling of `ResultOf<T>`, kept for one release.
///
/// A DERIVED CLASS rather than the alias template the obvious reading asks
/// for, and the reason is measured: `[[deprecated]]` on an alias template
/// warns on gcc 12.3 and is SILENT on clang 14 (both -std=c++14 and c++17),
/// so half our users would be told nothing until the name vanished. The same
/// attribute on a CLASS template warns on both. A deprecation nobody is told
/// about is just an alias, so the shape follows the diagnostic.
///
/// It converts from `ResultOf<T>` in both directions a caller needs: the
/// inherited `ok()`/`error()` factories return the base, which converts here,
/// and an `Expected<T>` slices back to the base wherever one is expected.
template <typename T>
class NROS_NODISCARD [[deprecated("nros::Expected<T> is now nros::ResultOf<T>; one template "
                                  "carries the whole error channel (phase-427 W8, RFC-0089). "
                                  "The old spelling goes away after one release.")]] Expected
    : public ResultOf<T> {
  public:
    Expected(const ResultOf<T>& r) : ResultOf<T>(r) {}
    Expected(ResultOf<T>&& r) : ResultOf<T>(::std::move(r)) {}
};

} // namespace nros

// ============================================================================
// rclcpp:: — the ROS 2 spelling (RFC-0089 stage 6, step A)
// ============================================================================
//
// Moved here from `nros/rclcpp_compat.hpp`, which no longer carries a surface
// of its own: RFC-0089 §"Naming: replace, with alias as the migration step"
// makes the ROS 2 spelling a first-class name declared by the API header that
// owns the concept, at which point a shim has nothing left to bridge.

// `rclcpp::Result` is NOT an upstream rclcpp name — it is a convenience the
// compat header carried, kept here because step A must not lose anything. It
// names `nros::Result` exactly; RFC-0018 forbids exceptions, so there is no
// upstream error type to adopt in its place.
namespace rclcpp {
using ::nros::Result;
} // namespace rclcpp

#endif // NROS_CPP_RESULT_HPP
