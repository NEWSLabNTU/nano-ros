/*
 * RFC-0132 / phase-560 W9.e -- the cmake road's CAN device delivery probe.
 *
 * Opens the image's CAN device (`open(NULL)`) and prints which device the
 * entry handed over and what `open` returned; on success it sends one classic
 * frame (0x263, 8 bytes) that `scripts/test/can-image-device.sh` reads back
 * with `candump`. The same lines as the cargo road's probe.
 *
 * Then it brings up a node with one timer and spins, so the image is a ROS program its host
 * census can count (a cross board's C entry is sized by its census, and that
 * census build links the CAN mock). With no router the bring-up fails after
 * the CAN lines are out, and the probe exits 0: the lines are its verdict.
 */

#include <stdio.h>

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/timer.h>

#include <nros/platform_can.h>

/* The board's store of the image's device (RFC-0132 D4), which the backend's
 * `open(NULL)` reads. Board-internal; the probe reads it only to report the
 * device it opened. */
extern const char* nros_board_can_image_device(void);

static void on_tick(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
}

static int ros_spin(int argc, char** argv) {
    static rclc_support_t support;
    static rcl_node_t node;
    static rclc_executor_t executor;
    static rcl_timer_t timer;
    rcl_allocator_t allocator = rcl_get_default_allocator();

    if (rclc_support_init(&support, argc, argv, &allocator) != RCL_RET_OK ||
        rclc_executor_init(&executor, &support.context, 1, NULL) != RCL_RET_OK ||
        rclc_executor_node_init(&executor, &node, "can_image_probe", NULL) != RCL_RET_OK ||
        rclc_timer_init_default(&timer, &support, 1000000000ULL, on_tick) != RCL_RET_OK ||
        rclc_executor_add_timer(&executor, &timer) != RCL_RET_OK) {
        printf("CAN-IMAGE: no ROS session (no router); done\n");
        return 0;
    }
    (void)rclc_executor_spin_period(&executor, 100000000ULL);
    return 0;
}

static void can_probe(void) {
    static nros_platform_can_t can;
    nros_platform_can_frame_t frame = {
        .id = 0x263u,
        .flags = 0u,
        .len = 8u,
        .data = {0x26, 0x33, 0x0e, 0x00, 0xca, 0xfe, 0x01, 0x02},
    };
    const char* dev = nros_board_can_image_device();
    int8_t rc;

    if (dev == NULL) {
        dev = "-";
    }
    rc = nros_platform_can_open(&can, NULL, 0u);
    printf("CAN-IMAGE: device=%s open(NULL)=%d\n", dev, (int)rc);
    if (rc != NROS_PLATFORM_CAN_OK) {
        fflush(stdout);
        return;
    }
    rc = nros_platform_can_send(&can, &frame, 100000u);
    if (rc == NROS_PLATFORM_CAN_OK) {
        printf("CAN-IMAGE: sent 0x263 on %s\n", dev);
    } else {
        printf("CAN-IMAGE: send failed: %d\n", (int)rc);
    }
    nros_platform_can_close(&can);
    fflush(stdout);
}

int main(int argc, char** argv) {
#ifdef _IOLBF
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    can_probe();
    return ros_spin(argc, argv);
}
