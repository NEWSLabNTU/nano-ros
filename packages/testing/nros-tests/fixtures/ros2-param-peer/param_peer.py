#!/usr/bin/env python3
"""A STOCK rclpy node declaring the parameters of `param-two-node-talker`'s
`alpha` node, for the ROS 2 interface compatibility tests.

    param_peer.py [node_name]      (default: param_peer)

Declares exactly what `alpha` declares, with the same descriptors:

    rate     integer, 10
    stepped  integer, 10, range 0..100 step 5, and the same description

and nothing else of its own, so every difference `ros2 param` shows between
the two is a difference between nano-ros and stock rclpy. Prints
`PARAM_PEER_READY` once the parameters are declared, then spins.

Why rclpy and not a stock package: no shipped node declares an integer range
with a step, which is the descriptor `ros2 param describe` and the off-step
refusal are compared on.
"""

import sys

import rclpy
from rcl_interfaces.msg import IntegerRange, ParameterDescriptor
from rclpy.node import Node


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "param_peer"
    rclpy.init()
    node = Node(name)
    node.declare_parameter("rate", 10)
    node.declare_parameter(
        "stepped",
        10,
        ParameterDescriptor(
            description="step-5 lattice; an off-step `ros2 param set` must be refused",
            integer_range=[IntegerRange(from_value=0, to_value=100, step=5)],
        ),
    )
    print("PARAM_PEER_READY", flush=True)
    rclpy.spin(node)


if __name__ == "__main__":
    main()
