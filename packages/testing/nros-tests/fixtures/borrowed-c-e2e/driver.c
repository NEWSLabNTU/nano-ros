// Phase 235.4 — C borrowed runtime E2E driver (issue 0021, RFC-0033).
//
// Owned-serialize a message, deserialize into the borrowed view, and assert the
// view fields point INTO the CDR buffer (zero-copy) with correct values. Built
// against the generated `e2e_msgs_msg_borrowed.{h,c}` (emitted by the
// rosidl-codegen `emit_c_borrowed_e2e` test) + linked to `libnros_c.a` for the
// CDR readers. Driven by `tests/borrowed_c_e2e.sh`.
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "e2e_msgs_msg_borrowed.h"

int main(void) {
    e2e_msgs_msg_borrowed msg;
    e2e_msgs_msg_borrowed_init(&msg);
    msg.width = 0xDEADBEEFu;
    strcpy(msg.label, "hello");
    msg.data.size = 4;
    msg.data.data[0] = 10;
    msg.data.data[1] = 20;
    msg.data.data[2] = 30;
    msg.data.data[3] = 40;
    msg.ranges.size = 2;
    msg.ranges.data[0] = 1.5f;
    msg.ranges.data[1] = 2.5f;

    uint8_t buf[1024];
    size_t n = 0;
    assert(e2e_msgs_msg_borrowed_serialize(&msg, buf, sizeof buf, &n) == 0);

    e2e_msgs_msg_borrowed_View view;
    assert(e2e_msgs_msg_borrowed_deserialize_borrowed(&view, buf, n) == 0);

    // Copied scalar.
    assert(view.width == 0xDEADBEEFu);

    // Borrowed string: aliases buf, excludes NUL from size.
    assert(view.label.size == 5);
    assert((const uint8_t*)view.label.data >= buf && (const uint8_t*)view.label.data < buf + n);
    assert(memcmp(view.label.data, "hello", 5) == 0);

    // Borrowed byte sequence: aliases buf.
    assert(view.data.size == 4);
    assert(view.data.data >= buf && view.data.data < buf + n);
    assert(view.data.data[0] == 10 && view.data.data[3] == 40);

    // Borrowed numeric (LE view): aliases buf, decodes per element.
    assert(view.ranges.count == 2);
    assert(view.ranges.bytes >= buf && view.ranges.bytes < buf + n);
    assert(nros_le_slice_view_f32_get(view.ranges, 0) == 1.5f);
    assert(nros_le_slice_view_f32_get(view.ranges, 1) == 2.5f);

    printf("C borrowed E2E OK: width=%x label=\"%.*s\" data.size=%zu ranges=[%g,%g]; "
           "all views alias the CDR buffer\n",
           view.width, (int)view.label.size, view.label.data, view.data.size,
           (double)nros_le_slice_view_f32_get(view.ranges, 0),
           (double)nros_le_slice_view_f32_get(view.ranges, 1));
    return 0;
}
