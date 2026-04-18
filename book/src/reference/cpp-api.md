# C++ API

This chapter provides a concise reference for the nano-ros C++ API. The API is
a freestanding C++14 library (no STL, no exceptions, no RTTI required) wrapping
the Rust `nros-node` layer via typed `extern "C"` FFI. It mirrors rclcpp naming
conventions: `Node`, `Publisher<M>`, `Subscription<M>`, `Service<S>`,
`Client<S>`, `ActionServer<A>`, `ActionClient<A>`, `Timer`, `GuardCondition`,
`Executor`.

All types live in the `nros` namespace. Include the umbrella header to get
everything:

```cpp
#include <nros/nros.hpp>
```

## Freestanding vs std Mode

By default, the API is fully freestanding -- it uses `const char*` for strings,
C function pointers for callbacks, and integer milliseconds for durations.

Define `NROS_CPP_STD` before including headers (or via `-DNROS_CPP_STD` in your
build) to enable convenience overloads that use STL types:

| Freestanding | `NROS_CPP_STD` mode |
|---|---|
| `const char*` | `std::string` overloads |
| `void (*)(void* ctx)` callbacks | `std::function<void()>` wrappers |
| `uint64_t` / `int32_t` milliseconds | `std::chrono::milliseconds` overloads |

The std overloads are additive -- freestanding signatures remain available.

## Error Handling: Result and NROS_TRY

All fallible operations return `nros::Result`. There are no exceptions.

```cpp
namespace nros {

enum class ErrorCode : int32_t {
    Ok = 0,
    Error = -1,
    Timeout = -2,
    InvalidArgument = -3,
    NotInitialized = -4,
    Full = -5,
    TryAgain = -6,
    Reentrant = -7,
    TransportError = -100,
};

class Result {
public:
    bool ok() const;
    explicit operator bool() const;  // same as ok()
    ErrorCode code() const;
    int32_t raw() const;
    static constexpr Result success();
};

} // namespace nros
```

Use the `NROS_TRY` macro for early return on error, replacing try/catch:

```cpp
nros::Result run() {
    NROS_TRY(nros::init("tcp/127.0.0.1:7447"));
    nros::Node node;
    NROS_TRY(nros::create_node(node, "my_node"));
    // ...
    return nros::Result::success();
}
```

## QoS

Quality of Service profiles mirror rclcpp with chainable setters:

```cpp
nros::QoS qos = nros::QoS()
    .reliable()
    .keep_last(10);
```

Predefined profiles:

| Profile | Settings |
|---------|----------|
| `QoS::default_profile()` | Reliable, volatile, keep-last(10) |
| `QoS::sensor_data()` | Best-effort, volatile, keep-last(5) |
| `QoS::services()` | Reliable, volatile, keep-last(10) |

Chainable setters: `reliable()`, `best_effort()`, `transient_local()`,
`durability_volatile()`, `keep_last(depth)`, `keep_all()`.

## Initialization

Two patterns are supported: global free functions (simple) and explicit
executor (multi-session).

### Global functions

```cpp
NROS_TRY(nros::init("tcp/127.0.0.1:7447", 0));  // locator, domain_id
bool running = nros::ok();
NROS_TRY(nros::spin_once(10));                    // timeout_ms
NROS_TRY(nros::spin(5000, 10));                   // duration_ms, poll_ms
NROS_TRY(nros::shutdown());
```

### Explicit Executor

```cpp
nros::Executor executor;
NROS_TRY(nros::Executor::create(executor, "tcp/127.0.0.1:7447", 0));

nros::Node node;
NROS_TRY(executor.create_node(node, "my_node"));

while (executor.ok()) {
    executor.spin_once(10);
}
executor.shutdown();
```

## Executor

```cpp
class Executor {
public:
    static Result create(Executor& out,
                         const char* locator = nullptr,
                         uint8_t domain_id = 0);
    Result create_node(Node& out,
                       const char* name,
                       const char* ns = nullptr);
    Result spin_once(int32_t timeout_ms = 10);
    Result spin(uint32_t duration_ms, int32_t poll_ms = 10);
    bool ok() const;
    void* handle() const;  // raw handle for Future::wait()
    Result shutdown();
};
```

The executor uses inline opaque storage -- no heap allocation. All types are
move-only (non-copyable).

## Node

Nodes are the primary interface for creating communication entities.

```cpp
class Node {
public:
    static Result create(Node& out,
                         const char* name,
                         const char* ns = nullptr);
    const char* get_name() const;
    const char* get_namespace() const;
    bool is_valid() const;

    // Entity creation (see sections below)
    template <typename M>
    Result create_publisher(Publisher<M>& out, const char* topic,
                            const QoS& qos = QoS::default_profile());
    template <typename M>
    Result create_subscription(Subscription<M>& out, const char* topic,
                               const QoS& qos = QoS::default_profile());
    template <typename S>
    Result create_service(Service<S>& out, const char* service_name,
                          const QoS& qos = QoS::services());
    template <typename S>
    Result create_client(Client<S>& out, const char* service_name,
                         const QoS& qos = QoS::services());
    template <typename A>
    Result create_action_server(ActionServer<A>& out, const char* action_name,
                                const QoS& qos = QoS::services());
    template <typename A>
    Result create_action_client(ActionClient<A>& out, const char* action_name,
                                const QoS& qos = QoS::services());
    Result create_timer(Timer& out, uint64_t period_ms,
                        nros_cpp_timer_callback_t callback,
                        void* context = nullptr);
    Result create_timer_oneshot(Timer& out, uint64_t delay_ms,
                                nros_cpp_timer_callback_t callback,
                                void* context = nullptr);
    Result create_guard_condition(GuardCondition& out,
                                  nros_cpp_guard_callback_t callback,
                                  void* context = nullptr);
};
```

With the global API, use the free function instead of `Node::create`:

```cpp
nros::Node node;
NROS_TRY(nros::create_node(node, "my_node", "/my_namespace"));
```

## Publisher\<M\>

Publishes typed messages or raw CDR bytes to a topic.

```cpp
template <typename M>
class Publisher {
public:
    Result publish(const M& msg);
    Result publish_raw(const uint8_t* data, size_t len);
    const char* get_topic_name() const;
    bool is_valid() const;
};
```

Usage:

```cpp
nros::Publisher<std_msgs::msg::Int32> pub;
NROS_TRY(node.create_publisher(pub, "/counter"));

std_msgs::msg::Int32 msg;
msg.data = 42;
pub.publish(msg);
```

## Subscription\<M\>

Receives typed messages or raw CDR bytes from a topic. Uses a manual-poll
model -- call `spin_once()` to drive I/O, then `try_recv()` to check for data.

```cpp
template <typename M>
class Subscription {
public:
    bool try_recv(M& msg);
    bool try_recv_raw(uint8_t* buf, size_t capacity, size_t& out_len);
    Stream<M>& stream();
    const char* get_topic_name() const;
    bool is_valid() const;
};
```

Usage:

```cpp
nros::Subscription<std_msgs::msg::Int32> sub;
NROS_TRY(node.create_subscription(sub, "/counter"));

nros::spin_once(10);
std_msgs::msg::Int32 msg;
if (sub.try_recv(msg)) {
    // process msg.data
}
```

For blocking reception, use the subscription's `Stream`:

```cpp
std_msgs::msg::Int32 msg;
NROS_TRY(sub.stream().wait_next(nros::global_handle(), 5000, msg));
```

## Service\<S\>

Server-side service handler. Poll for incoming requests and send replies.

```cpp
template <typename S>
class Service {
public:
    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    bool try_recv_request(RequestType& req, int64_t& seq_id);
    Result send_reply(int64_t seq_id, const ResponseType& resp);
    bool is_valid() const;
};
```

Usage:

```cpp
nros::Service<example_interfaces::srv::AddTwoInts> srv;
NROS_TRY(node.create_service(srv, "/add_two_ints"));

// In spin loop:
nros::spin_once(10);
typename decltype(srv)::RequestType req;
int64_t seq;
if (srv.try_recv_request(req, seq)) {
    typename decltype(srv)::ResponseType resp;
    resp.sum = req.a + req.b;
    srv.send_reply(seq, resp);
}
```

## Client\<S\>

Client-side service caller. Supports both non-blocking (Future-based) and
blocking call patterns.

```cpp
template <typename S>
class Client {
public:
    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    Future<ResponseType> send_request(const RequestType& req);
    Result call(const RequestType& req, ResponseType& resp);  // blocking
    bool is_valid() const;
};
```

Preferred usage (non-blocking):

```cpp
nros::Client<example_interfaces::srv::AddTwoInts> client;
NROS_TRY(node.create_client(client, "/add_two_ints"));

typename decltype(client)::RequestType req;
req.a = 2;
req.b = 3;
auto fut = client.send_request(req);

typename decltype(client)::ResponseType resp;
NROS_TRY(fut.wait(executor.handle(), 5000, resp));
// resp.sum == 5
```

## Future\<T\>

Single-shot deferred result returned by `Client::send_request()` and
`ActionClient::send_goal_future()` / `get_result_future()`. Move-only and
consumed on take.

```cpp
template <typename T>
class Future {
public:
    bool is_ready();
    Result try_take(T& out);
    Result wait(void* executor_handle, uint32_t timeout_ms, T& out);
    void cancel();
    bool is_consumed() const;
};
```

The `wait()` method spins the executor internally, so the transport stays
active while waiting. Pass `executor.handle()` or `nros::global_handle()`.

## Stream\<T\>

Multi-shot message receiver used by `Subscription::stream()` and action
feedback channels. Unlike `Future<T>`, a stream yields multiple values over
time.

```cpp
template <typename T>
class Stream {
public:
    bool try_next(T& out);
    Result wait_next(void* executor_handle, uint32_t timeout_ms, T& out);
    bool is_valid() const;
};
```

Usage:

```cpp
std_msgs::msg::Int32 msg;
// Non-blocking:
if (sub.stream().try_next(msg)) { /* ... */ }
// Blocking with timeout:
NROS_TRY(sub.stream().wait_next(executor.handle(), 5000, msg));
```

## ActionServer\<A\>

Server-side action handler. Mirrors `rclcpp_action::Server<A>` with a
callback-based API: register a typed goal callback that decides whether to
accept, reject, or defer each goal, and (optionally) a cancel callback.
Lifecycle state lives in the arena inside `nros-node` — the C++ class does not
buffer goals. Use `for_each_active_goal` to iterate live goals by UUID +
status.

```cpp
enum class GoalResponse   : int32_t { Reject = 0, AcceptAndExecute = 1, AcceptAndDefer = 2 };
enum class CancelResponse : int32_t { Reject = 0, Accept = 1 };
enum class GoalStatus     : int8_t  { Unknown = 0, Accepted = 1, Executing = 2,
                                       Canceling = 3, Succeeded = 4, Canceled = 5, Aborted = 6 };

template <typename A>
class ActionServer {
public:
    using GoalType     = typename A::Goal;
    using ResultType   = typename A::Result;
    using FeedbackType = typename A::Feedback;

    using TypedGoalFn    = GoalResponse   (*)(const uint8_t uuid[16], const GoalType& goal);
    using TypedCancelFn  = CancelResponse (*)(const uint8_t uuid[16]);
    using TypedVisitorFn = void           (*)(const uint8_t uuid[16], GoalStatus status);

    template <typename F> Result set_goal_callback(F f);     // F decays to TypedGoalFn
    template <typename F> Result set_cancel_callback(F f);   // F decays to TypedCancelFn
    template <typename F> Result for_each_active_goal(F f);  // F decays to TypedVisitorFn

    Result publish_feedback(const uint8_t goal_id[16], const FeedbackType& feedback);
    Result complete_goal(const uint8_t goal_id[16], const ResultType& result);
    bool   is_valid() const;
};
```

Callbacks must be **stateless** (empty-capture lambdas or plain function
pointers). The freestanding C++14 API has no `std::function` storage per
instance, so per-goal state must be tracked in user-side `{uuid → state}`
storage keyed by the UUID delivered to the goal callback.

Usage:

```cpp
using Fib = example_interfaces::action::Fibonacci;
static nros::ActionServer<Fib>* g_srv = nullptr;

static nros::GoalResponse on_goal(const uint8_t uuid[16], const Fib::Goal& goal) {
    if (goal.order < 0 || goal.order >= 64) {
        return nros::GoalResponse::Reject;
    }

    Fib::Result result;
    int32_t a = 0, b = 1;
    for (int32_t i = 0; i < goal.order; i++) {
        result.sequence.push_back(a);

        // Periodic feedback
        if (i > 0 && i % 3 == 0) {
            Fib::Feedback fb;
            for (uint32_t k = 0; k < result.sequence.length(); k++) {
                fb.sequence.push_back(result.sequence[k]);
            }
            g_srv->publish_feedback(uuid, fb);
        }
        int32_t next = a + b; a = b; b = next;
    }
    g_srv->complete_goal(uuid, result);
    return nros::GoalResponse::AcceptAndExecute;
}

nros::ActionServer<Fib> srv;
NROS_TRY(node.create_action_server(srv, "/fibonacci"));
g_srv = &srv;
NROS_TRY(srv.set_goal_callback(on_goal));

while (nros::ok()) {
    nros::spin_once(10);
}
```

Iterate currently live goals (e.g. for status reporting):

```cpp
srv.for_each_active_goal([](const uint8_t uuid[16], nros::GoalStatus status) {
    // ...
});
```

The arena does not retain the original goal CDR payload, so the visitor
receives only `(uuid, status)`. If you need the parsed goal bytes later, stash
them in your own `{uuid → state}` table from inside `set_goal_callback`.

## ActionClient\<A\>

Client-side action interface. Three usage patterns are available: blocking,
Future-based, and callback-based.

```cpp
template <typename A>
class ActionClient {
public:
    using GoalType = typename A::Goal;
    using ResultType = typename A::Result;
    using FeedbackType = typename A::Feedback;

    // Blocking API
    Result send_goal(const GoalType& goal, uint8_t goal_id[16]);
    Result get_result(const uint8_t goal_id[16], ResultType& result);

    // Future-based API (non-blocking)
    Future<GoalAccept> send_goal_future(const GoalType& goal);
    Future<ResultType> get_result_future(const uint8_t goal_id[16]);

    // Callback-based API (non-blocking)
    Result send_goal_async(const GoalType& goal, uint8_t goal_id[16]);
    Result get_result_async(const uint8_t goal_id[16]);
    void set_callbacks(const SendGoalOptions& options);
    void poll();

    // Feedback (all patterns)
    bool try_recv_feedback(FeedbackType& feedback);

    bool is_valid() const;
};
```

The `GoalAccept` nested struct returned by `send_goal_future()` contains the
16-byte goal UUID and an `accepted` flag.

### Blocking pattern

```cpp
nros::ActionClient<example_interfaces::action::Fibonacci> client;
NROS_TRY(node.create_action_client(client, "/fibonacci"));

typename decltype(client)::GoalType goal;
goal.order = 10;
uint8_t goal_id[16];
NROS_TRY(client.send_goal(goal, goal_id));

typename decltype(client)::ResultType result;
NROS_TRY(client.get_result(goal_id, result));
```

### Future-based pattern

```cpp
auto goal_fut = client.send_goal_future(goal);
typename decltype(client)::GoalAccept accept;
NROS_TRY(goal_fut.wait(executor.handle(), 5000, accept));

if (accept.accepted) {
    auto result_fut = client.get_result_future(accept.goal_id);
    typename decltype(client)::ResultType result;
    NROS_TRY(result_fut.wait(executor.handle(), 10000, result));
}
```

### Callback-based pattern

```cpp
typename decltype(client)::SendGoalOptions opts;
opts.goal_response = [](bool accepted, const uint8_t id[16], void* ctx) {
    // handle acceptance
};
opts.feedback = [](const uint8_t id[16], const uint8_t* data,
                   size_t len, void* ctx) {
    // handle feedback
};
opts.result = [](const uint8_t id[16], int status,
                 const uint8_t* data, size_t len, void* ctx) {
    // handle result
};
opts.context = nullptr;
client.set_callbacks(opts);

NROS_TRY(client.send_goal_async(goal, goal_id));
NROS_TRY(client.get_result_async(goal_id));

// In spin loop, after spin_once():
client.poll();
```

## Timer

Repeating or one-shot timer. Callbacks fire during `spin_once()`.

```cpp
class Timer {
public:
    Result cancel();
    Result reset();
    bool is_cancelled() const;
    bool is_valid() const;
};
```

Usage:

```cpp
void on_timer(void* ctx) {
    // periodic work
}

nros::Timer timer;
NROS_TRY(node.create_timer(timer, 1000, on_timer));        // repeating, 1s
NROS_TRY(node.create_timer_oneshot(timer, 5000, on_timer)); // one-shot, 5s

timer.cancel();
timer.reset();  // restart from zero
```

With `NROS_CPP_STD`:

```cpp
nros::Timer timer;
NROS_TRY(nros::create_timer(node, timer, std::chrono::milliseconds(1000),
                             []() { /* periodic work */ }));
```

## GuardCondition

Cross-thread signaling primitive. The `trigger()` method is thread-safe and
lock-free. The callback fires on the next `spin_once()`.

```cpp
class GuardCondition {
public:
    Result trigger();
    bool is_valid() const;
};
```

Usage:

```cpp
void on_signal(void* ctx) {
    // handle event
}

nros::GuardCondition guard;
NROS_TRY(node.create_guard_condition(guard, on_signal));

// From any thread:
guard.trigger();
```

## Message Types and Code Generation

Message, service, and action types are generated by the nano-ros codegen tool.
Generated types use ROS 2 standard namespaces (e.g., `std_msgs::msg::Int32`,
`example_interfaces::srv::AddTwoInts`).

Each generated message type `M` provides:

- `M::TYPE_NAME` -- ROS 2 type name string
- `M::TYPE_HASH` -- type hash string
- `M::SERIALIZED_SIZE_MAX` -- maximum CDR serialized size
- `M::ffi_serialize(const M*, uint8_t*, size_t, size_t*)` -- serialize to CDR
- `M::ffi_deserialize(const uint8_t*, size_t, M*)` -- deserialize from CDR
- `M::ffi_publish(void*, const M*)` -- direct typed publish

## CMake Integration

### Native (POSIX, bare-metal, RTOS)

```cmake
find_package(NanoRos REQUIRED CONFIG)

# Generate C++ interface types
nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    "msg/String.msg"
    LANGUAGE CPP
    SKIP_INSTALL
)

nano_ros_generate_interfaces(example_interfaces
    "srv/AddTwoInts.srv"
    "action/Fibonacci.action"
    LANGUAGE CPP
    SKIP_INSTALL
)

add_executable(my_app main.cpp)
target_link_libraries(my_app PRIVATE NanoRos::NanoRosCpp)
target_link_libraries(my_app PRIVATE std_msgs example_interfaces)
```

The `LANGUAGE CPP` argument tells the codegen to emit C++ headers with typed
serialize/deserialize methods.

### Zephyr

Enable the C++ API in `prj.conf`:

```ini
CONFIG_NROS=y
CONFIG_NROS_CPP_API=y
```

Generate interfaces in `CMakeLists.txt`:

```cmake
nros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    LANGUAGE CPP
)
```

## Ownership and Lifetime

All nros-cpp types are move-only (non-copyable). Entities (publishers,
subscriptions, services, etc.) must not outlive their parent node. Nodes must
not outlive their executor. Destructors automatically release resources.

All types use inline opaque storage -- no heap allocation is required in
freestanding mode. The `NROS_CPP_STD` convenience wrappers for
`std::function` callbacks do allocate on the heap; the caller is responsible
for the callback lifetime.
