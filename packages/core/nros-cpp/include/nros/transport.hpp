// nros-cpp: runtime-pluggable custom transport (Phase 115.D)
// Freestanding C++ — no STL required

/**
 * @file transport.hpp
 * @ingroup grp_transport
 * @brief `nros::TransportOps` — register a custom transport at runtime.
 */

#ifndef NROS_CPP_TRANSPORT_HPP
#define NROS_CPP_TRANSPORT_HPP

#include <cstddef>
#include <cstdint>

#include "nros/result.hpp"

extern "C" {
// Mirrors `nros_transport_ops_t` from <nros/nros_generated.h>. Kept
// inline here so user code only needs `#include <nros/transport.hpp>`.
typedef int nros_cpp_transport_ret_t;
struct nros_cpp_transport_ops_t {
    void* user_data;
    nros_cpp_transport_ret_t (*open)(void* user_data, const void* params);
    void (*close)(void* user_data);
    nros_cpp_transport_ret_t (*write)(void* user_data, const std::uint8_t* buf, std::size_t len);
    std::int32_t (*read)(void* user_data, std::uint8_t* buf, std::size_t len,
                         std::uint32_t timeout_ms);
};

/// Phase 115.D — Rust-side entry that copies the vtable into
/// `nros_rmw::set_custom_transport`. Implemented in `transport.rs`.
nros_cpp_transport_ret_t nros_cpp_set_custom_transport(const nros_cpp_transport_ops_t* ops);
nros_cpp_transport_ret_t nros_cpp_clear_custom_transport();
nros_cpp_transport_ret_t nros_cpp_has_custom_transport();
} // extern "C"

namespace nros {

/// Phase 115.D — wraps the four C function pointers into a typed
/// builder-style struct that mirrors `rmw_uros_set_custom_transport`'s
/// rclcpp-friendly shape.
///
/// Usage:
/// ```cpp
/// nros::TransportOps ops;
/// ops.user_data = &my_uart;
/// ops.open  = [](void* ctx, const void*) -> int {
///     reinterpret_cast<MyUart*>(ctx)->open(); return 0;
/// };
/// ops.close = [](void* ctx) { reinterpret_cast<MyUart*>(ctx)->close(); };
/// ops.write = [](void* ctx, const uint8_t* buf, std::size_t len) -> int {
///     return reinterpret_cast<MyUart*>(ctx)->write(buf, len);
/// };
/// ops.read  = [](void* ctx, uint8_t* buf, std::size_t len, uint32_t to) -> std::int32_t {
///     return reinterpret_cast<MyUart*>(ctx)->read(buf, len, to);
/// };
/// nros::set_custom_transport(ops);
/// ```
///
/// Lambdas with captures are NOT allowed — the four fields are raw C
/// function pointers. Pass per-instance state via `user_data`.
struct TransportOps {
    void* user_data = nullptr;
    nros_cpp_transport_ret_t (*open)(void* user_data, const void* params) = nullptr;
    void (*close)(void* user_data) = nullptr;
    nros_cpp_transport_ret_t (*write)(void* user_data, const std::uint8_t* buf,
                                      std::size_t len) = nullptr;
    std::int32_t (*read)(void* user_data, std::uint8_t* buf, std::size_t len,
                         std::uint32_t timeout_ms) = nullptr;
};

/// Phase 115.D — register a custom transport. Must be called before
/// the first `nros::Executor::open()`. Subsequent calls overwrite
/// the slot.
///
/// Returns `Result(0)` on success, `Result(NROS_CPP_RET_INVALID_ARGUMENT)`
/// if any of the four function pointers in `ops` is null.
inline Result set_custom_transport(const TransportOps& ops) {
    if (ops.open == nullptr || ops.close == nullptr || ops.write == nullptr ||
        ops.read == nullptr) {
        return Result(ErrorCode::InvalidArgument);
    }
    nros_cpp_transport_ops_t ffi{};
    ffi.user_data = ops.user_data;
    ffi.open = ops.open;
    ffi.close = ops.close;
    ffi.write = ops.write;
    ffi.read = ops.read;
    return Result(nros_cpp_set_custom_transport(&ffi));
}

/// Phase 115.D — clear any previously-registered transport.
inline Result clear_custom_transport() { return Result(nros_cpp_clear_custom_transport()); }

/// Phase 115.D — `true` if a custom transport is currently registered.
inline bool has_custom_transport() { return nros_cpp_has_custom_transport() == 1; }

} // namespace nros

#endif // NROS_CPP_TRANSPORT_HPP
