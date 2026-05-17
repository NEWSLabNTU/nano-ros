/**
 * @file main.cpp
 * @brief Phase 117.14 — Zephyr C++ pub/sub demo on the ARM FVP
 *        Base_RevC AEMv8-R Cortex-A SMP board, exercising `nros-cpp`
 *        + the codegen pipeline end-to-end.
 *
 * Publishes `std_msgs/Int32 { data = count }` on `/chatter` once a
 * second. Pair with a stock ROS 2 `ros2 topic echo /chatter
 * std_msgs/msg/Int32` peer for the runtime interop check, or with
 * the matching `examples/zephyr/cpp/dds/listener` running on
 * native_sim for an in-tree round-trip.
 *
 * The Cyclone DDS RMW lands on POSIX today (Phase 117.12); this
 * example builds against the Zephyr nros module's existing DDS
 * backend (`CONFIG_NROS_RMW_DDS=y`, dust-dds). Swap to Cyclone once
 * the Zephyr build glue at `packages/dds/nros-rmw-cyclonedds/` is
 * extended to ship a Zephyr module (tracked separately).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include <nros/nros.hpp>

#include "std_msgs.hpp"

LOG_MODULE_REGISTER(nros_aemv8r_cyclonedds_talker, LOG_LEVEL_INF);

int main(void)
{
    LOG_INF("Phase 117.14 — nros C++ talker on FVP AEMv8-R");

    nros::Result ret = nros::init("", CONFIG_NROS_DOMAIN_ID);
    if (!ret.ok()) {
        LOG_ERR("Init failed: %d", ret.raw());
        return 1;
    }

    nros::Node node;
    ret = nros::create_node(node, "aemv8r_cyclonedds_talker");
    if (!ret.ok()) {
        LOG_ERR("Node creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    nros::Publisher<std_msgs::msg::Int32> pub;
    ret = node.create_publisher(pub, "/chatter");
    if (!ret.ok()) {
        LOG_ERR("Publisher creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    LOG_INF("Publishing /chatter (std_msgs/Int32) at 1 Hz...");

    int32_t count = 0;
    while (true) {
        ++count;
        std_msgs::msg::Int32 msg;
        msg.data = count;

        ret = pub.publish(msg);
        if (ret.ok()) {
            LOG_INF("Published: %d", count);
        } else {
            LOG_ERR("Publish failed: %d", ret.raw());
        }
        k_sleep(K_SECONDS(1));
    }

    nros::shutdown();
    return 0;
}
