---
id: 2055
title: "A ported C++ client that stores create_client<S>(name) in Client<S>::SharedPtr and passes a per-request callback does not compile"
status: open
type: limitation
area: [api, cpp]
severity: medium
found: 2026-10-25
related: [phase-488, 2037, 1996, phase-518]
---

## What

ros2/demos `composition` @jazzy, `src/client_component.cpp`, writes the client
the way most rclcpp components write one:

```cpp
// header
rclcpp::Client<example_interfaces::srv::AddTwoInts>::SharedPtr client_;

// constructor
client_ = create_client<example_interfaces::srv::AddTwoInts>("add_two_ints");

// timer callback
if (!client_->wait_for_service(1s)) { ... return; }
auto request = std::make_shared<example_interfaces::srv::AddTwoInts::Request>();
using ServiceResponseFuture =
    rclcpp::Client<example_interfaces::srv::AddTwoInts>::SharedFuture;
auto response_received_callback = [this](ServiceResponseFuture future) {
    RCLCPP_INFO(this->get_logger(), "Got result: [%" PRId64 "]", future.get()->sum);
};
auto future_result = client_->async_send_request(request, response_received_callback);
```

Three things in that code fail to compile against nros-cpp:

1. **The member type.** `create_client<S>(name)` with no callback returns
   `rclcpp::Owned<rclcpp::PollClient<S>>`, the future-style client. Assigning
   it to `Client<S>::SharedPtr`, which is the dispatch handle
   `ClientHandle<S>`, fails. This divergence is ledgered at
   `cpp:Node::create_client`.
2. **The per-request callback.** `async_send_request(request, callback)`,
   with a callback taking `Client<S>::SharedFuture`, does not exist. Issue
   2037 resolved the future form and recorded this overload under "Still
   open". `Client<S>::SharedFuture` is not declared either; only
   `PollClient<S>::SharedFuture` is.
3. **Waiting inside a callback.** `wait_for_service(1s)` is called inside a
   timer callback. Ours drives the executor, so it may not run there. The doc
   comment on `ClientHandle::wait_for_service` says to call
   `service_is_ready()` there instead.

The port compiles after the edit below, and runs against a ROS 2 Humble peer.
The edit moves the reply to a later timer tick:

```cpp
rclcpp::Owned<rclcpp::PollClient<S>> client_;
rclcpp::PollClient<S>::FutureAndRequestId pending_;

void on_timer() {
    if (pending_.valid()) {
        if (!pending_.future.is_ready()) return;
        RCLCPP_INFO(get_logger(), "Got result: [%" PRId64 "]", pending_.get()->sum);
    }
    if (!client_->service_is_ready()) return;
    pending_ = client_->async_send_request(request);
}
```

## Why it is not a small fix

A per-request callback needs something to run it when the reply arrives.
Upstream's executor dispatches the reply to the client, and the client looks
up the callback by sequence number. Here the future-style client is polled by
the caller, and the dispatch client (`create_client<S>(name, callback)`) takes
one callback, a function pointer, at registration. Serving
`async_send_request(req, cb)` means one of these:

- a dispatch client that stores one callback per pending request, keyed by
  the sequence id that issue 2009 threads through, in the executor arena;
- or a `PollClient` the executor polls on the caller's behalf.

Either is a change to how clients register with the executor (RFC-0041), not
a header addition.

## Done when

The composition client above compiles unchanged on a hosted build. It needs:

- `Client<S>::SharedPtr` holding what `create_client<S>(name)` returns;
- `async_send_request(request, callback)` running the callback from the
  executor with a `Client<S>::SharedFuture` whose `get()` returns the
  response;
- the porting guide to give `service_is_ready()` as the in-callback
  replacement for `wait_for_service`, or `wait_for_service` to refuse inside
  a callback with that hint.

A runtime test on the stub backend stages the reply and asserts the callback
ran with it. If the overload is deliberately not taken, the porting guide
states the edit above instead.
