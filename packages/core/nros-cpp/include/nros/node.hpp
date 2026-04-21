// nros-cpp: Node class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_NODE_HPP
#define NROS_CPP_NODE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"
#include "nros/nros_cpp_config_generated.h"
#include "nros/qos.hpp"
#include "nros/publisher.hpp"
#include "nros/subscription.hpp"
#include "nros/service.hpp"
#include "nros/client.hpp"
#include "nros/action_server.hpp"
#include "nros/action_client.hpp"
#include "nros/timer.hpp"
#include "nros/guard_condition.hpp"
#include "nros/executor.hpp"

// FFI declarations (from nros-cpp generated header)
extern "C" {

typedef int nros_cpp_ret_t;

enum nros_cpp_qos_reliability_t {
    NROS_CPP_QOS_RELIABLE = 0,
    NROS_CPP_QOS_BEST_EFFORT = 1,
};

enum nros_cpp_qos_durability_t {
    NROS_CPP_QOS_VOLATILE = 0,
    NROS_CPP_QOS_TRANSIENT_LOCAL = 1,
};

enum nros_cpp_qos_history_t {
    NROS_CPP_QOS_KEEP_LAST = 0,
    NROS_CPP_QOS_KEEP_ALL = 1,
};

struct nros_cpp_qos_t {
    nros_cpp_qos_reliability_t reliability;
    nros_cpp_qos_durability_t durability;
    nros_cpp_qos_history_t history;
    int depth;
};

struct nros_cpp_node_t {
    void* executor;
    uint8_t name[64];
    uint8_t namespace_[64];
};

nros_cpp_ret_t nros_cpp_init(const char* locator, uint8_t domain_id, const char* node_name,
                             const char* ns, void* storage);

nros_cpp_ret_t nros_cpp_fini(void* storage);

nros_cpp_ret_t nros_cpp_node_create(void* executor_handle, const char* name, const char* ns,
                                    nros_cpp_node_t* out_node);

nros_cpp_ret_t nros_cpp_node_destroy(nros_cpp_node_t* node);

const char* nros_cpp_node_get_name(const nros_cpp_node_t* node);
const char* nros_cpp_node_get_namespace(const nros_cpp_node_t* node);

nros_cpp_ret_t nros_cpp_publisher_create(const nros_cpp_node_t* node, const char* topic,
                                         const char* type_name, const char* type_hash,
                                         nros_cpp_qos_t qos, void* storage);

nros_cpp_ret_t nros_cpp_subscription_create(const nros_cpp_node_t* node, const char* topic,
                                            const char* type_name, const char* type_hash,
                                            nros_cpp_qos_t qos, void* storage);

nros_cpp_ret_t nros_cpp_service_server_create(const nros_cpp_node_t* node, const char* service_name,
                                              const char* type_name, const char* type_hash,
                                              nros_cpp_qos_t qos, void* storage);

nros_cpp_ret_t nros_cpp_service_client_create(const nros_cpp_node_t* node, const char* service_name,
                                              const char* type_name, const char* type_hash,
                                              nros_cpp_qos_t qos, void* storage);

nros_cpp_ret_t nros_cpp_action_server_create(const nros_cpp_node_t* node, const char* action_name,
                                             const char* type_name, const char* type_hash,
                                             nros_cpp_qos_t qos, void* storage);
nros_cpp_ret_t nros_cpp_action_server_register(void* storage, void* executor_handle);

nros_cpp_ret_t nros_cpp_action_client_create(const nros_cpp_node_t* node, const char* action_name,
                                             const char* type_name, const char* type_hash,
                                             nros_cpp_qos_t qos, void* storage);

nros_cpp_ret_t nros_cpp_spin_once(void* handle, int32_t timeout_ms);

} // extern "C"

namespace nros {

/// Initialize an nros session.
///
/// Opens a middleware connection. Must be called before creating nodes.
/// Call `shutdown()` to clean up.
///
/// @param locator  Middleware locator (e.g., "tcp/127.0.0.1:7447"), or nullptr for default.
/// @param domain_id  ROS domain ID (0-232).
/// @return Result indicating success or failure.
inline Result init(const char* locator = nullptr, uint8_t domain_id = 0);

/// Shut down the nros session.
///
/// Closes the middleware connection and frees all resources.
inline Result shutdown();

/// Node — the primary interface for creating ROS entities.
///
/// Mirrors `rclcpp::Node`. Entities (publishers, subscriptions, services,
/// etc.) are created through the node. The node holds a reference to the
/// parent executor session.
///
/// Usage:
/// ```cpp
/// nros::Node node;
/// NROS_TRY(nros::Node::create(node, "my_node"));
/// ```
class Node {
  public:
    /// Default constructor — creates an uninitialized node.
    Node() : handle_(), initialized_(false), executor_handle_(nullptr) {}

    /// Create a new node.
    ///
    /// @param out   Receives the initialized node.
    /// @param name  Node name (null-terminated).
    /// @param ns    Node namespace (null-terminated), or nullptr for "/".
    /// @return Result indicating success or failure.
    static Result create(Node& out, const char* name, const char* ns = nullptr) {
        if (!out.executor_handle_) {
            return Result(ErrorCode::NotInitialized);
        }

        nros_cpp_ret_t ret = nros_cpp_node_create(out.executor_handle_, name, ns, &out.handle_);

        if (ret == 0) {
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Get the node name.
    const char* get_name() const {
        if (!initialized_) return "";
        return nros_cpp_node_get_name(&handle_);
    }

    /// Get the node namespace.
    const char* get_namespace() const {
        if (!initialized_) return "";
        return nros_cpp_node_get_namespace(&handle_);
    }

    /// Check if the node is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Create a publisher for a topic.
    ///
    /// @tparam M  Message type (must define TYPE_NAME and TYPE_HASH).
    /// @param out    Receives the initialized publisher.
    /// @param topic  Topic name (null-terminated).
    /// @param qos    QoS profile (default: reliable, keep-last(10)).
    template <typename M>
    Result create_publisher(Publisher<M>& out, const char* topic,
                            const QoS& qos = QoS::default_profile()) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_qos_t ffi_qos;
        ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
        ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
        ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
        ffi_qos.depth = qos.depth();
        nros_cpp_ret_t ret = nros_cpp_publisher_create(&handle_, topic, M::TYPE_NAME, M::TYPE_HASH,
                                                       ffi_qos, out.storage_);
        if (ret == 0) {
            // Phase 87.6: topic name lives C++-side now (was inside the
            // deleted `CppPublisher` Rust wrapper). Copy + null-terminate
            // into the fixed-size buffer; truncation is silent.
            size_t topic_len = 0;
            while (topic[topic_len] != '\0' &&
                   topic_len + 1 < sizeof(out.topic_name_)) {
                out.topic_name_[topic_len] = topic[topic_len];
                ++topic_len;
            }
            out.topic_name_[topic_len] = '\0';
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a subscription for a topic.
    ///
    /// @tparam M  Message type (must define TYPE_NAME and TYPE_HASH).
    /// @param out    Receives the initialized subscription.
    /// @param topic  Topic name (null-terminated).
    /// @param qos    QoS profile (default: reliable, keep-last(10)).
    template <typename M>
    Result create_subscription(Subscription<M>& out, const char* topic,
                               const QoS& qos = QoS::default_profile()) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_qos_t ffi_qos;
        ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
        ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
        ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
        ffi_qos.depth = qos.depth();
        nros_cpp_ret_t ret = nros_cpp_subscription_create(&handle_, topic, M::TYPE_NAME,
                                                          M::TYPE_HASH, ffi_qos, out.storage_);
        if (ret == 0) {
            // Phase 87.6: topic name lives C++-side now.
            size_t topic_len = 0;
            while (topic[topic_len] != '\0' &&
                   topic_len + 1 < sizeof(out.topic_name_)) {
                out.topic_name_[topic_len] = topic[topic_len];
                ++topic_len;
            }
            out.topic_name_[topic_len] = '\0';
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a service server.
    ///
    /// @tparam S  Service type (must define nested Request and Response with TYPE_NAME/TYPE_HASH).
    /// @param out           Receives the initialized service server.
    /// @param service_name  Service name (null-terminated).
    /// @param qos           QoS profile (default: services preset).
    template <typename S>
    Result create_service(Service<S>& out, const char* service_name,
                          const QoS& qos = QoS::services()) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_qos_t ffi_qos;
        ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
        ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
        ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
        ffi_qos.depth = qos.depth();
        nros_cpp_ret_t ret = nros_cpp_service_server_create(
            &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos, out.storage_);
        if (ret == 0) {
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a service client.
    ///
    /// @tparam S  Service type (must define nested Request and Response with TYPE_NAME/TYPE_HASH).
    /// @param out           Receives the initialized service client.
    /// @param service_name  Service name (null-terminated).
    /// @param qos           QoS profile (default: services preset).
    template <typename S>
    Result create_client(Client<S>& out, const char* service_name,
                         const QoS& qos = QoS::services()) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_qos_t ffi_qos;
        ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
        ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
        ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
        ffi_qos.depth = qos.depth();
        nros_cpp_ret_t ret = nros_cpp_service_client_create(
            &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos, out.storage_);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create an action server.
    ///
    /// Goals are auto-accepted during spin_once(). Use try_recv_goal() to poll.
    ///
    /// @tparam A  Action type (must define nested Goal, Result, Feedback with TYPE_NAME/TYPE_HASH).
    /// @param out          Receives the initialized action server.
    /// @param action_name  Action name (null-terminated).
    /// @param qos          QoS profile (default: services preset).
    template <typename A>
    Result create_action_server(ActionServer<A>& out, const char* action_name,
                                const QoS& qos = QoS::services()) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_qos_t ffi_qos;
        ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
        ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
        ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
        ffi_qos.depth = qos.depth();
        nros_cpp_ret_t ret = nros_cpp_action_server_create(
            &handle_, action_name, A::TYPE_NAME, A::Goal::TYPE_HASH, ffi_qos, out.storage_);
        if (ret != 0) return Result(ret);
        // Register with executor — creates transport handles (3 queryables + 2 publishers).
        // Deferred from create to avoid FreeRTOS QEMU deadlocks.
        ret = nros_cpp_action_server_register(out.storage_, executor_handle_);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create an action client.
    ///
    /// @tparam A  Action type (must define nested Goal, Result, Feedback with TYPE_NAME/TYPE_HASH).
    /// @param out          Receives the initialized action client.
    /// @param action_name  Action name (null-terminated).
    /// @param qos          QoS profile (default: services preset).
    template <typename A>
    Result create_action_client(ActionClient<A>& out, const char* action_name,
                                const QoS& qos = QoS::services()) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_qos_t ffi_qos;
        ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
        ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
        ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
        ffi_qos.depth = qos.depth();
        nros_cpp_ret_t ret = nros_cpp_action_client_create(
            &handle_, action_name, A::TYPE_NAME, A::Goal::TYPE_HASH, ffi_qos, out.storage_);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a repeating timer.
    ///
    /// The callback fires during `spin_once()` at the specified period.
    ///
    /// @param out        Receives the initialized timer.
    /// @param period_ms  Timer period in milliseconds.
    /// @param callback   C function pointer invoked on each tick.
    /// @param context    User context passed to the callback (may be nullptr).
    Result create_timer(Timer& out, uint64_t period_ms, nros_cpp_timer_callback_t callback,
                        void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        size_t handle_id = 0;
        nros_cpp_ret_t ret =
            nros_cpp_timer_create(executor_handle_, period_ms, callback, context, &handle_id);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.handle_id_ = handle_id;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a one-shot timer.
    ///
    /// The callback fires once after the specified delay.
    ///
    /// @param out       Receives the initialized timer.
    /// @param delay_ms  Delay in milliseconds before the callback fires.
    /// @param callback  C function pointer invoked once.
    /// @param context   User context passed to the callback (may be nullptr).
    Result create_timer_oneshot(Timer& out, uint64_t delay_ms, nros_cpp_timer_callback_t callback,
                                void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        size_t handle_id = 0;
        nros_cpp_ret_t ret = nros_cpp_timer_create_oneshot(executor_handle_, delay_ms, callback,
                                                           context, &handle_id);
        if (ret == 0) {
            out.executor_ = executor_handle_;
            out.handle_id_ = handle_id;
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Create a guard condition for cross-thread signaling.
    ///
    /// The callback fires during `spin_once()` when `guard.trigger()` is called.
    ///
    /// @param out       Receives the initialized guard condition.
    /// @param callback  C function pointer invoked when triggered.
    /// @param context   User context passed to the callback (may be nullptr).
    Result create_guard_condition(GuardCondition& out, nros_cpp_guard_callback_t callback,
                                  void* context = nullptr) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        nros_cpp_ret_t ret =
            nros_cpp_guard_condition_create(executor_handle_, callback, context, out.storage_);
        if (ret == 0) {
            out.initialized_ = true;
        }
        return Result(ret);
    }

    /// Destructor — releases node resources.
    ~Node() {
        if (initialized_) {
            nros_cpp_node_destroy(&handle_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    Node(Node&& other)
        : handle_(other.handle_), initialized_(other.initialized_),
          executor_handle_(other.executor_handle_) {
        other.initialized_ = false;
        other.executor_handle_ = nullptr;
    }

    Node& operator=(Node&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_node_destroy(&handle_);
            }
            handle_ = other.handle_;
            initialized_ = other.initialized_;
            executor_handle_ = other.executor_handle_;
            other.initialized_ = false;
            other.executor_handle_ = nullptr;
        }
        return *this;
    }

  private:
    Node(const Node&) = delete;
    Node& operator=(const Node&) = delete;

    nros_cpp_node_t handle_;
    bool initialized_;
    void* executor_handle_; // Set by nros::init() via friendship

    friend class Executor;
    friend Result init(const char* locator, uint8_t domain_id);
    friend Result shutdown();
    friend bool ok();
    friend Result create_node(Node& out, const char* name, const char* ns);
    friend Result spin_once(int32_t timeout_ms);
    friend Result spin(uint32_t duration_ms, int32_t poll_ms);
    friend void* global_handle();

    // Global executor inline storage for init/shutdown free functions.
    static uint8_t* global_storage() {
        alignas(8) static uint8_t storage[NROS_CPP_EXECUTOR_STORAGE_SIZE] = {};
        return storage;
    }
    static bool& global_initialized() {
        static bool init = false;
        return init;
    }
};

// -- Free function implementations --

inline Result init(const char* locator, uint8_t domain_id) {
    nros_cpp_ret_t ret =
        nros_cpp_init(locator, domain_id, "nros_cpp", nullptr, Node::global_storage());
    if (ret == 0) {
        Node::global_initialized() = true;
    }
    return Result(ret);
}

inline Result shutdown() {
    if (!Node::global_initialized()) {
        return Result::success();
    }
    nros_cpp_ret_t ret = nros_cpp_fini(Node::global_storage());
    Node::global_initialized() = false;
    return Result(ret);
}

/// Check if the nros session is initialized.
inline bool ok() {
    return Node::global_initialized();
}

/// Create a node (convenience — uses the global executor).
///
/// This is the primary way to create nodes after calling `nros::init()`.
///
/// @param out   Receives the initialized node.
/// @param name  Node name.
/// @param ns    Node namespace, or nullptr for "/".
inline Result create_node(Node& out, const char* name, const char* ns = nullptr) {
    if (!Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    out.executor_handle_ = Node::global_storage();
    return Node::create(out, name, ns);
}

// -- Executor::create_node implementation (requires full Node definition) --

inline Result Executor::create_node(Node& out, const char* name, const char* ns) {
    if (!initialized_) return Result(ErrorCode::NotInitialized);
    out.executor_handle_ = storage_;
    return Node::create(out, name, ns);
}

} // namespace nros

#endif // NROS_CPP_NODE_HPP
