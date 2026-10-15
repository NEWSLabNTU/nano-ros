// nros-cpp: CallbackGroup — Phase 273 (RFC-0047)
// Freestanding C++ — no exceptions, no STL required

/**
 * @file callback_group.hpp
 * @ingroup grp_node
 * @brief `rclcpp::CallbackGroup` — named scheduling group token (RFC-0047).
 *
 * A `CallbackGroup` is a name-only token produced by
 * `Node::create_callback_group("name")` or
 * `Node::create_callback_group("name")`. Pass it to
 * `create_timer_in_group` / `create_subscription_in_group` / `create_publisher_in` to
 * associate an entity with a named group whose `SchedContext` binding is
 * resolved at runtime via the executor's `group_sched_table`.
 *
 * rclcpp analogy: `rclcpp::CallbackGroup::SharedPtr` — but value-typed and
 * heap-free. The name is stored as a `const char*` pointer; use string
 * literals (or storage that outlives the group token and all entities created
 * in it).
 *
 * rclrs analogy: `callback_group: &str` parameter on entity creation.
 */

#ifndef NROS_RCLCPP_CALLBACK_GROUP_HPP
#define NROS_RCLCPP_CALLBACK_GROUP_HPP

#include <stdint.h>

namespace rclcpp {

/// Named callback-group token (RFC-0047 / Phase 273).
///
/// Create via `Node::create_callback_group("ctrl")` or
/// `Node::create_callback_group("ctrl")`.
///
/// Pass to `create_timer_in_group` / `create_subscription_in_group` /
/// `create_publisher_in_group` to bind the entity to the group's SchedContext.
///
/// The pointed-at string MUST outlive the group token and every entity that
/// references it — use compile-time string literals.
struct CallbackGroup {
    /// Group name, null-terminated. `nullptr` behaves like no group (node default).
    const char* name;

    /// Construct from a string literal (or static-lifetime string).
    explicit CallbackGroup(const char* name_) : name(name_) {}

    /// Null-check-safe accessor.
    const char* get_name() const { return name; }
};

namespace detail {

/// phase-459 W6 -- the code `set_error` records when a node's callback groups
/// disagree with its registration's `CALLBACK_GROUPS`. Same shape as
/// `DECLARED_DEPTH_MISMATCH` (-403): the image contradicting its own
/// declaration, not a backend failure.
constexpr int32_t DECLARED_CALLBACK_GROUP_MISMATCH = -459;

/// The groups one component's registration DECLARED, and which of them its
/// constructor CREATED.
///
/// Why this exists: `CALLBACK_GROUPS` is what the tier derivation reads
/// (phase-459 W1, through `nros-metadata.json`), and `create_callback_group` is
/// what the executor binds. Two spellings of one fact, so nothing stopped them
/// from disagreeing — a group created in code that the keyword did not declare
/// is scheduled on a tier the derivation never ranked, and a declared group
/// never created ranks a tier nothing runs on.
///
/// `list` is the registration's names as `|a|b|` (the
/// `NROS_DECLARED_CALLBACK_GROUPS` definition `nano_ros_node_register` puts on
/// the component's own target), or `nullptr` when the registration said
/// nothing, in which case nothing here refuses. It is installed by the
/// `NROS_COMPONENT` factory around the constructor, in the COMPONENT's
/// translation unit — never read by `Node`'s inline methods, which are compiled
/// into many TUs that each carry a different list (an ODR violation).
struct DeclaredCallbackGroups {
    const char* list;
    /// Bit i set: the i-th declared name was created. Lists past 32 names are
    /// not tracked for the "declared but never created" half.
    uint32_t created;
};

/// The scope the factory installs. A function-local static of POINTER type is
/// constant-initialised, so it needs no guard and no runtime — usable on a
/// freestanding target. Boot constructs components one at a time on one thread.
inline DeclaredCallbackGroups*& declared_callback_groups_scope() {
    static DeclaredCallbackGroups* scope = nullptr;
    return scope;
}

/// Index of `name` in a `|a|b|` list, or -1. Freestanding: no `<cstring>`.
inline int declared_callback_group_index(const char* list, const char* name) {
    if (list == nullptr || name == nullptr) return -1;
    int index = 0;
    const char* p = list;
    while (*p == '|') {
        const char* start = p + 1;
        const char* end = start;
        while (*end != '\0' && *end != '|')
            ++end;
        if (*end == '\0') return -1; // malformed tail: no closing bar
        if (end != start) {
            const char* a = start;
            const char* b = name;
            while (a != end && *b != '\0' && *a == *b) {
                ++a;
                ++b;
            }
            if (a == end && *b == '\0') return index;
            ++index;
        }
        p = end;
    }
    return -1;
}

/// Number of names in a `|a|b|` list.
inline int declared_callback_group_count(const char* list) {
    if (list == nullptr) return 0;
    int n = 0;
    for (const char* p = list; *p != '\0'; ++p) {
        if (*p == '|' && p[1] != '\0' && p[1] != '|') ++n;
    }
    return n;
}

/// `create_callback_group`'s half: is `name` admitted under the scope the
/// factory installed? Records it when it is. `true` outside a factory scope,
/// for a registration that declared nothing, and for the no-group name
/// (`nullptr` / empty), which means "node default".
inline bool admit_callback_group(const char* name) {
    DeclaredCallbackGroups* scope = declared_callback_groups_scope();
    if (scope == nullptr || scope->list == nullptr) return true;
    if (name == nullptr || *name == '\0') return true;
    const int i = declared_callback_group_index(scope->list, name);
    if (i < 0) return false;
    if (i < 32) scope->created |= (uint32_t{1} << i);
    return true;
}

/// The factory's half, after the constructor: the first DECLARED name the
/// constructor never created, written to `out`/`out_len`, or `false` when there
/// is none. Node-level placement — ONE declared group and none created in code —
/// is allowed (every member of the node runs on that group's tier), so a
/// one-name list never reports.
inline bool first_uncreated_callback_group(const DeclaredCallbackGroups& g, const char** out,
                                           int* out_len) {
    if (declared_callback_group_count(g.list) <= 1) return false;
    int index = 0;
    const char* p = g.list;
    while (*p == '|') {
        const char* start = p + 1;
        const char* end = start;
        while (*end != '\0' && *end != '|')
            ++end;
        if (*end == '\0') return false;
        if (end != start) {
            if (index < 32 && (g.created & (uint32_t{1} << index)) == 0) {
                *out = start;
                *out_len = static_cast<int>(end - start);
                return true;
            }
            ++index;
        }
        p = end;
    }
    return false;
}

} // namespace detail

} // namespace rclcpp

#endif // NROS_RCLCPP_CALLBACK_GROUP_HPP
