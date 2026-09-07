// nros-cpp: the ONE parameter facade every C++ node type wears
// Freestanding C++ — no exceptions, no STL required, no heap

/**
 * @file node_parameters.hpp
 * @ingroup grp_parameter
 * @brief `nros::detail::node_param_*` — typed forwarders from a C++ node onto
 *        the executor's parameter store, across the `nros_cpp_node_param_*`
 *        FFI.
 *
 * ## Why this header exists (phase-426 W4, RFC-0089)
 *
 * Three stores existed for one concept: the executor's `nros_params` table —
 * the one the six `rcl_interfaces/srv/*` servers read and therefore the one
 * `ros2 param get` sees — plus an inline `nros::ParameterServer` on
 * `rclcpp::Node` and another on `nros::ComponentNode`. A parameter declared
 * through either C++ facade was invisible to `ros2 param get`, and two nodes in
 * one image could not see each other's. That is not a missing feature; it is a
 * second implementation of one, which RFC-0019/0020 forbids and RFC-0089
 * §"Parameters" closes.
 *
 * Both C++ members are gone. Both facades call the functions below, which are
 * the same call in both languages: the storage, the typing, the read-only and
 * range rules and the node keying all live in Rust
 * (`nros_params::ParameterServer`), and this header is the call-shape adapter
 * RFC-0019 permits a wrapper to be.
 *
 * ## What a node's parameters are keyed by
 *
 * The `nros_cpp_node_t*` — its `node_id` IS the key (phase-426 W1). So two
 * nodes on one executor may declare the same name with different values, and
 * `ros2 param list` enumerates per node. Pass a node's `ffi_handle()`; a null
 * handle (an `nros::Node` that was never opened) is an error, never a silent
 * write to some other node's parameters.
 *
 * ## Freestanding
 *
 * `<cstdint>` / `<cstddef>` and the FFI header, nothing more. The
 * `std::string` and `std::vector<T>` overloads are behind `NROS_CPP_STD`, the
 * same opt-in `nros::ParameterServer` used for them, so a `-nostdinc++` build
 * gets the scalar set and no parse error. Nothing here declares a MEMBER of
 * anything, so no capability probe can move a layout
 * (`check-cpp-capability-layout`).
 *
 * ## Availability
 *
 * The FFI entry points are defined whatever `nros-cpp` was built with. Where
 * the bringup declares no `param_services` capability there is no store, and
 * every call answers `nros::ErrorCode::Unsupported` — a code a facade can
 * report, never a default that pretends to have been stored.
 */

#ifndef NROS_CPP_NODE_PARAMETERS_HPP
#define NROS_CPP_NODE_PARAMETERS_HPP

#include <cstddef>
#include <cstdint>
// Freestanding C++ often only puts `size_t` in the global namespace via
// `<stddef.h>`; include it so `::size_t` is always resolvable (parameter.hpp
// precedent).
#include <stddef.h>

#include "nros/result.hpp"
#include "nros_cpp_ffi.h"

#ifdef NROS_CPP_STD
#include <string>
#include <vector>
#endif

/// Stack buffer for reading a string parameter back across the FFI.
///
/// The store's own bound is `NROS_MAX_STRING_VALUE_LEN` (`nros-params`,
/// default 256) and C++ cannot see it, so this is the C++ side's matching
/// number. A stored value longer than this comes back TRUNCATED with
/// `ErrorCode::Full` rather than silently short. It sizes a LOCAL only — no
/// member anywhere depends on it, so overriding it cannot change a layout.
#ifndef NROS_NODE_PARAM_STRING_BUF
#define NROS_NODE_PARAM_STRING_BUF 256
#endif
// AFTER the `#endif`, deliberately, and not inside the block above. Both other
// placements are broken and neither is obvious (issues 0637, 1015):
//
//   * BEFORE the `#define`, the preprocessor reads an undefined macro as 0, so
//     `< 1` is true on every build that does not `-D` it — the guard fires
//     always and the header never compiles.
//   * INSIDE the `#ifndef`, a `-D NROS_NODE_PARAM_STRING_BUF=0` skips the whole
//     block and takes the guard with it — it is absent for exactly the input it
//     exists to catch.
//
// Here it sees whatever value actually reached the translation unit, from
// either source. `check-c-array-guard-probe` compiles this both ways.
#if NROS_NODE_PARAM_STRING_BUF < 1
#error "NROS_NODE_PARAM_STRING_BUF must be >= 1: it sizes a C array (issue 1015)"
#endif

namespace nros {
namespace detail {

// --- declare ----------------------------------------------------------------
//
// Overload sets, not a template chain with `if constexpr`: this header must
// parse at C++14 (`just check cpp` compiles ~15 probes at `-std=c++14`, and
// PX4 modules build `-std=gnu++14 -Werror`). Tag dispatch is what
// `component_node.hpp:582` said to reach for if the C++17 branch ever had to
// come back down, and deleting `adopt_launch_seed_` is what brought it down.
//
// The set mirrors `nros::ParameterServer`'s `declare_impl` / `get_impl` /
// `set_impl` exactly, INCLUDING the `int` overloads — a ported node writes
// `declare_parameter<int>("depth", 10)` and `int` is not `int64_t`.

inline Result node_param_declare(const nros_cpp_node_t* node, const char* name, bool v) {
    return Result(nros_cpp_node_declare_param_bool(node, name, v));
}
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name, int64_t v) {
    return Result(nros_cpp_node_declare_param_integer(node, name, v));
}
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name, int v) {
    return Result(nros_cpp_node_declare_param_integer(node, name, static_cast<int64_t>(v)));
}
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name, double v) {
    return Result(nros_cpp_node_declare_param_double(node, name, v));
}
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name, const char* v) {
    return Result(nros_cpp_node_declare_param_string(node, name, v));
}

// --- get --------------------------------------------------------------------

inline Result node_param_get(const nros_cpp_node_t* node, const char* name, bool& out) {
    return Result(nros_cpp_node_get_param_bool(node, name, &out));
}
inline Result node_param_get(const nros_cpp_node_t* node, const char* name, int64_t& out) {
    return Result(nros_cpp_node_get_param_integer(node, name, &out));
}
/// `int` reads go through the `int64_t` slot, then narrow — symmetric with the
/// `int` declare above, and the same shape `ParameterServer::get_impl` had.
inline Result node_param_get(const nros_cpp_node_t* node, const char* name, int& out) {
    int64_t v = 0;
    Result r(nros_cpp_node_get_param_integer(node, name, &v));
    if (r.ok()) {
        out = static_cast<int>(v);
    }
    return r;
}
inline Result node_param_get(const nros_cpp_node_t* node, const char* name, double& out) {
    return Result(nros_cpp_node_get_param_double(node, name, &out));
}
/// Read a string parameter into a caller buffer, null-terminated.
inline Result node_param_get(const nros_cpp_node_t* node, const char* name, char* out,
                             ::size_t max_len) {
    return Result(nros_cpp_node_get_param_string(node, name, out, max_len));
}

// --- set --------------------------------------------------------------------
//
// Every one of these routes through `ParameterServer::apply` on the Rust side,
// i.e. through the same read-only / type / range / undeclared rules a remote
// `ros2 param set` gets. A facade that wrote a slot directly would be a second
// answer to "may this set happen", which is the class this wave removes.

inline Result node_param_set(const nros_cpp_node_t* node, const char* name, bool v) {
    return Result(nros_cpp_node_set_param_bool(node, name, v));
}
inline Result node_param_set(const nros_cpp_node_t* node, const char* name, int64_t v) {
    return Result(nros_cpp_node_set_param_integer(node, name, v));
}
inline Result node_param_set(const nros_cpp_node_t* node, const char* name, int v) {
    return Result(nros_cpp_node_set_param_integer(node, name, static_cast<int64_t>(v)));
}
inline Result node_param_set(const nros_cpp_node_t* node, const char* name, double v) {
    return Result(nros_cpp_node_set_param_double(node, name, v));
}
inline Result node_param_set(const nros_cpp_node_t* node, const char* name, const char* v) {
    return Result(nros_cpp_node_set_param_string(node, name, v));
}

// --- has --------------------------------------------------------------------

inline bool node_param_has(const nros_cpp_node_t* node, const char* name) {
    return nros_cpp_node_has_param(node, name);
}

#ifdef NROS_CPP_STD

// --- std::string values -----------------------------------------------------

inline Result node_param_declare(const nros_cpp_node_t* node, const char* name,
                                 const ::std::string& v) {
    return node_param_declare(node, name, v.c_str());
}
inline Result node_param_set(const nros_cpp_node_t* node, const char* name,
                             const ::std::string& v) {
    return node_param_set(node, name, v.c_str());
}
inline Result node_param_get(const nros_cpp_node_t* node, const char* name, ::std::string& out) {
    char buf[NROS_NODE_PARAM_STRING_BUF];
    buf[0] = '\0';
    Result r = node_param_get(node, name, buf, sizeof(buf));
    if (r.ok()) {
        out.assign(buf);
    }
    return r;
}

// --- std::vector<T> values --------------------------------------------------
//
// `nros::ComponentNode::declare_parameter<std::vector<double>>` is the caller,
// and the reason the array half of the FFI exists: deleting the C++ store
// without it would have turned a working weight matrix into a silently
// defaulted one.
//
// The store OWNS the elements (`heapless::Vec` in its slot), so there is no
// pool to keep alive here and no borrow for the caller to outlive — which is
// what `nros::ParameterServer`'s `seq_pool_` existed to provide and why it
// leaves with the member. `std::vector<bool>` is absent for the reason it was
// absent before: it has no `data()`.

inline nros_cpp_ret_t node_param_declare_array_ffi(const nros_cpp_node_t* node, const char* name,
                                                   const double* d, ::size_t len) {
    return nros_cpp_node_declare_param_double_array(node, name, d, len);
}
inline nros_cpp_ret_t node_param_declare_array_ffi(const nros_cpp_node_t* node, const char* name,
                                                   const int64_t* d, ::size_t len) {
    return nros_cpp_node_declare_param_integer_array(node, name, d, len);
}
inline nros_cpp_ret_t node_param_declare_array_ffi(const nros_cpp_node_t* node, const char* name,
                                                   const bool* d, ::size_t len) {
    return nros_cpp_node_declare_param_bool_array(node, name, d, len);
}

inline nros_cpp_ret_t node_param_get_array_ffi(const nros_cpp_node_t* node, const char* name,
                                               double* out, ::size_t cap, ::size_t* len) {
    return nros_cpp_node_get_param_double_array(node, name, out, cap, len);
}
inline nros_cpp_ret_t node_param_get_array_ffi(const nros_cpp_node_t* node, const char* name,
                                               int64_t* out, ::size_t cap, ::size_t* len) {
    return nros_cpp_node_get_param_integer_array(node, name, out, cap, len);
}
inline nros_cpp_ret_t node_param_get_array_ffi(const nros_cpp_node_t* node, const char* name,
                                               bool* out, ::size_t cap, ::size_t* len) {
    return nros_cpp_node_get_param_bool_array(node, name, out, cap, len);
}

template <typename T>
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name,
                                 const ::std::vector<T>& v) {
    return Result(node_param_declare_array_ffi(node, name, v.data(), v.size()));
}

/// Read an array parameter into a `std::vector<T>`.
///
/// TWO calls, deliberately: the first asks only for the LENGTH (capacity 0,
/// which the FFI answers with `Full` and the count), the second copies. The
/// alternative — a fixed guess at the capacity — is how a longer-than-expected
/// array comes back truncated, and a truncated weight matrix is a plausible
/// wrong answer rather than a visible failure.
template <typename T>
inline Result node_param_get(const nros_cpp_node_t* node, const char* name, ::std::vector<T>& out) {
    ::size_t len = 0;
    nros_cpp_ret_t probe = node_param_get_array_ffi(node, name, static_cast<T*>(nullptr), 0, &len);
    if (probe != NROS_CPP_RET_OK && probe != NROS_CPP_RET_FULL) {
        return Result(probe);
    }
    out.assign(len, T());
    if (len == 0) {
        return Result(NROS_CPP_RET_OK);
    }
    return Result(node_param_get_array_ffi(node, name, out.data(), out.size(), &len));
}

#endif // NROS_CPP_STD

} // namespace detail
} // namespace nros

#endif // NROS_CPP_NODE_PARAMETERS_HPP
