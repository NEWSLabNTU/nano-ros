// nros-cpp: the parameters a node's contract DECLARES, and the lookup over them
// Freestanding C++ -- no STL.

/**
 * @file declared_params.hpp
 * @ingroup grp_parameter
 * @brief phase-446 W6 -- the parameters each node's contract declares, reaching
 *        the code that declares them.
 *
 * The launch contract states each node's parameters by NAME and TYPE:
 *
 *     nodes:
 *       mrm_handler:
 *         params:
 *           update_rate: { type: integer }
 *
 * and the build sizes the parameter store from that statement (phase-446 W4).
 * A store sized from a declaration is only right while the code declares what
 * the contract says, so `Node::declare_parameter` looks each name up
 * here: a name the contract does not declare, or a type that differs, refuses
 * the boot naming the node, the parameter and the contract.
 *
 * The table is written by `nros ws entity-inventory --output-params-header`
 * and put on the component library's include path by
 * `nano_ros_node_register()`, beside the declared-QoS table. It is keyed on
 * the node's FULLY-QUALIFIED name, because one component class may run as
 * several nodes and each has its own contract entry.
 *
 * # ABSENCE IS NOT A DECLARATION OF NOTHING
 *
 * A node with no row in the table has no `params:` in its contract, and its
 * code is not checked -- exactly as before the contract could say anything.
 * An image with no table at all compiles to the same code with every lookup
 * answering "not declared", so nothing is checked.
 *
 * # Names every node carries
 *
 * `use_sim_time` and `start_type_description_service` (rclcpp declares both
 * on every node) and `qos_overrides.*` (derived from the contract itself) are
 * never checked. These are the names play_launch exempts when it holds a
 * launch file to the same contract, so the two checks agree.
 */

#ifndef NROS_CPP_DECLARED_PARAMS_HPP
#define NROS_CPP_DECLARED_PARAMS_HPP

#include <stddef.h>

#if defined(__has_include)
#if __has_include(<nros/nros_declared_params_generated.h>)
#include <nros/nros_declared_params_generated.h>
#endif
#endif

namespace nros {

/// The node has no `params:` in its contract, so nothing is checked.
constexpr int DECLARED_PARAM_NODE_UNCHECKED = -1;
/// The node declares parameters, and this name is not one of them.
constexpr int DECLARED_PARAM_UNDECLARED = -2;

/// `rcl_interfaces/msg/ParameterType`, the numbering the generated table uses.
namespace param_type {
constexpr int BOOL = 1;
constexpr int INTEGER = 2;
constexpr int DOUBLE = 3;
constexpr int STRING = 4;
constexpr int BYTE_ARRAY = 5;
constexpr int BOOL_ARRAY = 6;
constexpr int INTEGER_ARRAY = 7;
constexpr int DOUBLE_ARRAY = 8;
constexpr int STRING_ARRAY = 9;
} // namespace param_type

namespace declared_params {

/// A node whose contract declares parameters, and the contract it came from.
struct Node {
    const char* fqn;
    const char* contract;
};

/// One declared parameter.
struct Row {
    const char* fqn;
    const char* name;
    int type;
};

#if defined(NROS_DECLARED_PARAM_NODES) && defined(NROS_DECLARED_PARAM_ROWS)
#define NROS_DECLARED_PARAM_NODE(nros_fqn, nros_contract) {(nros_fqn), (nros_contract)},
#define NROS_DECLARED_PARAM_ROW(nros_fqn, nros_name, nros_type)                                    \
    {(nros_fqn), (nros_name), (nros_type)},
/// Trailing sentinels keep each array non-empty, which C++ requires; the
/// COUNTs are what the lookups read.
constexpr Node NODES[] = {NROS_DECLARED_PARAM_NODES{nullptr, nullptr}};
constexpr Row ROWS[] = {NROS_DECLARED_PARAM_ROWS{nullptr, nullptr, 0}};
#undef NROS_DECLARED_PARAM_NODE
#undef NROS_DECLARED_PARAM_ROW
constexpr size_t NODE_COUNT = NROS_DECLARED_PARAM_NODE_COUNT;
constexpr size_t ROW_COUNT = NROS_DECLARED_PARAM_ROW_COUNT;
#else
constexpr Node NODES[] = {{nullptr, nullptr}};
constexpr Row ROWS[] = {{nullptr, nullptr, 0}};
constexpr size_t NODE_COUNT = 0;
constexpr size_t ROW_COUNT = 0;
#endif

} // namespace declared_params

namespace detail {

/// `strcmp(a, b) == 0` as a C++14 constant expression.
constexpr bool declared_params_streq(const char* a, const char* b) {
    return (*a == *b) ? ((*a == '\0') ? true : declared_params_streq(a + 1, b + 1)) : false;
}

/// Does `s` start with `prefix`?
constexpr bool declared_params_starts_with(const char* s, const char* prefix) {
    return (*prefix == '\0')
               ? true
               : ((*s == *prefix) ? declared_params_starts_with(s + 1, prefix + 1) : false);
}

/// Skip the namespace's trailing `/`s: `/` and `` both mean the root.
constexpr bool declared_params_ns_done(const char* ns) {
    return (*ns == '\0') ? true : ((*ns == '/') ? declared_params_ns_done(ns + 1) : false);
}

/// `fqn == ns + "/" + name`, with `ns` of `/` or `` meaning the root. Compared
/// piecewise so no buffer is built on a freestanding target.
constexpr bool declared_params_fqn_tail(const char* fqn, const char* name) {
    return (*fqn == '/') ? declared_params_streq(fqn + 1, name) : false;
}
constexpr bool declared_params_fqn_eq(const char* fqn, const char* ns, const char* name) {
    return declared_params_ns_done(ns)
               ? declared_params_fqn_tail(fqn, name)
               : ((*fqn == *ns) ? declared_params_fqn_eq(fqn + 1, ns + 1, name) : false);
}

constexpr const ::nros::declared_params::Node*
declared_params_find_node(const ::nros::declared_params::Node* nodes, size_t n, const char* ns,
                          const char* name) {
    return (n == 0) ? nullptr
                    : (declared_params_fqn_eq(nodes[0].fqn, ns, name)
                           ? nodes
                           : declared_params_find_node(nodes + 1, n - 1, ns, name));
}

constexpr int declared_params_find_row(const ::nros::declared_params::Row* rows, size_t n,
                                       const char* fqn, const char* param) {
    return (n == 0) ? ::nros::DECLARED_PARAM_UNDECLARED
                    : ((declared_params_streq(rows[0].fqn, fqn) &&
                        declared_params_streq(rows[0].name, param))
                           ? rows[0].type
                           : declared_params_find_row(rows + 1, n - 1, fqn, param));
}

} // namespace detail

/// The names no contract has to declare -- the three play_launch exempts.
constexpr bool declared_param_exempt(const char* name) {
    return detail::declared_params_streq(name, "use_sim_time") ||
           detail::declared_params_streq(name, "start_type_description_service") ||
           detail::declared_params_starts_with(name, "qos_overrides.");
}

/// The declaring node named `ns` + `name`, or `nullptr` when its contract has
/// no `params:` (then nothing about it is checked).
constexpr const declared_params::Node* declared_param_node(const char* ns, const char* name) {
    return detail::declared_params_find_node(declared_params::NODES, declared_params::NODE_COUNT,
                                             ns, name);
}

/// What `node` declares `param` as: a `param_type` code, or
/// @ref DECLARED_PARAM_UNDECLARED. `node` is what @ref declared_param_node
/// returned; `nullptr` answers @ref DECLARED_PARAM_NODE_UNCHECKED.
constexpr int declared_param_type(const declared_params::Node* node, const char* param) {
    return (node == nullptr)
               ? DECLARED_PARAM_NODE_UNCHECKED
               : detail::declared_params_find_row(declared_params::ROWS, declared_params::ROW_COUNT,
                                                  node->fqn, param);
}

/// The contract's spelling of a `param_type` code, for a diagnostic.
constexpr const char* declared_param_type_name(int type) {
    return type == param_type::BOOL            ? "bool"
           : type == param_type::INTEGER       ? "integer"
           : type == param_type::DOUBLE        ? "double"
           : type == param_type::STRING        ? "string"
           : type == param_type::BYTE_ARRAY    ? "byte_array"
           : type == param_type::BOOL_ARRAY    ? "bool_array"
           : type == param_type::INTEGER_ARRAY ? "integer_array"
           : type == param_type::DOUBLE_ARRAY  ? "double_array"
           : type == param_type::STRING_ARRAY  ? "string_array"
                                               : "?";
}

} // namespace nros

#endif // NROS_CPP_DECLARED_PARAMS_HPP
