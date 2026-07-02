/// @file Talker.c
/// @brief QEMU RISC-V NuttX C talker — typed component (RFC-0043).
///
/// `talker_configure` creates a raw publisher on `/chatter` + a timer that
/// publishes the official ROS 2 demo payload (`std_msgs/String`,
/// `Hello World: N`) each tick. The CDR is hand-encoded (phase-277 W4
/// fallback): unlike the other platforms, the NuttX kernel link has no
/// plumbing for the GENERATED typed C interface sources yet, so this
/// example keeps the platform's raw + hand-CDR shape (same as its C
/// service/action siblings). The keyexpr is the DDS-mangled
/// `std_msgs::msg::dds_::String_`, so a typed or raw subscriber matches.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} talker_t;

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_tick(void* ctx) {
    talker_t* self = (talker_t*)ctx;
    /* Pre-increment so the first payload is "Hello World: 1", matching the
     * official ROS 2 demo talker. */
    self->count++;
    char text[32];
    snprintf(text, sizeof(text), "Hello World: %d", (int)self->count);
    /* std_msgs/String CDR: 4-byte encapsulation header (CDR_LE) + u32 LE
     * length (payload incl. NUL) + bytes + NUL. */
    uint32_t n = (uint32_t)strlen(text) + 1u;
    uint8_t buf[8 + sizeof(text)];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, n);
    memcpy(buf + 8, text, n);
    if (nros_cpp_publish_raw(self->pub, buf, 8 + n) == 0) {
        printf("Publishing: '%s'\n", text);
    }
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", "std_msgs::msg::dds_::String_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(talker_t, talker_configure)
