// nros-cpp: Node class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_NODE_HPP
#define NROS_CPP_NODE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"
#include "nros/qos.hpp"

// FFI declarations (from nros-cpp-ffi generated header)
extern "C" {

typedef int nros_cpp_ret_t;

struct nros_cpp_node_t {
    void* executor;
    uint8_t name[64];
    uint8_t namespace_[64];
};

nros_cpp_ret_t nros_cpp_init(
    const char* locator,
    uint8_t domain_id,
    const char* node_name,
    const char* ns,
    void** out_handle);

nros_cpp_ret_t nros_cpp_fini(void* handle);

nros_cpp_ret_t nros_cpp_node_create(
    void* executor_handle,
    const char* name,
    const char* ns,
    nros_cpp_node_t* out_node);

nros_cpp_ret_t nros_cpp_node_destroy(nros_cpp_node_t* node);

const char* nros_cpp_node_get_name(const nros_cpp_node_t* node);
const char* nros_cpp_node_get_namespace(const nros_cpp_node_t* node);

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

        nros_cpp_ret_t ret = nros_cpp_node_create(
            out.executor_handle_, name, ns, &out.handle_);

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

    /// Destructor — releases node resources.
    ~Node() {
        if (initialized_) {
            nros_cpp_node_destroy(&handle_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    Node(Node&& other)
        : handle_(other.handle_),
          initialized_(other.initialized_),
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

    friend Result init(const char* locator, uint8_t domain_id);
    friend Result shutdown();
    friend bool ok();
    friend Result create_node(Node& out, const char* name, const char* ns);

    // Store the global executor handle for init/shutdown
    // (In freestanding C++, we use a simple static pointer)
    static void*& global_executor() {
        static void* handle = nullptr;
        return handle;
    }
};

// -- Free function implementations --

inline Result init(const char* locator, uint8_t domain_id) {
    // Default node name for the session
    void* handle = nullptr;
    nros_cpp_ret_t ret = nros_cpp_init(
        locator, domain_id, "nros_cpp", nullptr, &handle);

    if (ret == 0) {
        Node::global_executor() = handle;
    }
    return Result(ret);
}

inline Result shutdown() {
    void*& handle = Node::global_executor();
    if (!handle) {
        return Result::success();
    }
    nros_cpp_ret_t ret = nros_cpp_fini(handle);
    handle = nullptr;
    return Result(ret);
}

/// Check if the nros session is initialized.
inline bool ok() {
    return Node::global_executor() != nullptr;
}

/// Create a node (convenience — uses the global executor).
///
/// This is the primary way to create nodes after calling `nros::init()`.
///
/// @param out   Receives the initialized node.
/// @param name  Node name.
/// @param ns    Node namespace, or nullptr for "/".
inline Result create_node(Node& out, const char* name, const char* ns = nullptr) {
    out.executor_handle_ = Node::global_executor();
    if (!out.executor_handle_) {
        return Result(ErrorCode::NotInitialized);
    }
    return Node::create(out, name, ns);
}

} // namespace nros

#endif // NROS_CPP_NODE_HPP
