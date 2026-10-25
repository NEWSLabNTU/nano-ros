// phase-488 W3 — the hosted rclcpp surface a stock ROS 2 C++ package uses, as
// it spells it. Each case is a shape the Autoware `autoware_external_cmd_selector`
// (0.45.0) port needed nano-ros to accept; the node below is written for this
// test, in that package's style, and is not the ported code.
//
// What this RUNS, on the real `libnros_cpp.a` + the stub backend:
//
//   1. `create_callback_group(rclcpp::CallbackGroupType::MutuallyExclusive)`,
//      put in `SubscriptionOptions::callback_group` and passed to the
//      four-argument `create_subscription<M>(topic, qos, callback, options)`:
//      a staged sample reaches the member function `std::bind` named;
//   2. `create_service<S>(name, std::bind(&X::f, this, _1, _2),
//      rmw_qos_profile_services_default, group)`, with a handler that returns
//      `bool` and has STATE (`this`): a staged request is answered from the
//      object's state, and the handler changes it;
//   3. a capturing lambda as a service handler: its capture is copied into the
//      service's entry, so it survives the registering call, and is destroyed
//      exactly once when the node goes;
//   4. `pub_->publish(msg);` as a statement, under `-Wall -Wextra -Werror`, and
//      `pub_->publish(std::move(unique_ptr))` (ros2/demos `composition`): the
//      std surface's publish result may be ignored;
//   5. `diagnostic_updater::Updater updater_{this};` publishes on its own
//      timer, with nobody calling `update()`;
//   6. `RCLCPP_COMPONENTS_REGISTER_NODE(X)` defines the factory the generated
//      component main calls. The check also LINKS that main — written by
//      `cmake/NanoRosComponentMain.cmake`, which declares the class without
//      its header — against this file built as a component only; it does not
//      run it, because it spins forever.
//   7. built with `NROS_AMENT_START_PARAMETER_SERVICES` (what the ament verbs
//      define when the runtime carries `param_services`) and linked against
//      such a runtime, the node serves its parameters with no call of ours:
//      a `list_parameters` request is answered.

#include <diagnostic_updater/diagnostic_updater.hpp>
#include <diagnostic_updater/update_functions.hpp>
#include <rclcpp/rclcpp.hpp>
#include <rclcpp_components/register_node_macro.hpp>
// Nav2's `nav2_util` includes these by name (phase-488 node 2).
#include <rclcpp/logger.hpp>
#include <rclcpp/parameter_service.hpp>

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <ctime>
#include <functional>
#include <memory>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    nros_stub_rmw_set_accept_entities(true);
    // No per-entity wake: the executor polls every entity each pass, so a
    // request staged after the first spin is still taken.
    nros_stub_rmw_set_entity_wake_supported(false);
    (void)nros_stub_rmw_register();
}

namespace {

/// `[encap][int32 LE]`, the CDR a one-field message has on the wire.
int write_i32(int32_t v, uint8_t* buf, size_t cap, size_t* out) {
    if (cap < 8) return -1;
    const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
    std::memcpy(buf, hdr, 4);
    std::memcpy(buf + 4, &v, 4);
    *out = 8;
    return 0;
}
int read_i32(const uint8_t* data, size_t len, int32_t* v) {
    if (len != 8) return -1;
    std::memcpy(v, data + 4, 4);
    return 0;
}

struct Mode {
#if defined(NROS_CPP_HAS_SHARED_PTR)
    using SharedPtr = ::std::shared_ptr<Mode>;
    using ConstSharedPtr = ::std::shared_ptr<const Mode>;
#endif
    int32_t data = 0;
    static constexpr const char* TYPE_NAME = "demo_msgs::msg::dds_::Mode_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub_mode";
    static constexpr size_t SERIALIZED_SIZE_MAX = 8;
    // As the generated glue does: serialize, then hand the bytes to the runtime.
    static int ffi_publish(void* handle, const void* m) {
        uint8_t buf[8];
        size_t len = 0;
        if (write_i32(static_cast<const Mode*>(m)->data, buf, sizeof(buf), &len) != 0) return -1;
        return nros_cpp_publish_raw(handle, buf, len);
    }
    static int ffi_serialize(const void* m, uint8_t* buf, size_t cap, size_t* out) {
        return write_i32(static_cast<const Mode*>(m)->data, buf, cap, out);
    }
    static int ffi_deserialize(const uint8_t* d, size_t len, void* m) {
        return read_i32(d, len, &static_cast<Mode*>(m)->data);
    }
};

struct Select {
    struct Request {
#if defined(NROS_CPP_HAS_SHARED_PTR)
        using SharedPtr = ::std::shared_ptr<Request>;
        using ConstSharedPtr = ::std::shared_ptr<const Request>;
#endif
        int32_t mode = 0;
        static constexpr const char* TYPE_NAME = "demo_msgs::srv::dds_::Select_Request_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_select";
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
        static int ffi_serialize(const void* m, uint8_t* buf, size_t cap, size_t* out) {
            return write_i32(static_cast<const Request*>(m)->mode, buf, cap, out);
        }
        static int ffi_deserialize(const uint8_t* d, size_t len, void* m) {
            return read_i32(d, len, &static_cast<Request*>(m)->mode);
        }
    };
    struct Response {
#if defined(NROS_CPP_HAS_SHARED_PTR)
        using SharedPtr = ::std::shared_ptr<Response>;
        using ConstSharedPtr = ::std::shared_ptr<const Response>;
#endif
        int32_t previous = 0;
        static constexpr const char* TYPE_NAME = "demo_msgs::srv::dds_::Select_Response_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_select";
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
        static int ffi_serialize(const void* m, uint8_t* buf, size_t cap, size_t* out) {
            return write_i32(static_cast<const Response*>(m)->previous, buf, cap, out);
        }
        static int ffi_deserialize(const uint8_t* d, size_t len, void* m) {
            return read_i32(d, len, &static_cast<Response*>(m)->previous);
        }
    };
    static constexpr const char* TYPE_NAME = "demo_msgs::srv::dds_::Select_";
};

/// Counts live copies, so "destroyed exactly once" is "back to where it began".
struct Tracked {
    static int live;
    Tracked() { ++live; }
    Tracked(const Tracked&) { ++live; }
    Tracked(Tracked&&) { ++live; }
    ~Tracked() { --live; }
};
int Tracked::live = 0;

} // namespace

namespace demo {

class Selector : public rclcpp::Node {
  public:
    explicit Selector(const rclcpp::NodeOptions& options) : Node("selector", options) {
        using std::placeholders::_1;
        using std::placeholders::_2;
        group_ = create_callback_group(rclcpp::CallbackGroupType::MutuallyExclusive);
        rclcpp::SubscriptionOptions sub_options;
        sub_options.callback_group = group_;
        sub_ = create_subscription<Mode>("~/input/mode", rclcpp::QoS(1),
                                         std::bind(&Selector::on_mode, this, _1), sub_options);
        srv_ = create_service<Select>("~/service/select",
                                      std::bind(&Selector::on_select, this, _1, _2),
                                      rmw_qos_profile_services_default, group_);
        Tracked owner;
        lambda_srv_ = create_service<Select>("~/service/echo",
                                             [owner, this](const Select::Request::SharedPtr req,
                                                           const Select::Response::SharedPtr res) {
                                                 (void)owner;
                                                 res->previous = req->mode + offset_;
                                             });
        pub_ = create_publisher<Mode>("~/output/mode", 1);
        updater_.setHardwareID("selector");
        updater_.add("heartbeat", this, &Selector::on_diag);
        updater_.setPeriod(0.02);
    }

    void publish_mode() {
        Mode msg;
        msg.data = current_;
        pub_->publish(msg); // -Werror: the std surface's result is ignorable
    }

    // ros2/demos `composition`'s talker: the message handed over as a
    // `std::unique_ptr`.
    void publish_mode_owned() {
        auto msg = std::make_unique<Mode>();
        msg->data = current_;
        pub_->publish(std::move(msg));
    }

    bool publish_null_refused() {
        return pub_->publish(std::unique_ptr<Mode>()).raw() ==
               static_cast<int32_t>(rclcpp::ErrorCode::InvalidArgument);
    }

    int32_t current_ = 1;
    int32_t last_input_ = 0;
    int32_t offset_ = 100;
    int diag_calls_ = 0;
    bool group_valid() const { return static_cast<bool>(group_); }

  private:
    void on_mode(const Mode::ConstSharedPtr msg) { last_input_ = msg->data; }

    bool on_select(const Select::Request::SharedPtr req, const Select::Response::SharedPtr res) {
        res->previous = current_;
        current_ = req->mode;
        return true;
    }

    void on_diag(diagnostic_updater::DiagnosticStatusWrapper& stat) {
        ++diag_calls_;
        stat.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "ok");
    }

    rclcpp::CallbackGroup::SharedPtr group_;
    rclcpp::Subscription<Mode>::SharedPtr sub_;
    rclcpp::Service<Select>::SharedPtr srv_;
    rclcpp::Service<Select>::SharedPtr lambda_srv_;
    rclcpp::Publisher<Mode>::SharedPtr pub_;
    diagnostic_updater::Updater updater_{this};
};

} // namespace demo

RCLCPP_COMPONENTS_REGISTER_NODE(demo::Selector)

// Built a second time with this defined, as the component the generated main
// links against (case 6): the factory above, and no `main` of its own.
#if !defined(NROS_PORT_SURFACE_COMPONENT_ONLY)

namespace {

// `rclcpp::Time(msg->header.stamp)` (phase-488 node 2; 78 Autoware sources):
// from anything shaped like `builtin_interfaces/msg/Time`, on the ROS clock by
// default, as upstream's implicit constructor is.
struct StampLike {
    int32_t sec;
    uint32_t nanosec;
};
constexpr StampLike kStamp{3, 250000000u};
static_assert(rclcpp::Time(kStamp).nanoseconds() == 3250000000LL,
              "rclcpp::Time from a stamp keeps its seconds and nanoseconds");
static_assert(rclcpp::Time(kStamp).get_clock_type() == RCL_ROS_TIME,
              "rclcpp::Time from a stamp is on the ROS clock, as upstream's is");
static_assert(rclcpp::Time(kStamp, RCL_STEADY_TIME).get_clock_type() == RCL_STEADY_TIME,
              "rclcpp::Time from a stamp takes a clock type");

int g_failures = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

template <typename P> bool spin_until(const std::shared_ptr<rclcpp::Node>& node, P pred) {
    for (int i = 0; i < 200; ++i) {
        rclcpp::spin_some(node);
        if (pred()) return true;
        struct timespec ts = {0, 5 * 1000 * 1000};
        nanosleep(&ts, nullptr);
    }
    return false;
}

bool call(const char* suffix, int64_t seq, int32_t mode, int32_t* answer,
          const std::shared_ptr<rclcpp::Node>& node) {
    uint8_t req[8];
    size_t len = 0;
    (void)write_i32(mode, req, sizeof(req), &len);
    if (!nros_stub_rmw_stage_request(suffix, seq, req, len)) return false;
    return spin_until(node, [&]() {
        int64_t got_seq = -1;
        size_t got_len = 0;
        const uint8_t* got = nros_stub_rmw_last_response(suffix, &got_seq, &got_len);
        return got != nullptr && got_seq == seq && read_i32(got, got_len, answer) == 0;
    });
}

} // namespace

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    const int live_before = Tracked::live;
    {
        // (6) through the factory the generated main calls.
        std::shared_ptr<rclcpp::Node> node =
            nros_ament_component_detail::make_component<demo::Selector>(rclcpp::NodeOptions{},
                                                                        "selector");
        check(static_cast<bool>(node), "the registered factory constructs the component");
        auto* sel = static_cast<demo::Selector*>(node.get());
        check(sel->group_valid(), "create_callback_group(type) returns a group that tests true");

        // (3) the lambda the constructor built is gone; the entry's copy lives.
        check(Tracked::live == live_before + 1,
              "a capturing service handler's capture lives in its entry, once");

        // (1) the four-argument subscription with a group in its options.
        uint8_t sample[8];
        size_t len = 0;
        (void)write_i32(7, sample, sizeof(sample), &len);
        check(nros_stub_rmw_stage_sample("/input/mode", sample, len), "stage a sample");
        check(spin_until(node, [&]() { return sel->last_input_ == 7; }),
              "a std::bind member callback in a grouped subscription receives the sample");

        // (2) the stateful std::bind service handler returning bool.
        int32_t previous = -1;
        check(call("/service/select", 11, 2, &previous, node), "the select service answers");
        check(previous == 1, "the answer is read from the object's state");
        check(sel->current_ == 2, "the handler changed the object's state");
        check(call("/service/select", 12, 3, &previous, node) && previous == 2,
              "a second call sees the first call's change");

        // (3) the capturing lambda, through its arena copy.
        int32_t echoed = -1;
        check(call("/service/echo", 13, 5, &echoed, node) && echoed == 105,
              "a capturing lambda service handler answers through its captured `this`");

        // (4) the ignored publish result.
        const uint32_t before = nros_stub_rmw_publish_count();
        sel->publish_mode();
        check(nros_stub_rmw_publish_count() == before + 1,
              "publish() with its result ignored publishes");
        sel->publish_mode_owned();
        check(nros_stub_rmw_publish_count() == before + 2, "publish(std::unique_ptr<M>) publishes");
        check(sel->publish_null_refused() && nros_stub_rmw_publish_count() == before + 2,
              "publish() of a null unique_ptr is refused and sends nothing");

#if defined(NROS_AMENT_START_PARAMETER_SERVICES)
        // (7) `[encap][prefixes: u32 0][pad][depth: u64 0]`, an empty request.
        {
            const uint8_t list_req[20] = {0x00, 0x01, 0x00, 0x00};
            check(nros_stub_rmw_stage_request("/selector/list_parameters", 21, list_req,
                                              sizeof(list_req)),
                  "stage a list_parameters request");
            check(spin_until(node,
                             [&]() {
                                 int64_t got = -1;
                                 return nros_stub_rmw_last_response("/selector/list_parameters",
                                                                    &got, nullptr) != nullptr &&
                                        got == 21;
                             }),
                  "the node serves its parameters with no call of ours");
        }
#endif

        // (5) the updater's own timer.
        check(spin_until(node, [&]() { return sel->diag_calls_ >= 2; }),
              "diagnostic_updater::Updater publishes on its own timer");
    }
    check(Tracked::live == live_before,
          "destroying the node destroys the service handler's capture, exactly once");
    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "ament_port_surface_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("ament_port_surface_runtime: OK\n");
    return 0;
}

#endif // !NROS_PORT_SURFACE_COMPONENT_ONLY
