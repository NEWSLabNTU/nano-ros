/*
 * phase-492 W7/W8 — the rosidl-shaped C message surface, run.
 *
 * Three behaviours, none visible to a syntax check:
 *
 *   1. CAPACITY IS A LIMIT. Deserialising a string (or a sequence) longer than
 *      the `capacity` the caller wired returns an error and writes NOTHING past
 *      it — measured with guard bytes around the buffer, and the bytes inside it
 *      too, because the decoder refuses before copying.
 *   2. NOTHING ALLOCATES. `__init_in`, `__serialize` and `__deserialize` of
 *      std_msgs/String and sensor_msgs/LaserScan, and `rcl_publish` end to end
 *      (descriptor codec -> bounded staging -> RMW), are run under a counting
 *      allocator interposed on malloc/calloc/realloc/posix_memalign/
 *      aligned_alloc, and must see zero calls. RFC-0105 D10: the callee never
 *      allocates.
 *   3. `rcl_publish` PUTS THE MESSAGE'S OWN BYTES ON THE WIRE: what the stub
 *      backend received equals what `std_msgs__msg__String__serialize`
 *      produces, and a non-NULL allocation or a codec-less descriptor is
 *      refused.
 *
 * Links the committed rosidl-laid-out codegen goldens' `.c` files (the stock
 * Humble std_msgs/String, std_msgs/Header, builtin_interfaces/Time,
 * sensor_msgs/LaserScan), the lane's `libnros_c.a` and the stub RMW backend.
 */

#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include "stub_rmw_backend.h"

#include <rclc/rclc.h>
#include <rosidl_runtime_c/message_type_support_struct.h>
#include <sensor_msgs/msg/laser_scan.h>
#include <std_msgs/msg/string.h>

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ---- counting allocator ------------------------------------------------- */

extern void* __libc_malloc(size_t);
extern void* __libc_calloc(size_t, size_t);
extern void* __libc_realloc(void*, size_t);
extern void* __libc_memalign(size_t, size_t);
extern void __libc_free(void*);

static volatile int g_counting = 0;
static volatile unsigned g_allocs = 0;

void* malloc(size_t n) {
    if (g_counting) g_allocs++;
    return __libc_malloc(n);
}
void* calloc(size_t a, size_t b) {
    if (g_counting) g_allocs++;
    return __libc_calloc(a, b);
}
void* realloc(void* p, size_t n) {
    if (g_counting) g_allocs++;
    return __libc_realloc(p, n);
}
int posix_memalign(void** out, size_t align, size_t n) {
    if (g_counting) g_allocs++;
    void* p = __libc_memalign(align, n);
    if (p == NULL) return 12; /* ENOMEM */
    *out = p;
    return 0;
}
void* aligned_alloc(size_t align, size_t n) {
    if (g_counting) g_allocs++;
    return __libc_memalign(align, n);
}
void* memalign(size_t align, size_t n);
void* memalign(size_t align, size_t n) {
    if (g_counting) g_allocs++;
    return __libc_memalign(align, n);
}
void free(void* p) {
    __libc_free(p);
}

/* ---- harness ------------------------------------------------------------ */

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

static int g_failures = 0;
#define CHECK(cond, ...)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL %s:%d: ", __FILE__, __LINE__);                                   \
            fprintf(stderr, __VA_ARGS__);                                                          \
            fprintf(stderr, "\n");                                                                 \
            g_failures++;                                                                          \
        }                                                                                          \
    } while (0)

static std_msgs__msg__String__Storage s_string_storage;
static sensor_msgs__msg__LaserScan__Storage s_scan_storage;
static uint8_t s_wire[2048];

/* 1. capacity is a limit ---------------------------------------------------- */

static void capacity_is_a_limit(void) {
    /* A 20-character string, serialised from a buffer large enough for it. */
    char source[64];
    std_msgs__msg__String out;
    out.data.data = source;
    out.data.capacity = sizeof source;
    out.data.size = (size_t)snprintf(source, sizeof source, "%s", "twenty characters!!!");
    size_t n = 0;
    CHECK(std_msgs__msg__String__serialize(&out, s_wire, sizeof s_wire, &n) == 0 && n > 0,
          "a 20-character String did not serialise");

    /* Received into 8 bytes of capacity, with guard bytes on both sides. */
    uint8_t region[32];
    memset(region, 0xA5, sizeof region);
    std_msgs__msg__String in;
    in.data.data = (char*)&region[8];
    in.data.capacity = 8;
    in.data.size = 0;
    int32_t rc = std_msgs__msg__String__deserialize(&in, s_wire, n);
    CHECK(rc != 0, "a 20-character string deserialised into 8 bytes of capacity (rc=%d)", rc);
    for (size_t i = 0; i < sizeof region; ++i) {
        CHECK(region[i] == 0xA5, "byte %zu of the guarded region was written (0x%02x)", i,
              region[i]);
    }
    CHECK(in.data.size == 0, "size moved on a refused decode (%zu)", in.data.size);

    /* The same bytes fit 21 bytes exactly: 20 characters and the NUL. */
    char exact[21];
    in.data.data = exact;
    in.data.capacity = sizeof exact;
    rc = std_msgs__msg__String__deserialize(&in, s_wire, n);
    CHECK(rc == 0 && in.data.size == 20 && strcmp(exact, "twenty characters!!!") == 0,
          "the string did not decode into exactly-sufficient capacity (rc=%d size=%zu)", rc,
          in.data.size);

    /* A sequence past its capacity: 10 ranges into a capacity of 4. */
    sensor_msgs__msg__LaserScan scan;
    sensor_msgs__msg__LaserScan__init_in(&scan, &s_scan_storage);
    for (size_t i = 0; i < 10; ++i)
        scan.ranges.data[i] = (float)i;
    scan.ranges.size = 10;
    n = 0;
    CHECK(sensor_msgs__msg__LaserScan__serialize(&scan, s_wire, sizeof s_wire, &n) == 0,
          "a 10-range LaserScan did not serialise");
    float ranges[4 + 4];
    for (size_t i = 0; i < 8; ++i)
        ranges[i] = -1.0f;
    sensor_msgs__msg__LaserScan small;
    sensor_msgs__msg__LaserScan__init_in(&small, &s_scan_storage);
    small.ranges.data = ranges;
    small.ranges.capacity = 4;
    rc = sensor_msgs__msg__LaserScan__deserialize(&small, s_wire, n);
    CHECK(rc != 0, "10 ranges deserialised into a capacity of 4 (rc=%d)", rc);
    for (size_t i = 0; i < 8; ++i) {
        CHECK(ranges[i] == -1.0f, "range slot %zu was written by a refused decode", i);
    }
}

/* 2. nothing allocates ------------------------------------------------------ */

static void codec_allocates_nothing(void) {
    std_msgs__msg__String msg;
    std_msgs__msg__String in;
    sensor_msgs__msg__LaserScan scan;
    sensor_msgs__msg__LaserScan scan_in;
    static sensor_msgs__msg__LaserScan__Storage scan_in_storage;
    static std_msgs__msg__String__Storage in_storage;
    size_t n = 0;

    g_allocs = 0;
    g_counting = 1;
    std_msgs__msg__String__init_in(&msg, &s_string_storage);
    std_msgs__msg__String__init_in(&in, &in_storage);
    msg.data.size = (size_t)snprintf(msg.data.data, msg.data.capacity, "Hello World: %d", 7);
    int32_t rc1 = std_msgs__msg__String__serialize(&msg, s_wire, sizeof s_wire, &n);
    int32_t rc2 = std_msgs__msg__String__deserialize(&in, s_wire, n);

    sensor_msgs__msg__LaserScan__init_in(&scan, &s_scan_storage);
    sensor_msgs__msg__LaserScan__init_in(&scan_in, &scan_in_storage);
    scan.header.frame_id.size =
        (size_t)snprintf(scan.header.frame_id.data, scan.header.frame_id.capacity, "laser");
    for (size_t i = 0; i < 16; ++i)
        scan.ranges.data[i] = 0.5f * (float)i;
    scan.ranges.size = 16;
    int32_t rc3 = sensor_msgs__msg__LaserScan__serialize(&scan, s_wire, sizeof s_wire, &n);
    int32_t rc4 = sensor_msgs__msg__LaserScan__deserialize(&scan_in, s_wire, n);
    g_counting = 0;

    CHECK(rc1 == 0 && rc2 == 0 && rc3 == 0 && rc4 == 0, "codec round trip failed (%d %d %d %d)",
          rc1, rc2, rc3, rc4);
    CHECK(strcmp(in.data.data, "Hello World: 7") == 0 && in.data.size == 14,
          "String round trip lost the payload");
    CHECK(scan_in.ranges.size == 16 && scan_in.ranges.data[15] == 7.5f &&
              strcmp(scan_in.header.frame_id.data, "laser") == 0,
          "LaserScan round trip lost the payload");
    CHECK(g_allocs == 0, "__init_in + serialize + deserialize allocated %u time(s)", g_allocs);
}

/* 3. rcl_publish -------------------------------------------------------------- */

static rclc_support_t s_support;
static rcl_node_t s_node;
static rcl_publisher_t s_publisher;
static rcl_publisher_t s_named_publisher;

static void rcl_publish_serialises_through_the_descriptor(void) {
    nros_stub_rmw_set_accept_entities(true);
    rcl_ret_t rc = rclc_support_init_rmw(&s_support, "stub://none", 46, "rosidl_message_no_alloc",
                                         NROS_STUB_RMW_NAME);
    CHECK(rc == RCL_RET_OK, "rclc_support_init_rmw: %d", rc);
    rc = rclc_node_init_default(&s_node, "talker", "", &s_support);
    CHECK(rc == RCL_RET_OK, "rclc_node_init_default: %d", rc);
    rc = rclc_publisher_init_default(&s_publisher, &s_node,
                                     ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "chatter");
    CHECK(rc == RCL_RET_OK, "rclc_publisher_init_default: %d", rc);

    std_msgs__msg__String msg;
    std_msgs__msg__String__init_in(&msg, &s_string_storage);
    msg.data.size = (size_t)snprintf(msg.data.data, msg.data.capacity, "Hello World: %d", 42);

    uint32_t before = nros_stub_rmw_publish_count();
    g_allocs = 0;
    g_counting = 1;
    rc = rcl_publish(&s_publisher, &msg, NULL);
    g_counting = 0;
    CHECK(rc == RCL_RET_OK, "rcl_publish: %d", rc);
    CHECK(g_allocs == 0, "rcl_publish allocated %u time(s)", g_allocs);
    CHECK(nros_stub_rmw_publish_count() == before + 1, "the backend saw no publish");

    size_t want_len = 0;
    CHECK(std_msgs__msg__String__serialize(&msg, s_wire, sizeof s_wire, &want_len) == 0,
          "serialize");
    size_t got_len = 0;
    const uint8_t* got = nros_stub_rmw_last_payload(&got_len);
    CHECK(got_len == want_len && memcmp(got, s_wire, want_len) == 0,
          "rcl_publish put %zu bytes on the wire; the serialiser produces %zu", got_len, want_len);

    /* allocation must be NULL: a non-NULL pointer is refused, not ignored. */
    rc = rcl_publish(&s_publisher, &msg, (rmw_publisher_allocation_t*)(uintptr_t)0x1);
    CHECK(rc == RCL_RET_INVALID_ARGUMENT, "a non-NULL allocation was accepted (%d)", rc);

    /* A descriptor built from names alone has no codec: UNSUPPORTED, loudly. */
    static const rclc_message_type_t names_only = {
        .type_name = "std_msgs::msg::dds_::String_",
        .type_hash = "",
        .serialized_size_max = 0,
        .serialize = NULL,
        .deserialize = NULL,
    };
    rc = rclc_publisher_init_default(&s_named_publisher, &s_node, &names_only, "named");
    CHECK(rc == RCL_RET_OK, "rclc_publisher_init_default (names only): %d", rc);
    rc = rcl_publish(&s_named_publisher, &msg, NULL);
    CHECK(rc == RCL_RET_UNSUPPORTED, "a codec-less descriptor published (%d)", rc);

    /* A message too long for its .msg type? String is unbounded; a too-big
     * message for the image's buffer is refused rather than truncated. */
    static char huge[8192];
    memset(huge, 'x', sizeof huge - 1);
    huge[sizeof huge - 1] = '\0';
    std_msgs__msg__String big;
    big.data.data = huge;
    big.data.size = sizeof huge - 1;
    big.data.capacity = sizeof huge;
    rc = rcl_publish(&s_publisher, &big, NULL);
    CHECK(rc == RCL_RET_ERROR, "an over-image-buffer message was not refused (%d)", rc);

    (void)rcl_publisher_fini(&s_named_publisher, &s_node);
    (void)rcl_publisher_fini(&s_publisher, &s_node);
    (void)rcl_node_fini(&s_node);
    (void)rclc_support_fini(&s_support);
}

/* Negative control: the counter is live, so a zero below is a measurement and
 * not an interposition that never took. */
static void the_counter_counts(void) {
    g_allocs = 0;
    g_counting = 1;
    void* volatile p = malloc(16);
    g_counting = 0;
    free(p);
    CHECK(g_allocs == 1, "the counting allocator is not interposed (%u)", g_allocs);
}

int main(void) {
    the_counter_counts();
    capacity_is_a_limit();
    codec_allocates_nothing();
    rcl_publish_serialises_through_the_descriptor();
    if (g_failures != 0) {
        fprintf(stderr, "rosidl_message_no_alloc: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("rosidl_message_no_alloc: capacity refused with no write past it; 0 allocations "
           "across __init_in, serialize, deserialize and rcl_publish\n");
    return 0;
}
