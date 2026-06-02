// nros-cpp: tick-time context for executable components
// Freestanding C++ — no exceptions, no STL required

/**
 * @file tick_ctx.hpp
 * @ingroup grp_node
 * @brief `nros::TickCtx` — client-side dispatch from a component's `tick`
 *        body.
 *
 * Phase 212.M-F.4.c — mirror of the Rust substrate's `TickCtx` client-side
 * seams added in Phase 212.M-F.4 (`d15565efe`). A component's per-spin
 * `tick` hook runs between callback dispatch, where the executor is free,
 * so it is the only place a component can drive service-client `call`s or
 * action-client `send_goal`s (both need `&mut Executor` which a mid-spin
 * callback can't hold).
 *
 * The two raw FFI entry points (`nros_cpp_tick_ctx_call_raw` /
 * `nros_cpp_tick_ctx_send_goal_raw`) live in the C-ABI surface; this
 * header wraps them in a typed `nros::TickCtx` so user `tick` bodies can
 * write:
 *
 * ```cpp
 * void tick(TickCtx& ctx) {
 *     MyReq req{...};
 *     MyResp resp{};
 *     auto r = ctx.call<MyReq, MyResp>("my_service", req, resp);
 *     // ...
 *     uint8_t goal_id[16] = {};
 *     auto g = ctx.send_goal<MyGoal>("my_action", goal, goal_id);
 * }
 * ```
 *
 * ## Stub contract (until M-F.4.a)
 *
 * Until the codegen-side `GenClientDispatch` impl lands in `nros-cli`
 * (Phase 212.M-F.4.a) and the generated runtime starts passing a real
 * non-null tick-ctx handle, both raw FFI symbols return
 * `NROS_CPP_RET_ERROR` (matching the Rust `UnsupportedClients` stub in
 * `nros-node`'s `ExecutorComponentRuntime::run_ticks`). The symbols
 * exist + are callable; C++ user code can write tick bodies against the
 * typed wrappers today, and the runtime will surface `ErrorCode::Error`
 * until the generated dispatch reaches them.
 */

#ifndef NROS_CPP_TICK_CTX_HPP
#define NROS_CPP_TICK_CTX_HPP

#include <cstddef>
#include <cstdint>
#include <string.h>

#include "nros/result.hpp"
// Phase 118.D — cbindgen output is the canonical FFI surface.
// `nros_cpp_tick_ctx_call_raw` / `_send_goal_raw` are declared in
// nros_cpp_ffi.h; we just pull them in here. No hand-written
// `extern "C"` redeclaration block (that pattern bit-rots).
#include "nros_cpp_ffi.h"

namespace nros {

/// Client-side dispatch handle handed to `ExecutableComponent::tick`.
///
/// Mirrors the Rust `nros::TickCtx` substrate (Phase 212.M-F.4). The
/// generated runtime constructs one per spin and hands it to the
/// component's `tick` body via an opaque per-tick context pointer
/// (`handle()`); the typed `call<Req, Resp>()` / `send_goal<G>()`
/// wrappers route through the raw FFI to the runtime's
/// `ClientDispatch` impl.
///
/// Component code never sees the executor directly — the substrate
/// keeps that detail inside the runtime. Until the codegen-side
/// `GenClientDispatch` impl lands (M-F.4.a) every call returns
/// `ErrorCode::Error`; the surface is stable so user `tick` bodies can
/// be written against it today.
class TickCtx {
  public:
    /// Default-construct an empty tick context. Calls return
    /// `ErrorCode::Error` (no backend handle). Useful for forward-compat
    /// smoke tests + the symbol-exists contract.
    constexpr TickCtx() : handle_(nullptr) {}

    /// Construct from an opaque per-tick context handle. The generated
    /// runtime calls this once per spin and forwards `*this` to the
    /// component's `tick` body.
    explicit constexpr TickCtx(void* handle) : handle_(handle) {}

    /// Opaque per-tick context handle. Pass-through for FFI users; not
    /// intended for component code.
    void* handle() const { return handle_; }

    /// Issue a raw-CDR service-client request and block on the reply.
    /// Mirrors `nros::TickCtx::call_raw` (Rust).
    ///
    /// @param service_entity   Stable entity id of the service client
    ///                         (NUL-terminated string).
    /// @param request_cdr      Request CDR bytes (already framed).
    /// @param request_len      Request byte length.
    /// @param response_buf     Caller-owned reply buffer.
    /// @param response_buf_cap Reply buffer capacity in bytes.
    /// @param response_len_out Out-param: response length on success.
    /// @return `Ok` on success, error code otherwise.
    Result call_raw(const char* service_entity, const uint8_t* request_cdr, size_t request_len,
                    uint8_t* response_buf, size_t response_buf_cap, size_t* response_len_out) {
        if (!service_entity) return Result(ErrorCode::InvalidArgument);
        size_t entity_len = ::strlen(service_entity);
        return Result(nros_cpp_tick_ctx_call_raw(
            handle_, reinterpret_cast<const uint8_t*>(service_entity), entity_len, request_cdr,
            request_len, response_buf, response_buf_cap, response_len_out));
    }

    /// Issue a typed service-client request and decode the reply.
    /// Mirrors `nros::TickCtx::call<Req, Resp, REQ_N, RESP_N>` (Rust).
    ///
    /// `Req` / `Resp` must provide `SERIALIZED_SIZE_MAX`, `ffi_serialize`,
    /// and `ffi_deserialize` (the codegen-emitted interface, same as
    /// `nros::Client<S>`).
    template <typename Req, typename Resp>
    Result call(const char* service_entity, const Req& request, Resp& response) {
        uint8_t req_buf[Req::SERIALIZED_SIZE_MAX];
        size_t req_len = 0;
        if (Req::ffi_serialize(&request, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Result(ErrorCode::Error);
        }

        uint8_t resp_buf[Resp::SERIALIZED_SIZE_MAX];
        size_t resp_len = 0;
        Result r =
            call_raw(service_entity, req_buf, req_len, resp_buf, sizeof(resp_buf), &resp_len);
        if (!r.ok()) return r;

        if (Resp::ffi_deserialize(resp_buf, resp_len, &response) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result::success();
    }

    /// Kick a raw-CDR action-client goal. Mirrors
    /// `nros::TickCtx::send_goal_raw` (Rust). The 16-byte goal id is
    /// stamped by the server-side accept; result + feedback streams
    /// arrive via callback dispatch — not this method.
    ///
    /// @param action_entity Stable entity id of the action client.
    /// @param goal_cdr      Goal CDR bytes.
    /// @param goal_len      Goal byte length.
    /// @param goal_id_out   16-byte buffer; receives the assigned goal id.
    /// @return `Ok` on success, error code otherwise.
    Result send_goal_raw(const char* action_entity, const uint8_t* goal_cdr, size_t goal_len,
                         uint8_t goal_id_out[16]) {
        if (!action_entity) return Result(ErrorCode::InvalidArgument);
        size_t entity_len = ::strlen(action_entity);
        return Result(nros_cpp_tick_ctx_send_goal_raw(
            handle_, reinterpret_cast<const uint8_t*>(action_entity), entity_len, goal_cdr,
            goal_len, reinterpret_cast<uint8_t (*)[16]>(goal_id_out)));
    }

    /// Kick a typed action-client goal. Mirrors
    /// `nros::TickCtx::send_goal<G, N>` (Rust).
    ///
    /// `G` must provide `SERIALIZED_SIZE_MAX` and `ffi_serialize` (the
    /// codegen-emitted interface, same as `nros::ActionClient<A>`).
    template <typename G>
    Result send_goal(const char* action_entity, const G& goal, uint8_t goal_id_out[16]) {
        uint8_t goal_buf[G::SERIALIZED_SIZE_MAX];
        size_t goal_len = 0;
        if (G::ffi_serialize(&goal, goal_buf, sizeof(goal_buf), &goal_len) != 0) {
            return Result(ErrorCode::Error);
        }
        return send_goal_raw(action_entity, goal_buf, goal_len, goal_id_out);
    }

  private:
    void* handle_;
};

} // namespace nros

#endif // NROS_CPP_TICK_CTX_HPP
