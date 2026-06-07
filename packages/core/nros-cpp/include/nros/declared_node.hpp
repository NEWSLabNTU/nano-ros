// nros-cpp: component-mode node declarations
// Freestanding C++ - no exceptions, no STL required

/**
 * @file component_node.hpp
 * @ingroup grp_node
 * @brief `nros::DeclaredNode` declaration handle.
 */

#ifndef NROS_CPP_COMPONENT_NODE_HPP
#define NROS_CPP_COMPONENT_NODE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/qos.hpp"
#include "nros/result.hpp"

namespace nros {

enum class NodeEntityKind : uint8_t {
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

struct NodeEntityDescriptor {
    const char* stable_id;
    const char* node_id;
    NodeEntityKind kind;
    const char* source_name;
    const char* type_name;
    const char* type_hash;
    const char* callback_id;
};

class NodeContext;

class DeclaredNode {
  public:
    DeclaredNode() : context_(nullptr), stable_id_(nullptr), runtime_handle_(nullptr) {}

    DeclaredNode(NodeContext* context, const char* stable_id, void* runtime_handle)
        : context_(context), stable_id_(stable_id), runtime_handle_(runtime_handle) {}

    bool is_valid() const { return context_ != nullptr && stable_id_ != nullptr; }
    const char* stable_id() const { return stable_id_ ? stable_id_ : ""; }
    void* runtime_handle() const { return runtime_handle_; }

    Result create_entity(const NodeEntityDescriptor& descriptor);
    Result create_entity(const char* stable_id, NodeEntityKind kind, const char* source_name,
                         const char* type_name = "", const char* type_hash = "",
                         const char* callback_id = nullptr);
    Result create_publisher(const char* stable_id, const char* topic_name, const char* type_name,
                            const char* type_hash = "");
    Result create_subscription(const char* stable_id, const char* topic_name,
                               const char* type_name, const char* callback_id,
                               const char* type_hash = "");
    Result create_timer(const char* stable_id, const char* period_ms, const char* callback_id);

    template <typename M>
    Result create_publisher(const char* topic_name,
                            const QoS& qos = QoS::default_profile());
    template <typename M>
    Result create_subscription(const char* topic_name, const char* callback_id,
                               const QoS& qos = QoS::default_profile());

  private:
    NodeContext* context_;
    const char* stable_id_;
    void* runtime_handle_;
};

} // namespace nros

#endif // NROS_CPP_COMPONENT_NODE_HPP
