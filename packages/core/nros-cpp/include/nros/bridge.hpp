#ifndef NROS_BRIDGE_HPP
#define NROS_BRIDGE_HPP

/**
 * @file bridge.hpp
 * @brief C++ multi-RMW bridge surface (Phase 128.F.5).
 *
 * Wraps the C entry points from `<nros/bridge.h>` in RAII-friendly
 * classes. Single-backend code does not need this header.
 *
 * Requires the binary to link `libnros_bridge.a` (Rust bridge crate)
 * in addition to its usual nros-cpp + per-backend libs.
 */

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "nros/bridge.h"
#include "nros/result.hpp"

namespace nros {

/// Mirror of `nros_session_spec_t` — one entry per backend session.
/// Use `SessionSpec("zenoh", "tcp/127.0.0.1:7447")` for the common
/// case; chainable setters cover the rest.
struct SessionSpec {
    std::string rmw;
    std::string locator;
    std::uint32_t domain_id = 0;
    std::string node_name;
    std::string namespace_;

    SessionSpec(std::string rmw_, std::string locator_)
        : rmw(std::move(rmw_)), locator(std::move(locator_)) {}

    SessionSpec &with_domain_id(std::uint32_t id) {
        domain_id = id;
        return *this;
    }
    SessionSpec &with_node_name(std::string name) {
        node_name = std::move(name);
        return *this;
    }
    SessionSpec &with_namespace(std::string ns) {
        namespace_ = std::move(ns);
        return *this;
    }
};

/// RAII handle around `nros_init_multi` / `nros_fini_multi`. Opens
/// the executor on construction; closes on destruction.
class MultiExecutor {
public:
    /// Open the executor against `specs`. Throws nothing; check
    /// `valid()` and `last_ret()` after construction.
    explicit MultiExecutor(const std::vector<SessionSpec> &specs) {
        std::vector<nros_session_spec_t> c_specs;
        c_specs.reserve(specs.size());
        for (auto &s : specs) {
            c_specs.push_back(nros_session_spec_t{
                s.rmw.c_str(),
                s.locator.c_str(),
                s.domain_id,
                s.node_name.empty() ? nullptr : s.node_name.c_str(),
                s.namespace_.empty() ? nullptr : s.namespace_.c_str(),
            });
        }
        last_ret_ = nros_init_multi(c_specs.data(), c_specs.size(), &handle_);
    }

    ~MultiExecutor() {
        if (handle_ != nullptr) {
            nros_fini_multi(handle_);
        }
    }

    MultiExecutor(const MultiExecutor &) = delete;
    MultiExecutor &operator=(const MultiExecutor &) = delete;
    MultiExecutor(MultiExecutor &&other) noexcept
        : handle_(other.handle_), last_ret_(other.last_ret_) {
        other.handle_ = nullptr;
    }
    MultiExecutor &operator=(MultiExecutor &&other) noexcept {
        if (this != &other) {
            if (handle_ != nullptr) {
                nros_fini_multi(handle_);
            }
            handle_ = other.handle_;
            last_ret_ = other.last_ret_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    bool valid() const { return handle_ != nullptr && last_ret_ == NROS_RMW_RET_OK; }
    nros_rmw_ret_t last_ret() const { return last_ret_; }
    nros_executor_handle_t handle() const { return handle_; }

private:
    nros_executor_handle_t handle_ = nullptr;
    nros_rmw_ret_t last_ret_ = NROS_RMW_RET_OK;
};

namespace bridge {

/// Per-pump counters mirroring `nros_pump_stats_t`.
struct PumpStats {
    std::size_t forwarded = 0;
    std::size_t dropped_echo = 0;
};

/// RAII pubsub bridge — forwards raw samples from a source Node
/// topic to a destination Node topic, both opened on the same
/// `MultiExecutor`.
///
/// Construct via the named factory `pubsub_raw(...)` below; this
/// constructor takes ownership of an already-created C handle.
class PubSubBridge {
public:
    explicit PubSubBridge(nros_pubsub_bridge_t handle) : handle_(handle) {}

    ~PubSubBridge() {
        if (handle_ != nullptr) {
            nros_pubsub_bridge_destroy(handle_);
        }
    }

    PubSubBridge(const PubSubBridge &) = delete;
    PubSubBridge &operator=(const PubSubBridge &) = delete;
    PubSubBridge(PubSubBridge &&other) noexcept : handle_(other.handle_) {
        other.handle_ = nullptr;
    }
    PubSubBridge &operator=(PubSubBridge &&other) noexcept {
        if (this != &other) {
            if (handle_ != nullptr) {
                nros_pubsub_bridge_destroy(handle_);
            }
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    bool valid() const { return handle_ != nullptr; }

    /// Drain every queued sample and forward to the destination.
    /// Returns the number actually forwarded (samples dropped by the
    /// dedup window are not counted).
    std::size_t pump() {
        if (handle_ == nullptr) return 0;
        return nros_pubsub_bridge_pump(handle_);
    }

    PumpStats pump_with_stats() {
        if (handle_ == nullptr) return {};
        auto s = nros_pubsub_bridge_pump_with_stats(handle_);
        return PumpStats{s.forwarded, s.dropped_echo};
    }

    nros_pubsub_bridge_t handle() const { return handle_; }

private:
    nros_pubsub_bridge_t handle_ = nullptr;
};

/// Construct a raw pubsub bridge. `origin` enables the dedup window
/// (pass the source backend's RMW name); empty string skips dedup
/// for single-direction bridges.
inline Result<PubSubBridge> pubsub_raw(MultiExecutor &exec,
                                       const std::string &src_node,
                                       const std::string &src_rmw,
                                       const std::string &src_topic,
                                       const std::string &dst_node,
                                       const std::string &dst_rmw,
                                       const std::string &dst_topic,
                                       const std::string &type_name,
                                       const std::string &type_hash,
                                       const std::string &origin) {
    nros_pubsub_bridge_t handle = nullptr;
    nros_rmw_ret_t rc = nros_pubsub_bridge_create(
        exec.handle(),
        src_node.c_str(), src_rmw.c_str(), src_topic.c_str(),
        dst_node.c_str(), dst_rmw.c_str(), dst_topic.c_str(),
        type_name.c_str(), type_hash.c_str(),
        origin.empty() ? nullptr : origin.c_str(),
        &handle);
    if (rc != NROS_RMW_RET_OK) {
        return Result<PubSubBridge>(rc);
    }
    return Result<PubSubBridge>(PubSubBridge(handle));
}

} // namespace bridge
} // namespace nros

#endif // NROS_BRIDGE_HPP
