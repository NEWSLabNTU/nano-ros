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
 * handle (an `rclcpp::Node` that was never opened) is an error, never a silent
 * write to some other node's parameters.
 *
 * ## Freestanding
 *
 * `<cstdint>` / `<cstddef>`, `nros/parameter.hpp` (`Seq<T, N>`, a value with
 * no STL in it) and the FFI header, nothing more. The `std::string` and
 * `std::vector<T>` overloads are behind `NROS_CPP_STD`, so a `-nostdinc++`
 * build gets the scalar set, the `Seq` array set and no parse error. Nothing
 * here declares a MEMBER of anything, so no capability probe can move a
 * layout (`check-cpp-capability-layout`).
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

#include "nros/declared_params.hpp" // phase-446 W6 -- `nros::param_type`
#include "nros/parameter.hpp"       // phase-426 W4 -- `nros::Seq<T, N>`
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

// --- array values, the shared half ------------------------------------------
//
// The three `*_array` FFI entry points, as an overload set on the ELEMENT
// type. Two value types ride on them and they are available in different
// builds, which is why this block sits OUTSIDE `NROS_CPP_STD`:
//
//   * `Seq<T, N>` (`nros/parameter.hpp`) - freestanding, fixed capacity, no
//     heap. Phase-426 W4: this is what `nros::ParameterServer<Cap>` used to
//     serve out of an inline bump pool, moved onto the one store so a
//     sequence parameter is visible to `ros2 param get` like every scalar.
//   * `std::vector<T>` - hosted, below, behind `NROS_CPP_STD`. A ported
//     `declare_parameter<std::vector<double>>` (the vendored ASI weight
//     matrix) is the caller, and the reason the array half of the FFI exists:
//     deleting the C++ store without it would have turned a working weight
//     matrix into a silently defaulted one.
//
// The store OWNS the elements (`heapless::Vec` in its slot), so there is no
// pool to keep alive here and no borrow for the caller to outlive - which is
// what `nros::ParameterServer`'s `seq_pool_` existed to provide and why it
// left with the class. `std::vector<bool>` is absent for the reason it was
// absent before: it has no `data()`. `Seq<bool, N>` is NOT - its storage is a
// plain `bool[N]`, so it has one.

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

inline nros_cpp_ret_t node_param_set_array_ffi(const nros_cpp_node_t* node, const char* name,
                                               const double* d, ::size_t len) {
    return nros_cpp_node_set_param_double_array(node, name, d, len);
}
inline nros_cpp_ret_t node_param_set_array_ffi(const nros_cpp_node_t* node, const char* name,
                                               const int64_t* d, ::size_t len) {
    return nros_cpp_node_set_param_integer_array(node, name, d, len);
}
inline nros_cpp_ret_t node_param_set_array_ffi(const nros_cpp_node_t* node, const char* name,
                                               const bool* d, ::size_t len) {
    return nros_cpp_node_set_param_bool_array(node, name, d, len);
}

// --- Seq<T, N> values --------------------------------------------------------
//
// Freestanding, so no `#ifdef`: this is the array surface a `-nostdinc++`
// node has. `Seq` is a VALUE - the store copies the elements in on declare
// and out on get, and the caller's `Seq` need not outlive either call.

template <typename T, ::size_t N>
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name,
                                 const ::nros::Seq<T, N>& v) {
    return Result(node_param_declare_array_ffi(node, name, v.data(), v.size()));
}

template <typename T, ::size_t N>
inline Result node_param_set(const nros_cpp_node_t* node, const char* name,
                             const ::nros::Seq<T, N>& v) {
    return Result(node_param_set_array_ffi(node, name, v.data(), v.size()));
}

/// Read an array parameter into a `Seq<T, N>`.
///
/// A stored array LONGER than `N` is refused (`ErrorCode::Full`) and `out` is
/// left cleared, never truncated: a short weight matrix is a plausible wrong
/// answer rather than a visible failure, which is the same reason the
/// `std::vector` read below asks for the length first.
template <typename T, ::size_t N>
inline Result node_param_get(const nros_cpp_node_t* node, const char* name,
                             ::nros::Seq<T, N>& out) {
    T buf[N];
    ::size_t len = 0;
    Result r(node_param_get_array_ffi(node, name, buf, N, &len));
    out.clear();
    if (!r.ok()) {
        return r;
    }
    for (::size_t i = 0; i < len; ++i) {
        (void)out.push_back(buf[i]);
    }
    return r;
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

template <typename T>
inline Result node_param_declare(const nros_cpp_node_t* node, const char* name,
                                 const ::std::vector<T>& v) {
    return Result(node_param_declare_array_ffi(node, name, v.data(), v.size()));
}

/// Set a declared array parameter from a `std::vector<T>`.
///
/// phase-426 W4 -- the hosted twin of the `Seq` setter. It was missing with
/// the rest of the array setters, so `set_parameter<std::vector<double>>` did
/// not compile while `declare_parameter<std::vector<double>>` did: an ASI
/// weight matrix could be declared and never updated.
template <typename T>
inline Result node_param_set(const nros_cpp_node_t* node, const char* name,
                             const ::std::vector<T>& v) {
    return Result(node_param_set_array_ffi(node, name, v.data(), v.size()));
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

// --- descriptors, undeclare, listing and the on-set hook (phase-417 W4.a) ----
//
// Forwarders onto the same one store. TEXT crosses the FFI as a borrowed
// `const char*` in and a caller-owned `char*` out; there is no descriptor
// STRUCT, because one would either carry a pointer into the store or an inline
// buffer, and the second would make `NROS_MAX_PARAM_DESCRIPTION_LEN` a layout
// (`check-cpp-capability-layout`: a probe may gate a METHOD, never a `sizeof`).
// Nothing below declares a member of anything.

inline Result node_param_add_description(const nros_cpp_node_t* node, const char* name,
                                         const char* description,
                                         const char* additional_constraints) {
    return Result(
        nros_cpp_node_add_param_description(node, name, description, additional_constraints));
}

inline Result node_param_set_read_only(const nros_cpp_node_t* node, const char* name,
                                       bool read_only) {
    return Result(nros_cpp_node_set_param_read_only(node, name, read_only));
}

inline Result node_param_add_range(const nros_cpp_node_t* node, const char* name, int64_t from,
                                   int64_t to, int64_t step) {
    return Result(nros_cpp_node_add_param_constraint_integer(node, name, from, to, step));
}

inline Result node_param_add_range(const nros_cpp_node_t* node, const char* name, double from,
                                   double to, double step) {
    return Result(nros_cpp_node_add_param_constraint_double(node, name, from, to, step));
}

inline Result node_param_undeclare(const nros_cpp_node_t* node, const char* name) {
    return Result(nros_cpp_node_undeclare_param(node, name));
}

inline Result node_param_get_type(const nros_cpp_node_t* node, const char* name, int32_t& out) {
    return Result(nros_cpp_node_get_param_type(node, name, &out));
}

inline Result node_param_describe(const nros_cpp_node_t* node, const char* name,
                                  char* out_description, ::size_t description_len,
                                  char* out_constraints, ::size_t constraints_len,
                                  bool* out_read_only, int32_t* out_type) {
    return Result(nros_cpp_node_describe_param(node, name, out_description, description_len,
                                               out_constraints, constraints_len, out_read_only,
                                               out_type));
}

inline Result node_param_get_range(const nros_cpp_node_t* node, const char* name, int64_t* from,
                                   int64_t* to, int64_t* step) {
    return Result(nros_cpp_node_get_param_integer_range(node, name, from, to, step));
}

inline Result node_param_get_range(const nros_cpp_node_t* node, const char* name, double* from,
                                   double* to, double* step) {
    return Result(nros_cpp_node_get_param_double_range(node, name, from, to, step));
}

inline Result node_param_list(const nros_cpp_node_t* node, const char* prefix, char* out_names,
                              ::size_t name_stride, ::size_t max_names, ::size_t* out_count) {
    return Result(
        nros_cpp_node_list_params(node, prefix, out_names, name_stride, max_names, out_count));
}

} // namespace detail
} // namespace nros

namespace rclcpp {

/// `rclcpp::ParameterType` — the `rcl_interfaces/msg/ParameterType` codes,
/// under upstream's own name and with upstream's own enumerator spellings.
///
/// phase-417 W4.a — C had `nros_parameter_type_t` and Rust had
/// `ParameterType`, both mirroring the same message, and C++ named neither, so
/// a ported file that wrote `rclcpp::ParameterType::PARAMETER_DOUBLE` did not
/// compile and no C++ accessor could have returned one.
///
/// An `enum` and not an `enum class`, matching upstream: rclcpp's is a plain
/// enum whose enumerators carry the `PARAMETER_` prefix, so
/// `rclcpp::ParameterType::PARAMETER_DOUBLE` and the bare
/// `rclcpp::PARAMETER_DOUBLE` both resolve there, and both resolve here.
/// The values are `nros::param_type`'s, which is where the codes live; this
/// enum NAMES them rather than restating them, so the two cannot drift.
enum ParameterType {
    PARAMETER_NOT_SET = 0,
    PARAMETER_BOOL = ::nros::param_type::BOOL,
    PARAMETER_INTEGER = ::nros::param_type::INTEGER,
    PARAMETER_DOUBLE = ::nros::param_type::DOUBLE,
    PARAMETER_STRING = ::nros::param_type::STRING,
    PARAMETER_BYTE_ARRAY = ::nros::param_type::BYTE_ARRAY,
    PARAMETER_BOOL_ARRAY = ::nros::param_type::BOOL_ARRAY,
    PARAMETER_INTEGER_ARRAY = ::nros::param_type::INTEGER_ARRAY,
    PARAMETER_DOUBLE_ARRAY = ::nros::param_type::DOUBLE_ARRAY,
    PARAMETER_STRING_ARRAY = ::nros::param_type::STRING_ARRAY,
};

/// The proposed write an on-set-parameters callback sees.
///
/// rclcpp hands the callback a `std::vector<rclcpp::Parameter>` and takes a
/// `rcl_interfaces::msg::SetParametersResult` back. Neither type exists here —
/// both are generated messages, and the vector needs an allocator — so the
/// callback sees ONE write at a time as scalars, and answers with a `bool`
/// whose `false` is upstream's `successful = false`.
using ParameterWrite = ::nros_cpp_param_write_t;

/// `bool (*)(const ParameterWrite*, void* context)`. A plain function pointer
/// with a context, not a `std::function`: there is no allocator to hold a
/// closure, and the freestanding lane has no `<functional>`.
using OnSetParametersCallbackType = ::nros_cpp_param_callback_t;

/// `rclcpp::ParameterDescriptor` — a parameter's metadata, as a value a
/// freestanding C++ TU can build and read.
///
/// Upstream's is `rcl_interfaces::msg::ParameterDescriptor`, a generated
/// message whose strings own their storage. Ours BORROWS its text: the
/// pointers are read during the call that consumes the descriptor (the store
/// copies into its own slot) and never retained, so a descriptor can be a
/// string-literal aggregate on the stack with no allocator anywhere.
///
/// Reading one back needs the mirror of that: `Node::describe_parameter` takes
/// a caller-owned text buffer and points the two `const char*` at it. There is
/// no way around it — a returned descriptor that owned its strings would need
/// the allocator this type exists to avoid.
struct ParameterDescriptor {
    /// Human-readable description; NULL or "" for none.
    const char* description;
    /// Free-text extra constraints, which `ros2 param describe` prints.
    const char* additional_constraints;
    /// Refuse every write after the declaration.
    bool read_only;
    /// A range applies. `integer_range` picks which of the two below is read,
    /// from the parameter's own type.
    bool has_range;
    /// The range bounds, read as integers for an integer parameter and as
    /// doubles for a double one. Two representations rather than a union
    /// because a union in a freestanding aggregate cannot have a default
    /// member initialiser at C++14, and this type must be brace-initialisable.
    int64_t integer_from, integer_to, integer_step;
    /// @see integer_from
    double double_from, double_to, double_step;
};

/// A `ParameterDescriptor` with nothing set — the C++14-friendly way to build
/// one field at a time (`auto d = rclcpp::parameter_descriptor(); d.read_only
/// = true;`), since designated initialisers are C++20.
inline ParameterDescriptor parameter_descriptor() {
    ParameterDescriptor d = {nullptr, nullptr, false, false, 0, 0, 0, 0.0, 0.0, 0.0};
    return d;
}

/// The token `remove_on_set_parameters_callback` takes.
///
/// rclcpp returns an owning `ParameterCallbackHandle` shared_ptr whose
/// destruction unregisters. With no allocator there is nothing to own, so this
/// is a plain token and unregistering is explicit.
using ParameterCallbackHandle = uint16_t;

} // namespace rclcpp

namespace nros {
namespace detail {

inline Result node_param_add_on_set_callback(const nros_cpp_node_t* node,
                                             ::rclcpp::OnSetParametersCallbackType callback,
                                             void* context,
                                             ::rclcpp::ParameterCallbackHandle* out_handle) {
    return Result(nros_cpp_node_add_on_set_params_callback(node, callback, context, out_handle));
}

inline Result node_param_remove_on_set_callback(const nros_cpp_node_t* node,
                                                ::rclcpp::ParameterCallbackHandle handle) {
    return Result(nros_cpp_node_remove_on_set_params_callback(node, handle));
}

inline Result node_params_set_atomically(const nros_cpp_node_t* node,
                                         const ::rclcpp::ParameterWrite* writes, ::size_t count) {
    return Result(nros_cpp_node_set_params_atomically(node, writes, count));
}

/// Attach a whole `ParameterDescriptor` to an ALREADY-DECLARED parameter.
///
/// Three FFI calls rather than a descriptor-carrying declare, deliberately:
/// the store's own verbs are rclc's post-declare mutators, so this is
/// composition rather than a fifth declare entry point per type. The executor
/// is single-threaded on every platform we ship, so nothing can observe the
/// parameter between the declare and these calls; on a platform where that
/// stopped being true, a descriptor-carrying declare would be the fix.
///
/// The first failing step wins, and the ones after it are not attempted —
/// a descriptor half applied is worse than one not applied.
inline Result node_param_apply_descriptor(const nros_cpp_node_t* node, const char* name,
                                          const ::rclcpp::ParameterDescriptor& d, int param_type) {
    Result r = node_param_add_description(node, name, d.description, d.additional_constraints);
    if (!r.ok()) {
        return r;
    }
    if (d.has_range) {
        r = (param_type == ::nros::param_type::INTEGER)
                ? node_param_add_range(node, name, d.integer_from, d.integer_to, d.integer_step)
                : node_param_add_range(node, name, d.double_from, d.double_to, d.double_step);
        if (!r.ok()) {
            return r;
        }
    }
    // read_only LAST: it is the one flag that would refuse the writes above if
    // it were set first, which is the ordering bug this comment exists to
    // prevent someone re-introducing.
    if (d.read_only) {
        r = node_param_set_read_only(node, name, true);
    }
    return r;
}

// phase-446 W6 -- the contract type a `declare_parameter<T>` declares, as the
// rcl_interfaces code the declared-parameter table carries. Mirrors the store
// overloads above: `bool`, any other integer (`int` and `int64_t` both reach
// the integer slot), a floating-point type, and a string. Lives beside those
// overloads so the two cannot drift apart; `Node::declare_parameter` passes it
// to `Node::check_declared_param`.
//
// Spelled as explicit specializations, not `std::is_integral` & co.: this
// header is parsed under `-nostdinc++` (the ThreadX shim probe in
// `check-cpp`), where `<type_traits>` is not available. Anything not listed
// is a string, which is also what the `const char*` overload stores.
template <typename T> struct node_param_type {
    static constexpr int value = ::nros::param_type::STRING;
};
#define NROS_NODE_PARAM_TYPE_(T, CODE)                                                             \
    template <> struct node_param_type<T> {                                                        \
        static constexpr int value = ::nros::param_type::CODE;                                     \
    }
NROS_NODE_PARAM_TYPE_(bool, BOOL);
NROS_NODE_PARAM_TYPE_(char, INTEGER);
NROS_NODE_PARAM_TYPE_(signed char, INTEGER);
NROS_NODE_PARAM_TYPE_(unsigned char, INTEGER);
NROS_NODE_PARAM_TYPE_(short, INTEGER);
NROS_NODE_PARAM_TYPE_(unsigned short, INTEGER);
NROS_NODE_PARAM_TYPE_(int, INTEGER);
NROS_NODE_PARAM_TYPE_(unsigned int, INTEGER);
NROS_NODE_PARAM_TYPE_(long, INTEGER);
NROS_NODE_PARAM_TYPE_(unsigned long, INTEGER);
NROS_NODE_PARAM_TYPE_(long long, INTEGER);
NROS_NODE_PARAM_TYPE_(unsigned long long, INTEGER);
NROS_NODE_PARAM_TYPE_(float, DOUBLE);
NROS_NODE_PARAM_TYPE_(double, DOUBLE);
NROS_NODE_PARAM_TYPE_(long double, DOUBLE);
#undef NROS_NODE_PARAM_TYPE_

/// The array code for a scalar code: `std::vector<T>` declares the array of
/// whatever `T` declares.
constexpr int node_param_array_type(int scalar) {
    return (scalar == ::nros::param_type::BOOL)      ? ::nros::param_type::BOOL_ARRAY
           : (scalar == ::nros::param_type::INTEGER) ? ::nros::param_type::INTEGER_ARRAY
           : (scalar == ::nros::param_type::DOUBLE)  ? ::nros::param_type::DOUBLE_ARRAY
                                                     : ::nros::param_type::STRING_ARRAY;
}
template <typename T, ::size_t N> struct node_param_type<::nros::Seq<T, N>> {
    static constexpr int value = node_param_array_type(node_param_type<T>::value);
};
#ifdef NROS_CPP_STD
template <typename T, typename A> struct node_param_type<::std::vector<T, A>> {
    static constexpr int value = node_param_array_type(node_param_type<T>::value);
};
#endif

} // namespace detail
} // namespace nros

#endif // NROS_CPP_NODE_PARAMETERS_HPP
