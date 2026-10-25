// nros-cpp: rclcpp::ParameterEventHandler — react to ANY node's parameter changes
// The handler is hosted (NROS_CPP_STD); a freestanding build refuses constructing one.

/**
 * @file parameter_event_handler.hpp
 * @ingroup grp_params
 * @brief `rclcpp::ParameterEventHandler` (issue 2024). The `rclcpp::Parameter`
 *        it hands its callbacks is `rclcpp/parameter_value.hpp`'s (issue 2056).
 *
 * Every surface PUBLISHES `/parameter_events` since issue 2023. This is the
 * subscribing half: one `rcl_interfaces/msg/ParameterEvent` subscription per
 * handler, created on the node through `Node::create_subscription` (so the
 * entity inventory counts it like any other subscription), and two kinds of
 * callback on it:
 *
 *   - `add_parameter_callback(name, cb, node_name = "")` — `cb(const
 *     rclcpp::Parameter&)` for each new or changed value of ONE parameter of
 *     ONE node (this node when `node_name` is empty; a relative name resolves
 *     against this node's namespace);
 *   - `add_parameter_event_callback(cb)` — `cb(const EventT&)` for every
 *     event, where `EventT` is the type the callback takes: the generated
 *     `rcl_interfaces::msg::ParameterEvent` of the caller's own codegen.
 *
 * Each returns an owning `shared_ptr` handle, as upstream's does; the handler
 * keeps only a weak reference, so dropping the handle unregisters.
 *
 * The frame is decoded here, in the header. Every type EXISTS on every build
 * with one layout (`check-cpp-capability-layout`): the hosted state —
 * `std::string` / `std::vector` values, `std::function` callbacks,
 * `shared_ptr` handles — lives on the heap behind two-pointer members, and only
 * the METHODS that reach it are hosted. A freestanding build refuses
 * constructing a handler.
 */

#ifndef NROS_RCLCPP_PARAMETER_EVENT_HANDLER_HPP
#define NROS_RCLCPP_PARAMETER_EVENT_HANDLER_HPP

#include <cstddef>
#include <cstdint>

// The umbrella first: the handler builds on `Node::create_subscription`, whose
// template body is defined there (`rclcpp/rclcpp.hpp` includes this header
// LAST, so on that road this include is a no-op).
#include "rclcpp/rclcpp.hpp"
#include "rclcpp/detail/std_detect.hpp" // NROS_CPP_HAS_SHARED_PTR, NROS_CPP_STD
#include "rclcpp/logging.hpp"           // NROS_RCLCPP_REFUSE_PARAMETER_EVENT_HANDLER, refuse
#include "rclcpp/node_parameters.hpp"   // rclcpp::ParameterType
#include "rclcpp/parameter_value.hpp"   // rclcpp::Parameter, rclcpp::ParameterValue
#include "rclcpp/qos.hpp"

#ifdef NROS_CPP_STD // hosted-family: container-interop
#include <cstring>
#include <functional>
#include <list>
#include <memory>
#include <string>
#include <type_traits>
#include <vector>
#endif

#ifndef NROS_PARAM_EVENT_RX_BYTES
/// Issue 2024 — the receive buffer of a `ParameterEventHandler`'s
/// `/parameter_events` subscription, in bytes. A frame larger than this (a
/// peer announcing many parameters, or long string arrays, in ONE event) is
/// truncated and therefore dropped. Hosted only, so the default is generous;
/// raise it with `-DNROS_PARAM_EVENT_RX_BYTES=<n>`.
#define NROS_PARAM_EVENT_RX_BYTES 4096
#endif
#if NROS_PARAM_EVENT_RX_BYTES < 1
#error "NROS_PARAM_EVENT_RX_BYTES must be >= 1: it sizes a C array (issue 1015)"
#endif

namespace rclcpp {

namespace detail {

struct ParameterEventDecoder;
struct ParameterEventHandlerAccess;

/// An empty, non-copyable base: what keeps `ParameterEventCallbackHandle`
/// from being copied without declaring a constructor on it.
struct ParameterEventHandleNoCopy {
    ParameterEventHandleNoCopy() = default;
    ParameterEventHandleNoCopy(const ParameterEventHandleNoCopy&) = delete;
    ParameterEventHandleNoCopy& operator=(const ParameterEventHandleNoCopy&) = delete;
};

/// Issue 2024 — the `/parameter_events` subscription's message type: the raw
/// CDR frame. The handler decodes it itself (`ParameterEventDecoder`), and an
/// event callback's own `EventT::ffi_deserialize` reads it again.
struct ParameterEventFrame {
    static constexpr const char* TYPE_NAME = "rcl_interfaces::msg::dds_::ParameterEvent_";
    /// What the executor's own `/parameter_events` publisher announces
    /// (`nros_rcl_interfaces::msg::ParameterEvent::TYPE_HASH`), so the two match.
    static constexpr const char* TYPE_HASH = "TypeHashNotSupported";
    static const size_t SERIALIZED_SIZE_MAX = NROS_PARAM_EVENT_RX_BYTES;
    size_t len = 0;
    uint8_t bytes[NROS_PARAM_EVENT_RX_BYTES];
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t*) { return -1; }
    static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
        ParameterEventFrame* f = static_cast<ParameterEventFrame*>(out);
        if (len > sizeof(f->bytes)) return -1; // truncated: drop, never misread
        for (size_t k = 0; k < len; ++k)
            f->bytes[k] = data[k];
        f->len = len;
        return 0;
    }
};

} // namespace detail

// `rclcpp::Parameter`, what a parameter callback receives, is the ONE
// value object in `rclcpp/parameter_value.hpp` (issue 2056): the node's own
// `get_parameter` and on-set callback hand back the same type. Issue 2024
// landed a second, event-only `Parameter` here the same day; the decoder below
// builds the shared one instead.

/// `rclcpp::ParameterEventCallbackHandle` — issue 2024: the owning handle
/// `ParameterEventHandler::add_parameter_event_callback` returns, as a
/// `SharedPtr` on a hosted build. Dropping every copy unregisters the
/// callback (the handler keeps a weak reference).
/// The callback is TYPE-ERASED over the frame — upstream's member is typed to
/// `rcl_interfaces::msg::ParameterEvent`, which this header cannot name — and
/// held behind a pointer, so the type has one layout everywhere.
struct ParameterEventCallbackHandle : private detail::ParameterEventHandleNoCopy {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: shared-ptr-interop
    using SharedPtr = ::std::shared_ptr<ParameterEventCallbackHandle>;
    using WeakPtr = ::std::weak_ptr<ParameterEventCallbackHandle>;
#endif
    // Non-copyable through its base (it owns `fn_`); no constructor of its own,
    // as upstream's has none.
    ~ParameterEventCallbackHandle() {
        if (free_ != nullptr) free_(fn_);
    }

  private:
    friend struct detail::ParameterEventHandlerAccess;
    void* fn_ = nullptr; ///< a heap `std::function<void(const uint8_t*, size_t)>`
    void (*free_)(void*) = nullptr;
};

#ifdef NROS_CPP_STD // hosted-family: container-interop
namespace detail {

/// Issue 2024 — a little-endian CDR reader over one `rcl_interfaces/msg/ParameterEvent`
/// frame, enough to build `rclcpp::Parameter`s. Any short read fails the
/// whole frame: a truncation is not "no parameters" (issue 0223's rule).
struct ParameterEventDecoder {
    const uint8_t* p;
    size_t len;
    size_t pos = 4; // past the encapsulation header

    bool align(size_t n) {
        pos = 4 + ((pos - 4 + n - 1) & ~(n - 1));
        return pos <= len;
    }
    template <typename T> bool scalar(T& out, size_t a) {
        if (!align(a) || pos + sizeof(T) > len) return false;
        ::std::memcpy(&out, p + pos, sizeof(T));
        pos += sizeof(T);
        return true;
    }
    bool u32(uint32_t& out) { return scalar(out, 4); }
    bool str(::std::string& out) {
        uint32_t n = 0;
        if (!u32(n) || n == 0 || pos + n > len) return false;
        out.assign(reinterpret_cast<const char*>(p + pos), n - 1); // n counts the NUL
        pos += n;
        return true;
    }
    template <typename T, typename V> bool seq(V& out, size_t a) {
        uint32_t n = 0;
        if (!u32(n)) return false;
        out.clear();
        for (uint32_t k = 0; k < n; ++k) {
            T x{};
            if (!scalar(x, a)) return false;
            out.push_back(x);
        }
        return true;
    }
    bool parameter(::rclcpp::Parameter& out) {
        ::std::string name;
        ::std::string sv;
        uint8_t type = 0;
        uint8_t b = 0;
        int64_t i = 0;
        double d = 0.0;
        if (!str(name) || !scalar(type, 1) || !scalar(b, 1) || !scalar(i, 8) || !scalar(d, 8) ||
            !str(sv))
            return false;
        ::std::vector<uint8_t> bytes;
        ::std::vector<uint8_t> bools;
        ::std::vector<int64_t> ints;
        ::std::vector<double> doubles;
        if (!seq<uint8_t>(bytes, 1) || !seq<uint8_t>(bools, 1) || !seq<int64_t>(ints, 8) ||
            !seq<double>(doubles, 8))
            return false;
        uint32_t n = 0;
        if (!u32(n)) return false;
        ::std::vector<::std::string> strings;
        for (uint32_t k = 0; k < n; ++k) {
            ::std::string x;
            if (!str(x)) return false;
            strings.push_back(x);
        }
        ::rclcpp::ParameterValue v;
        switch (type) {
        case PARAMETER_BOOL:
            v = ::rclcpp::ParameterValue(b != 0);
            break;
        case PARAMETER_INTEGER:
            v = ::rclcpp::ParameterValue(static_cast<long long>(i));
            break;
        case PARAMETER_DOUBLE:
            v = ::rclcpp::ParameterValue(d);
            break;
        case PARAMETER_STRING:
            v = ::rclcpp::ParameterValue(sv);
            break;
        case PARAMETER_BYTE_ARRAY:
            v = ::rclcpp::ParameterValue(bytes);
            break;
        case PARAMETER_BOOL_ARRAY:
            v = ::rclcpp::ParameterValue(::std::vector<bool>(bools.begin(), bools.end()));
            break;
        case PARAMETER_INTEGER_ARRAY:
            v = ::rclcpp::ParameterValue(ints);
            break;
        case PARAMETER_DOUBLE_ARRAY:
            v = ::rclcpp::ParameterValue(doubles);
            break;
        case PARAMETER_STRING_ARRAY:
            v = ::rclcpp::ParameterValue(strings);
            break;
        default:
            break; // NOT_SET: a deleted parameter, or a type this side does not know
        }
        out = ::rclcpp::Parameter(name, v);
        return true;
    }
    bool parameters(::std::vector<::rclcpp::Parameter>& out) {
        uint32_t n = 0;
        if (!u32(n)) return false;
        for (uint32_t k = 0; k < n; ++k) {
            ::rclcpp::Parameter prm;
            if (!parameter(prm)) return false;
            out.push_back(static_cast<::rclcpp::Parameter&&>(prm));
        }
        return true;
    }
    /// `node`, then new + changed (what a parameter callback is told about),
    /// then deleted. Little-endian only: every in-tree publisher and every
    /// stock ROS 2 host writes it.
    bool event(::std::string& node, ::std::vector<::rclcpp::Parameter>& set,
               ::std::vector<::rclcpp::Parameter>& deleted) {
        if (len < 4 || p[1] != 0x01) return false; // CDR_LE only
        int32_t sec = 0;
        uint32_t nanosec = 0;
        return scalar(sec, 4) && u32(nanosec) && str(node) && parameters(set) && parameters(set) &&
               parameters(deleted);
    }
};

/// The argument type an event callback takes (`const EventT&`), deduced from
/// the callable: a lambda, a function pointer or a `std::function`.
template <typename F> struct param_event_arg : param_event_arg<decltype(&F::operator())> {};
template <typename C, typename R, typename A> struct param_event_arg<R (C::*)(A) const> {
    using type = typename ::std::decay<A>::type;
};
template <typename C, typename R, typename A> struct param_event_arg<R (C::*)(A)> {
    using type = typename ::std::decay<A>::type;
};
template <typename R, typename A> struct param_event_arg<R (*)(A)> {
    using type = typename ::std::decay<A>::type;
};

using ParameterEventFn = ::std::function<void(const uint8_t*, size_t)>;

/// What reaches a `ParameterEventCallbackHandle`'s private storage.
struct ParameterEventHandlerAccess {
    static void set(ParameterEventCallbackHandle& h, ParameterEventFn fn) {
        h.fn_ = new ParameterEventFn(static_cast<ParameterEventFn&&>(fn));
        h.free_ = [](void* q) { delete static_cast<ParameterEventFn*>(q); };
    }
    static void call(const ParameterEventCallbackHandle& h, const uint8_t* data, size_t len) {
        if (h.fn_ != nullptr) (*static_cast<ParameterEventFn*>(h.fn_))(data, len);
    }
};

} // namespace detail
#endif

/// `rclcpp::ParameterEventHandler` — issue 2024, HOSTED.
///
/// One `/parameter_events` subscription (`rcl_interfaces/msg/ParameterEvent`)
/// on the node it is constructed with, and two kinds of callback on it; see
/// the file comment. Callbacks run from the executor's dispatch, during a
/// spin of that node's executor. Destroying the handler stops every callback.
///
/// Differences from upstream, each a compile error or written here:
///   - the per-parameter handle is `ParameterEventHandler::ParameterCallbackHandle`,
///     not `rclcpp::ParameterCallbackHandle` — that name is the on-set
///     callback token here (`uint16_t`, `cpp:ParameterCallbackHandle`);
///     `auto` and `ParameterEventHandler::ParameterCallbackHandle::SharedPtr`
///     both compile;
///   - an event callback takes the caller's GENERATED
///     `rcl_interfaces::msg::ParameterEvent` (deduced from the callback), so
///     that message must be generated for the image;
///   - the static `get_parameter(s)_from_event` helpers are absent;
///   - a freestanding build refuses the constructor (no allocator for the
///     callbacks, handles and decoded values).
class ParameterEventHandler {
  public:
#ifdef NROS_CPP_STD // hosted-family: container-interop
    using SharedPtr = ::std::shared_ptr<ParameterEventHandler>;

    /// Upstream's `rclcpp::ParameterCallbackHandle`: one parameter of one node.
    struct ParameterCallbackHandle {
        using SharedPtr = ::std::shared_ptr<ParameterCallbackHandle>;
        using WeakPtr = ::std::weak_ptr<ParameterCallbackHandle>;
        using ParameterCallbackType = ::std::function<void(const ::rclcpp::Parameter&)>;
        ::std::string parameter_name;
        ::std::string node_name;
        ParameterCallbackType callback;
    };
    using ParameterCallbackType = ParameterCallbackHandle::ParameterCallbackType;

    /// Subscribe on `node` (a `shared_ptr` to a `Node` or a type derived from
    /// one). `qos` defaults to upstream's `QoS(KeepLast(1))`.
    template <typename N>
    explicit ParameterEventHandler(const ::std::shared_ptr<N>& node,
                                   const ::rclcpp::QoS& qos = ::rclcpp::QoS(1))
        : ParameterEventHandler(static_cast<::rclcpp::Node*>(node.get()), qos) {}

    /// Subscribe on `node` — the spelling a class derived from `rclcpp::Node`
    /// uses from its constructor (`std::make_shared<ParameterEventHandler>(this)`).
    explicit ParameterEventHandler(::rclcpp::Node* node,
                                   const ::rclcpp::QoS& qos = ::rclcpp::QoS(1)) {
        ::std::shared_ptr<State>* holder =
            new ::std::shared_ptr<State>(::std::make_shared<State>());
        state_ = holder;
        state_free_ = [](void* q) {
            ::std::shared_ptr<State>* h = static_cast<::std::shared_ptr<State>*>(q);
            (*h)->alive = false; // a frame dispatched after the handler is gone does nothing
            delete h;
        };
        if (node == nullptr) return;
        ::std::shared_ptr<State> st = *holder;
        st->node_fqn = fqn_of(*node);
        st->node_ns = node->get_namespace();
        subscription_ = node->template create_subscription<detail::ParameterEventFrame>(
            "/parameter_events", qos,
            [st](const detail::ParameterEventFrame& f) { st->dispatch(f.bytes, f.len); });
    }

    /// `add_parameter_callback(parameter_name, callback, node_name = "")`:
    /// `callback(parameter)` for every new or changed value of
    /// `parameter_name` on `node_name` — this node when empty, a relative name
    /// resolved against this node's namespace. Keep the handle: dropping it
    /// unregisters.
    ParameterCallbackHandle::SharedPtr add_parameter_callback(const ::std::string& parameter_name,
                                                              ParameterCallbackType callback,
                                                              const ::std::string& node_name = "") {
        ParameterCallbackHandle::SharedPtr h = ::std::make_shared<ParameterCallbackHandle>();
        h->parameter_name = parameter_name;
        h->node_name = resolve(node_name);
        h->callback = static_cast<ParameterCallbackType&&>(callback);
        state().parameter_callbacks.push_front(h);
        return h;
    }

    /// `remove_parameter_callback(handle)`.
    void remove_parameter_callback(const ParameterCallbackHandle::SharedPtr& handle) {
        state().parameter_callbacks.remove_if([&handle](const ParameterCallbackHandle::WeakPtr& w) {
            ParameterCallbackHandle::SharedPtr s = w.lock();
            return !s || s == handle;
        });
    }

    /// `add_parameter_event_callback(callback)`: `callback(event)` for every
    /// `/parameter_events` frame, any node. `event`'s type is the one the
    /// callback takes — the caller's generated `rcl_interfaces::msg::ParameterEvent`,
    /// filled by its own `ffi_deserialize`.
    template <typename F>
    ParameterEventCallbackHandle::SharedPtr add_parameter_event_callback(F callback) {
        using EventT = typename detail::param_event_arg<typename ::std::decay<F>::type>::type;
        ParameterEventCallbackHandle::SharedPtr h =
            ::std::make_shared<ParameterEventCallbackHandle>();
        detail::ParameterEventHandlerAccess::set(*h, [callback](const uint8_t* data, size_t len) {
            EventT ev{};
            if (EventT::ffi_deserialize(data, len, &ev) != 0) return;
            callback(ev);
        });
        state().event_callbacks.push_front(h);
        return h;
    }

    /// `remove_parameter_event_callback(handle)`.
    void remove_parameter_event_callback(const ParameterEventCallbackHandle::SharedPtr& handle) {
        state().event_callbacks.remove_if(
            [&handle](const ParameterEventCallbackHandle::WeakPtr& w) {
                ParameterEventCallbackHandle::SharedPtr s = w.lock();
                return !s || s == handle;
            });
    }
#else
    /// **REFUSED** on a freestanding build (issue 2024): its callbacks are
    /// `std::function`s, its handles `shared_ptr`s and the parameters it
    /// decodes hold `std::string` / `std::vector` values.
    template <typename N, typename... Rest> explicit ParameterEventHandler(N&&, Rest&&...) {
        static_assert(::rclcpp::detail::refuse<N>::value,
                      NROS_RCLCPP_REFUSE_PARAMETER_EVENT_HANDLER);
    }
#endif

    ParameterEventHandler(const ParameterEventHandler&) = delete;
    ParameterEventHandler& operator=(const ParameterEventHandler&) = delete;

    /// Destroying the handler stops every callback it holds.
    ~ParameterEventHandler() {
        if (state_free_ != nullptr) state_free_(state_);
    }

  private:
#ifdef NROS_CPP_STD // hosted-family: container-interop
    /// What the subscription's capture shares with the handler, so a frame
    /// dispatched after the handler is gone finds `alive == false`.
    struct State {
        bool alive = true;
        ::std::string node_fqn;
        ::std::string node_ns;
        ::std::list<ParameterCallbackHandle::WeakPtr> parameter_callbacks;
        ::std::list<ParameterEventCallbackHandle::WeakPtr> event_callbacks;

        void dispatch(const uint8_t* data, size_t len) {
            if (!alive) return;
            ::std::string node;
            ::std::vector<::rclcpp::Parameter> set;
            ::std::vector<::rclcpp::Parameter> deleted;
            detail::ParameterEventDecoder dec{data, len};
            const bool decoded = dec.event(node, set, deleted);
            // Snapshot the live handles first: a callback may add or remove.
            if (decoded) {
                ::std::vector<ParameterCallbackHandle::SharedPtr> live;
                for (const ParameterCallbackHandle::WeakPtr& w : parameter_callbacks)
                    if (ParameterCallbackHandle::SharedPtr s = w.lock()) live.push_back(s);
                for (const ParameterCallbackHandle::SharedPtr& h : live) {
                    if (h->node_name != node) continue;
                    for (const ::rclcpp::Parameter& prm : set)
                        if (prm.get_name() == h->parameter_name && h->callback) h->callback(prm);
                }
            }
            ::std::vector<ParameterEventCallbackHandle::SharedPtr> events;
            for (const ParameterEventCallbackHandle::WeakPtr& w : event_callbacks)
                if (ParameterEventCallbackHandle::SharedPtr s = w.lock()) events.push_back(s);
            for (const ParameterEventCallbackHandle::SharedPtr& h : events)
                detail::ParameterEventHandlerAccess::call(*h, data, len);
            parameter_callbacks.remove_if(
                [](const ParameterCallbackHandle::WeakPtr& w) { return w.expired(); });
            event_callbacks.remove_if(
                [](const ParameterEventCallbackHandle::WeakPtr& w) { return w.expired(); });
        }
    };

    State& state() { return **static_cast<::std::shared_ptr<State>*>(state_); }
    const State& state() const { return **static_cast<const ::std::shared_ptr<State>*>(state_); }

    static ::std::string fqn_of(const ::rclcpp::Node& node) {
        char buf[256] = {};
        size_t n = 0;
        if (!node.get_fully_qualified_name(buf, sizeof(buf), &n).ok()) return ::std::string();
        return ::std::string(buf, n);
    }

    /// Upstream's `resolve_path`: empty is this node, an absolute name stays,
    /// a relative one joins this node's namespace.
    ::std::string resolve(const ::std::string& name) const {
        if (name.empty()) return state().node_fqn;
        if (name[0] == '/') return name;
        const ::std::string& ns = state().node_ns;
        if (ns.empty() || ns == "/") return "/" + name;
        return ns + "/" + name;
    }
#endif

    /// The hosted state (a heap `std::shared_ptr<State>` the subscription's
    /// capture shares) behind two pointers, so `sizeof` is one number on every
    /// build.
    void* state_ = nullptr;
    void (*state_free_)(void*) = nullptr;
    typename ::rclcpp::Subscription<detail::ParameterEventFrame>::SharedPtr subscription_;
};

} // namespace rclcpp

#endif // NROS_RCLCPP_PARAMETER_EVENT_HANDLER_HPP
