// nros-cpp: Parameter server wrapper
// Freestanding C++ — no exceptions, no STL required, no heap

/**
 * @file parameter.hpp
 * @ingroup grp_parameter
 * @brief `nros::ParameterServer<Cap>` — node-local typed parameter store.
 *
 * Wraps the C `nros_param_server_t` (see `nros/parameter.h`) with a
 * fixed-capacity storage array and rclcpp-shape `declare_parameter<T>` /
 * `get_parameter<T>` / `set_parameter<T>` template methods.
 *
 * The server is purely local — no ROS 2 service exposure. For
 * service-backed parameters, use the `param-services` Cargo feature on
 * `nros-c` and the `nros_executor_*_param_*` C functions directly.
 */

#ifndef NROS_CPP_PARAMETER_HPP
#define NROS_CPP_PARAMETER_HPP

#include <cstddef>
#include <cstdint>

#include "nros/result.hpp"

extern "C" {
#include "nros/parameter.h"
}

namespace nros {

/// Fixed-capacity, node-local typed parameter server.
///
/// Capacity is compile-time; storage lives inline. No heap allocation.
/// Bool / int64 / double / string types supported in v1 (matches the
/// rclcpp `declare_parameter<T>` shape used by Autoware safety-island).
///
/// Strings are copied into a 128-byte slot inside the server; callers do
/// not need to keep the source buffer alive past the call.
///
/// Usage:
/// ```cpp
/// nros::ParameterServer<16> params;
/// NROS_TRY(params.declare_parameter<double>("ctrl_period", 0.15));
/// double v = 0.0;
/// NROS_TRY(params.get_parameter<double>("ctrl_period", v));
/// ```
template <std::size_t Capacity> class ParameterServer {
  public:
    ParameterServer() : server_(nros_param_server_get_zero_initialized()) {
        nros_param_server_init(&server_, storage_, Capacity);
    }

    ~ParameterServer() { nros_param_server_fini(&server_); }

    ParameterServer(const ParameterServer&) = delete;
    ParameterServer& operator=(const ParameterServer&) = delete;
    ParameterServer(ParameterServer&&) = delete;
    ParameterServer& operator=(ParameterServer&&) = delete;

    /// Declare a parameter with a default value.
    ///
    /// @tparam T  bool, int64_t, double, or const char*.
    /// @param name           Parameter name (null-terminated).
    /// @param default_value  Default value used until overridden.
    template <typename T> Result declare_parameter(const char* name, T default_value) {
        return declare_impl(name, default_value);
    }

    /// Get a parameter value.
    ///
    /// @tparam T  bool, int64_t, or double.
    /// @param name  Parameter name.
    /// @param out   Receives the value on success.
    /// @retval ErrorCode::Ok       on success.
    /// @retval Other code (raw)    NROS_RET_NOT_FOUND if undeclared.
    template <typename T> Result get_parameter(const char* name, T& out) const {
        return get_impl(name, out);
    }

    /// Get a string parameter into a caller-provided buffer.
    ///
    /// @param name      Parameter name.
    /// @param out       Output buffer (null-terminated on success).
    /// @param max_len   Buffer capacity in bytes.
    Result get_parameter(const char* name, char* out, std::size_t max_len) const {
        return Result(nros_param_get_string(&server_, name, out, max_len));
    }

    /// Set an existing parameter's value.
    template <typename T> Result set_parameter(const char* name, T value) {
        return set_impl(name, value);
    }

    /// Check whether a parameter has been declared.
    bool has_parameter(const char* name) const { return nros_param_has(&server_, name); }

    /// Number of declared parameters.
    std::size_t parameter_count() const { return nros_param_server_get_count(&server_); }

    /// Get the underlying C server pointer.
    ///
    /// Useful for handing the server to C-API helpers (e.g. ROS 2
    /// service registration when the `param-services` feature is on).
    nros_param_server_t* raw() { return &server_; }
    const nros_param_server_t* raw() const { return &server_; }

  private:
    /* declare overloads dispatch by argument type */
    Result declare_impl(const char* name, bool v) {
        return Result(nros_param_declare_bool(&server_, name, v));
    }
    Result declare_impl(const char* name, int64_t v) {
        return Result(nros_param_declare_integer(&server_, name, v));
    }
    Result declare_impl(const char* name, double v) {
        return Result(nros_param_declare_double(&server_, name, v));
    }
    Result declare_impl(const char* name, const char* v) {
        return Result(nros_param_declare_string(&server_, name, v));
    }
    /* int / uint literals collapse to int64_t */
    Result declare_impl(const char* name, int v) {
        return Result(nros_param_declare_integer(&server_, name, static_cast<int64_t>(v)));
    }

    Result get_impl(const char* name, bool& out) const {
        return Result(nros_param_get_bool(&server_, name, &out));
    }
    Result get_impl(const char* name, int64_t& out) const {
        return Result(nros_param_get_integer(&server_, name, &out));
    }
    Result get_impl(const char* name, double& out) const {
        return Result(nros_param_get_double(&server_, name, &out));
    }

    Result set_impl(const char* name, bool v) {
        return Result(nros_param_set_bool(&server_, name, v));
    }
    Result set_impl(const char* name, int64_t v) {
        return Result(nros_param_set_integer(&server_, name, v));
    }
    Result set_impl(const char* name, double v) {
        return Result(nros_param_set_double(&server_, name, v));
    }
    Result set_impl(const char* name, const char* v) {
        return Result(nros_param_set_string(&server_, name, v));
    }
    Result set_impl(const char* name, int v) {
        return Result(nros_param_set_integer(&server_, name, static_cast<int64_t>(v)));
    }

    nros_param_server_t server_;
    nros_parameter_t storage_[Capacity];
};

} // namespace nros

#endif // NROS_CPP_PARAMETER_HPP
