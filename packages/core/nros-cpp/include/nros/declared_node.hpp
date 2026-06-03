// nros-cpp: component-mode node declarations
// Freestanding C++ - no exceptions, no STL required

/**
 * @file component_node.hpp
 * @ingroup grp_node
 * @brief `nros::ComponentNode` declaration handle.
 */

#ifndef NROS_CPP_COMPONENT_NODE_HPP
#define NROS_CPP_COMPONENT_NODE_HPP

#include <cstdint>

#include "nros/result.hpp"

namespace nros {

enum class ComponentEntityKind : uint8_t {
    Publisher = 0,
    Subscription = 1,
    Timer = 2,
    ServiceServer = 3,
    ServiceClient = 4,
    ActionServer = 5,
    ActionClient = 6,
    Parameter = 7,
};

enum class CallbackEffectKind : uint8_t {
    Reads = 0,
    Publishes = 1,
    Writes = 2,
};

struct NodeOptions {
    const char* name;
    const char* namespace_;
    uint32_t domain_id;

    static constexpr NodeOptions make(const char* node_name) {
        return NodeOptions{node_name, "/", 0};
    }
};

struct ComponentEntityDescriptor {
    const char* stable_id;
    const char* node_id;
    ComponentEntityKind kind;
    const char* source_name;
    const char* type_name;
    const char* type_hash;
    const char* callback_id;
};

class ComponentContext;

class ComponentNode {
  public:
    ComponentNode() : context_(nullptr), stable_id_(nullptr), runtime_handle_(nullptr) {}

    ComponentNode(ComponentContext* context, const char* stable_id, void* runtime_handle)
        : context_(context), stable_id_(stable_id), runtime_handle_(runtime_handle) {}

    bool is_valid() const { return context_ != nullptr && stable_id_ != nullptr; }
    const char* stable_id() const { return stable_id_ ? stable_id_ : ""; }
    void* runtime_handle() const { return runtime_handle_; }

    Result create_entity(const ComponentEntityDescriptor& descriptor);

  private:
    ComponentContext* context_;
    const char* stable_id_;
    void* runtime_handle_;
};

} // namespace nros

#endif // NROS_CPP_COMPONENT_NODE_HPP
