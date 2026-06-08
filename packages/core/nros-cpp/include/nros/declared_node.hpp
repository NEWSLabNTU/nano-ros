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

#ifndef NROS_CPP_DEPRECATED
#if __cplusplus >= 201402L
#define NROS_CPP_DEPRECATED [[deprecated]]
#elif defined(__GNUC__) || defined(__clang__)
#define NROS_CPP_DEPRECATED __attribute__((deprecated))
#elif defined(_MSC_VER)
#define NROS_CPP_DEPRECATED __declspec(deprecated)
#else
#define NROS_CPP_DEPRECATED
#endif
#endif

namespace nros {

static constexpr size_t DECLARED_NODE_SYNTHETIC_ID_MAX = 96;

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

class DeclaredEntity {
  public:
    DeclaredEntity() : kind_(NodeEntityKind::Publisher), valid_(false) { stable_id_[0] = '\0'; }

    bool is_valid() const { return valid_; }
    const char* stable_id() const { return valid_ ? stable_id_ : ""; }
    NodeEntityKind kind() const { return kind_; }

  private:
    friend class DeclaredNode;

    Result assign(const char* stable_id, NodeEntityKind kind);

    char stable_id_[DECLARED_NODE_SYNTHETIC_ID_MAX];
    NodeEntityKind kind_;
    bool valid_;
};

class DeclaredCallback {
  public:
    DeclaredCallback() : valid_(false) { callback_id_[0] = '\0'; }

    bool is_valid() const { return valid_; }
    const char* callback_id() const { return valid_ ? callback_id_ : ""; }

  private:
    friend class DeclaredNode;

    Result assign(const char* callback_id);

    char callback_id_[DECLARED_NODE_SYNTHETIC_ID_MAX];
    bool valid_;
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

    NROS_CPP_DEPRECATED Result create_entity(const NodeEntityDescriptor& descriptor);
    NROS_CPP_DEPRECATED Result create_entity(const char* stable_id, NodeEntityKind kind,
                                             const char* source_name, const char* type_name = "",
                                             const char* type_hash = "",
                                             const char* callback_id = nullptr);
    NROS_CPP_DEPRECATED Result create_publisher(const char* stable_id, const char* topic_name,
                                                const char* type_name, const char* type_hash = "");
    NROS_CPP_DEPRECATED Result create_subscription(const char* stable_id, const char* topic_name,
                                                   const char* type_name, const char* callback_id,
                                                   const char* type_hash = "");
    NROS_CPP_DEPRECATED Result create_timer(const char* stable_id, const char* period_ms,
                                            const char* callback_id);
    NROS_CPP_DEPRECATED Result create_service_server(const char* stable_id,
                                                     const char* service_name,
                                                     const char* type_name,
                                                     const char* callback_id = nullptr,
                                                     const char* type_hash = "");
    NROS_CPP_DEPRECATED Result create_service_client(const char* stable_id,
                                                     const char* service_name,
                                                     const char* type_name,
                                                     const char* callback_id = nullptr,
                                                     const char* type_hash = "");
    NROS_CPP_DEPRECATED Result create_action_server(const char* stable_id, const char* action_name,
                                                    const char* type_name,
                                                    const char* callback_id = nullptr,
                                                    const char* type_hash = "");
    NROS_CPP_DEPRECATED Result create_action_client(const char* stable_id, const char* action_name,
                                                    const char* type_name,
                                                    const char* callback_id = nullptr,
                                                    const char* type_hash = "");

    Result declare_callback(DeclaredCallback& out, const char* callback_id);
    Result create_publisher(DeclaredEntity& out, const char* topic_name, const char* type_name,
                            const char* type_hash = "");
    Result create_subscription(DeclaredEntity& out, const char* topic_name, const char* type_name,
                               const DeclaredCallback& callback, const char* type_hash = "");
    Result create_timer(DeclaredEntity& out, const char* period_ms,
                        const DeclaredCallback& callback);
    Result create_service_server(DeclaredEntity& out, const char* service_name,
                                 const char* type_name, const DeclaredCallback& callback,
                                 const char* type_hash = "");
    Result create_service_client(DeclaredEntity& out, const char* service_name,
                                 const char* type_name, const char* type_hash = "");
    Result create_service_client(DeclaredEntity& out, const char* service_name,
                                 const char* type_name, const DeclaredCallback& callback,
                                 const char* type_hash = "");
    Result create_action_server(DeclaredEntity& out, const char* action_name, const char* type_name,
                                const DeclaredCallback& callback, const char* type_hash = "");
    Result create_action_client(DeclaredEntity& out, const char* action_name, const char* type_name,
                                const char* type_hash = "");
    Result create_action_client(DeclaredEntity& out, const char* action_name, const char* type_name,
                                const DeclaredCallback& callback, const char* type_hash = "");

    template <typename M>
    Result create_publisher(const char* topic_name, const QoS& qos = QoS::default_profile());
    template <typename M>
    NROS_CPP_DEPRECATED Result create_subscription(const char* topic_name, const char* callback_id,
                                                   const QoS& qos = QoS::default_profile());
    template <typename M>
    Result create_publisher(DeclaredEntity& out, const char* topic_name,
                            const QoS& qos = QoS::default_profile());
    template <typename M>
    Result create_subscription(DeclaredEntity& out, const char* topic_name,
                               const DeclaredCallback& callback,
                               const QoS& qos = QoS::default_profile());
    template <typename S>
    Result create_service_server(DeclaredEntity& out, const char* service_name,
                                 const DeclaredCallback& callback,
                                 const QoS& qos = QoS::default_profile());
    template <typename S>
    Result create_service_client(DeclaredEntity& out, const char* service_name,
                                 const QoS& qos = QoS::default_profile());
    template <typename S>
    Result create_service_client(DeclaredEntity& out, const char* service_name,
                                 const DeclaredCallback& callback,
                                 const QoS& qos = QoS::default_profile());
    template <typename A>
    Result create_action_server(DeclaredEntity& out, const char* action_name,
                                const DeclaredCallback& callback,
                                const QoS& qos = QoS::default_profile());
    template <typename A>
    Result create_action_client(DeclaredEntity& out, const char* action_name,
                                const QoS& qos = QoS::default_profile());
    template <typename A>
    Result create_action_client(DeclaredEntity& out, const char* action_name,
                                const DeclaredCallback& callback,
                                const QoS& qos = QoS::default_profile());

  private:
    Result create_entity_raw(const NodeEntityDescriptor& descriptor);
    Result create_entity_raw(const char* stable_id, NodeEntityKind kind, const char* source_name,
                             const char* type_name = "", const char* type_hash = "",
                             const char* callback_id = nullptr);

    NodeContext* context_;
    const char* stable_id_;
    void* runtime_handle_;
};

} // namespace nros

#endif // NROS_CPP_COMPONENT_NODE_HPP
