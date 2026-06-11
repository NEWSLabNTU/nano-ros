// Phase 235.8 — C++ borrowed runtime E2E driver.
// Serialize an owned message (Rust FFI), then deserialize into the borrowed
// view (real Rust nros_cpp_deserialize_*_borrowed) and assert every borrowed
// view (StringView / Span / LeSpan) aliases the CDR buffer with correct values.
#include <cassert>
#include <cstdio>
#include <cstring>

#include "e2e_msgs_msg_borrowed.hpp"

// The (unused-here) publish FFI fn references this; provide a dummy at link.
extern "C" int nros_cpp_publish_raw(void*, const unsigned char*, unsigned long) { return 0; }

using e2e_msgs::msg::Borrowed;
using e2e_msgs::msg::BorrowedView;

int main() {
    Borrowed msg{};
    msg.width = 0xDEADBEEFu;
    msg.label = "hello";
    msg.data.size = 0;
    msg.data.push_back(10);
    msg.data.push_back(20);
    msg.data.push_back(30);
    msg.data.push_back(40);
    msg.ranges.size = 0;
    msg.ranges.push_back(1.5f);
    msg.ranges.push_back(2.5f);

    uint8_t buf[1024];
    size_t n = 0;
    assert(Borrowed::ffi_serialize(&msg, buf, sizeof buf, &n) == 0);

    BorrowedView view{};
    assert(BorrowedView::deserialize_borrowed(buf, n, &view) == 0);

    assert(view.width == 0xDEADBEEFu);

    assert(view.label.size() == 5);
    assert((const uint8_t*)view.label.data() >= buf && (const uint8_t*)view.label.data() < buf + n);
    assert(memcmp(view.label.data(), "hello", 5) == 0);

    assert(view.data.size() == 4);
    assert((const uint8_t*)view.data.data() >= buf && (const uint8_t*)view.data.data() < buf + n);
    assert(view.data[0] == 10 && view.data[3] == 40);

    assert(view.ranges.size() == 2);
    assert((const uint8_t*)view.ranges.bytes >= buf && (const uint8_t*)view.ranges.bytes < buf + n);
    assert(view.ranges[0] == 1.5f && view.ranges[1] == 2.5f);

    printf("C++ borrowed E2E OK: width=%x label=\"%.*s\" data.size=%zu ranges=[%g,%g]; "
           "all views alias the CDR buffer\n",
           view.width, (int)view.label.size(), view.label.data(), view.data.size(),
           (double)view.ranges[0], (double)view.ranges[1]);
    return 0;
}
