// nros-cpp: the dispatch service server's handle
// Freestanding C++ — no exceptions, no STL required

/**
 * @file service_handle.hpp
 * @ingroup grp_service
 * @brief `nros::ServiceHandle<S>` — what `Service<S>::SharedPtr` is.
 */

#ifndef NROS_CPP_SERVICE_HANDLE_HPP
#define NROS_CPP_SERVICE_HANDLE_HPP

#include <cstddef>

/// `rclcpp::Service<S>` is what this handle REFERS to, and `element_type` below
/// names it. Declared rather than included: `nros/service.hpp` includes THIS
/// header (the nested `SharedPtr` alias needs the type), and an alias member is
/// satisfied by an incomplete type.
namespace rclcpp {
template <typename S> class Service;
}

namespace nros {

/// A registered dispatch service server — phase-456 W5.
///
/// WHAT IT IS, AND WHY IT IS NOT A POINTER TO A `Service<S>`
///
/// A service created with a HANDLER is owned by the Rust executor arena:
/// `nros_cpp_service_server_register` returns a handle id, the arena holds the
/// `RmwServiceServer`, and since phase-456 W3 it holds the user's handler as
/// its own trampoline context. Nothing of the caller's is referenced after
/// registration, so there is no C++ object to point at and none is made.
///
/// WHAT THE CORPUS DOES WITH ONE — the measurement that decided the shape
///
/// W3 ran the same census W2 ran for subscriptions, across `examples/`,
/// `tests/`, `book/` and `packages/`. On a DISPATCH service, at every site,
/// out-ref and `::SharedPtr` alike: **nothing is invoked**. It is stored and
/// dropped. That is a keep-alive, and a keep-alive is two words.
///
/// The taking API that used to ride along on the same class — `take_request`,
/// `send_response` and their deprecated spellings — belongs to the POLL server,
/// which owns its own `RmwServiceServer` in caller storage. It is
/// `nros::PollService<S>` now (`nros/polling_service.hpp`), for the reason W2b
/// split `nros::PollSubscription<M>` out: one class serving two ownership
/// models behind a `callback_mode_` flag offers every caller the other one's
/// method set, and on the dispatch path those methods read storage the arena
/// never filled.
///
/// WHAT IT DELIBERATELY DOES NOT HAVE
///
/// No `operator->`, because there is nothing to dereference. A ported file that
/// calls a method on its service handle gets a compile error naming the handle,
/// which is the mechanical edit RFC-0089 asks for.
///
/// No unregister, no `cancel()`. The executor arena has no removal path — the
/// registration lives as long as the executor does. Offering a verb that cannot
/// be implemented is the defect this type exists to remove, so it is not
/// offered.
template <typename S> class ServiceHandle {
  public:
    /// The service type, for the same reason `std::shared_ptr` exposes one.
    using service_type = S;
    /// What this handle refers to. A NAME, not a dereference — there is still
    /// no `operator->`, and no C++ object at the other end.
    using element_type = ::rclcpp::Service<S>;

    constexpr ServiceHandle() : executor_(nullptr), handle_id_(0) {}
    /// Null, spelled the way a ported file spells it
    /// (`Service<S>::SharedPtr srv_ = nullptr;`).
    constexpr ServiceHandle(decltype(nullptr)) : executor_(nullptr), handle_id_(0) {}
    constexpr ServiceHandle(void* executor, size_t handle_id)
        : executor_(executor), handle_id_(handle_id) {}

    /// `if (srv_)` / `if (!srv_)`. A default-constructed handle is empty; one
    /// from a successful `create_service` never is.
    explicit constexpr operator bool() const { return executor_ != nullptr; }

    /// The executor arena slot this registration occupies. Present for
    /// introspection and for tests that assert a registration happened; there
    /// is nothing to do with it through this API today.
    constexpr size_t handle_id() const { return handle_id_; }

    /// Stop referring to the registration. Does NOT unregister it — the arena
    /// has no removal path, and the handler goes on being dispatched. Present
    /// because ported code writes `srv_.reset()` meaning "I am done with this
    /// handle", and that is exactly what this does.
    void reset() {
        executor_ = nullptr;
        handle_id_ = 0;
    }

  private:
    void* executor_;
    size_t handle_id_;
};

template <typename S>
constexpr bool operator==(const ServiceHandle<S>& a, const ServiceHandle<S>& b) {
    return a.handle_id() == b.handle_id() && static_cast<bool>(a) == static_cast<bool>(b);
}
template <typename S>
constexpr bool operator!=(const ServiceHandle<S>& a, const ServiceHandle<S>& b) {
    return !(a == b);
}
template <typename S> constexpr bool operator==(const ServiceHandle<S>& a, decltype(nullptr)) {
    return !static_cast<bool>(a);
}
template <typename S> constexpr bool operator!=(const ServiceHandle<S>& a, decltype(nullptr)) {
    return static_cast<bool>(a);
}
template <typename S> constexpr bool operator==(decltype(nullptr), const ServiceHandle<S>& a) {
    return !static_cast<bool>(a);
}
template <typename S> constexpr bool operator!=(decltype(nullptr), const ServiceHandle<S>& a) {
    return static_cast<bool>(a);
}

} // namespace nros

#endif // NROS_CPP_SERVICE_HANDLE_HPP
