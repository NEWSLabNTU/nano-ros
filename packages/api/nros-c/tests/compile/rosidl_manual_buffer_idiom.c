/*
 * phase-492 W7 (RFC-0089 "Messages take rosidl's C names AND its member
 * shape") — the micro-ROS manual-buffer idiom compiles UNCHANGED.
 *
 * The body below is what a micro-ROS application writes to give a message's
 * strings and sequences memory without an allocator: point `data` at a
 * buffer it owns and say how big it is. It names rosidl's struct
 * (`std_msgs__msg__String`), rosidl's include path (`<std_msgs/msg/string.h>`)
 * and rosidl's member shape (`msg.data.data` / `.size` / `.capacity`), so
 * before W7 not one of its lines compiled here. The headers it includes are
 * the committed codegen goldens of the stock Humble interfaces
 * (`rosidl-codegen/tests/fixtures/fingerprint-corpus/expected/rosidl/`), so a
 * pack edit that changes the generated shape fails this lane in seconds.
 *
 * Compile-only (no main), `-Wall -Wextra -Werror`.
 */

#include <sensor_msgs/msg/laser_scan.h>
#include <std_msgs/msg/string.h>

#include <stdio.h>
#include <string.h>

static char string_buffer[64];

static char scan_frame_id[32];
static float scan_ranges[360];
static float scan_intensities[360];

void prepare_string(std_msgs__msg__String* msg);
void prepare_string(std_msgs__msg__String* msg) {
    msg->data.data = string_buffer;
    msg->data.size = 0;
    msg->data.capacity = sizeof(string_buffer);

    msg->data.size = (size_t)snprintf(msg->data.data, msg->data.capacity, "Hello World: %d", 1);
}

void prepare_scan(sensor_msgs__msg__LaserScan* scan);
void prepare_scan(sensor_msgs__msg__LaserScan* scan) {
    scan->header.frame_id.data = scan_frame_id;
    scan->header.frame_id.capacity = sizeof(scan_frame_id);
    strcpy(scan->header.frame_id.data, "laser");
    scan->header.frame_id.size = strlen(scan->header.frame_id.data);
    scan->header.stamp.sec = 0;
    scan->header.stamp.nanosec = 0;

    scan->ranges.data = scan_ranges;
    scan->ranges.size = 0;
    scan->ranges.capacity = sizeof(scan_ranges) / sizeof(scan_ranges[0]);

    scan->intensities.data = scan_intensities;
    scan->intensities.size = 0;
    scan->intensities.capacity = sizeof(scan_intensities) / sizeof(scan_intensities[0]);

    for (size_t i = 0; i < scan->ranges.capacity; ++i) {
        scan->ranges.data[i] = 1.0f;
    }
    scan->ranges.size = scan->ranges.capacity;
    scan->angle_min = -3.14f;
    scan->angle_max = 3.14f;
}

/* The same messages wired by the generated helper instead — nano-ros's default,
 * and the ONE way in which this file is not ported code. */
static std_msgs__msg__String__Storage string_storage;
static sensor_msgs__msg__LaserScan__Storage scan_storage;

void prepare_with_storage(std_msgs__msg__String* msg, sensor_msgs__msg__LaserScan* scan);
void prepare_with_storage(std_msgs__msg__String* msg, sensor_msgs__msg__LaserScan* scan) {
    std_msgs__msg__String__init_in(msg, &string_storage);
    sensor_msgs__msg__LaserScan__init_in(scan, &scan_storage);
}

/* A sequence of messages in rosidl's shape exists too. */
typedef std_msgs__msg__String__Sequence string_sequence_t;
