// nros-cpp: CallbackGroup — Phase 273 (RFC-0047)
// Freestanding C++ — no exceptions, no STL required

/**
 * @file callback_group.hpp
 * @ingroup grp_node
 * @brief `nros::CallbackGroup` — named scheduling group token (RFC-0047).
 *
 * A `CallbackGroup` is a name-only token produced by
 * `Node::create_callback_group("name")` or
 * `ComponentNode::create_callback_group("name")`. Pass it to
 * `create_timer_in` / `create_subscription_in` / `create_publisher_in` to
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

#ifndef NROS_CPP_CALLBACK_GROUP_HPP
#define NROS_CPP_CALLBACK_GROUP_HPP

namespace nros {

/// Named callback-group token (RFC-0047 / Phase 273).
///
/// Create via `Node::create_callback_group("ctrl")` or
/// `ComponentNode::create_callback_group("ctrl")`.
///
/// Pass to `create_timer_in` / `create_subscription_in` /
/// `create_publisher_in` to bind the entity to the group's SchedContext.
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

} // namespace nros

#endif // NROS_CPP_CALLBACK_GROUP_HPP
