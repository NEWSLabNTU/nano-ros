#!/usr/bin/env python3
"""A STOCK rclpy node with one `/chatter` endpoint whose QoS has every policy
off its default, for the ROS 2 interface compatibility tests.

    qos_peer.py publisher|subscription [node_name]   (default: qos_override)

The profile is the one `qos-override-pubsub` builds with
`NROS_QOS_OVERRIDE=graph-profile` (its `GRAPH_PROFILE`); keep the two in step:

    reliability  BEST_EFFORT
    durability   TRANSIENT_LOCAL
    history      KEEP_LAST, depth 1 (publisher) / 3 (subscription)
    deadline     1.5 s
    lifespan     2 s
    liveliness   MANUAL_BY_TOPIC, lease 3 s

So every difference `ros2 topic info -v /chatter` shows between the two is a
difference between nano-ros and stock rclpy. Prints `QOS_PEER_READY` once the
endpoint exists, then spins.

Why rclpy and not a stock package: no shipped node declares a deadline, a
lifespan or a manual liveliness.
"""

import sys

import rclpy
from rclpy.duration import Duration
from rclpy.node import Node
from rclpy.qos import (
    DurabilityPolicy,
    HistoryPolicy,
    LivelinessPolicy,
    QoSProfile,
    ReliabilityPolicy,
)
from std_msgs.msg import Int32


def profile(depth):
    return QoSProfile(
        reliability=ReliabilityPolicy.BEST_EFFORT,
        durability=DurabilityPolicy.TRANSIENT_LOCAL,
        history=HistoryPolicy.KEEP_LAST,
        depth=depth,
        deadline=Duration(seconds=1, nanoseconds=500_000_000),
        lifespan=Duration(seconds=2),
        liveliness=LivelinessPolicy.MANUAL_BY_TOPIC,
        liveliness_lease_duration=Duration(seconds=3),
    )


def main():
    role = sys.argv[1] if len(sys.argv) > 1 else ""
    name = sys.argv[2] if len(sys.argv) > 2 else "qos_override"
    rclpy.init()
    node = Node(name)
    if role == "publisher":
        node.create_publisher(Int32, "/chatter", profile(1))
    elif role == "subscription":
        node.create_subscription(Int32, "/chatter", lambda _msg: None, profile(3))
    else:
        sys.exit(f"usage: qos_peer.py publisher|subscription [node_name], got {role!r}")
    print("QOS_PEER_READY", flush=True)
    rclpy.spin(node)


if __name__ == "__main__":
    main()
