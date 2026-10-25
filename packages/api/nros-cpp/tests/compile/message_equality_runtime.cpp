// phase-488 — a generated message has rosidl's `operator==` / `operator!=`.
//
// Nav2's velocity smoother writes `last_cmd_.twist == geometry_msgs::msg::Twist()`.
// This compiles the committed C++ golden of the corpus's every-field-shape
// message (`Shapes.hpp`, the pack's real output) and checks that equality is
// field-wise and by VALUE: primitives, a fixed string, a bounded and an
// unbounded sequence, a sequence of strings, a fixed array, and a sequence of
// messages (which reaches the element message's own `operator==`).

#define NROS_CPP_STD 1

#include "Shapes.hpp"

#include <cstdio>
#include <cstring>

// The heap sequences allocate through the runtime, so this links
// `libnros_cpp.a`, whose session init names this hook; nothing here opens one.
extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

using fingerprint_corpus::msg::Shapes;

} // namespace

int main() {
    Shapes a;
    Shapes b;
    check(a == b && !(a != b), "two default messages are equal");

    a.f64_v = 1.5;
    check(a != b, "a primitive field differs");
    b.f64_v = 1.5;
    check(a == b, "...and equal again once it matches");

    a.text = "odom";
    check(a != b, "a fixed string differs");
    b.text = "odom";
    check(a == b, "the same text is equal");

    a.arr_fixed[2] = 3.0;
    check(a != b, "a fixed-array element differs");
    b.arr_fixed[2] = 3.0;

    (void)a.seq_bounded.push_back(7);
    check(a != b, "a bounded sequence of a different length differs");
    (void)b.seq_bounded.push_back(7);
    check(a == b, "the same bounded sequence is equal");

    rclcpp::FixedString<256> s;
    s = "x";
    (void)a.seq_string.push_back(s);
    (void)b.seq_string.push_back(s);
    check(a == b, "a sequence of strings compares its elements");
    b.seq_string[0] = "y";
    check(a != b, "...element by element");
    b.seq_string[0] = "x";

    (void)a.seq_prim.push_back(42);
    check(a != b, "an unbounded sequence of a different length differs");
    (void)b.seq_prim.push_back(42);
    check(a == b, "the same unbounded sequence is equal, whatever its capacity");

    // `Shapes` is move-only (it holds a `HeapSequence`), so the nested case is
    // an array of messages, which a generated `Shapes[N]` field compares the
    // same way.
    Shapes ma[2];
    Shapes mb[2];
    check(rclcpp::detail::msg_field_eq(ma, mb), "an array of messages uses their operator==");
    mb[1].flag = true;
    check(!rclcpp::detail::msg_field_eq(ma, mb), "...and sees a nested field differ");

    if (g_failures) {
        std::fprintf(stderr, "message_equality_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("message_equality_runtime: ok\n");
    return 0;
}
