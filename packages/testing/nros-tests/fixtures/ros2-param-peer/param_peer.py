#!/usr/bin/env python3
"""A STOCK rclpy node declaring the parameters of `param-two-node-talker`'s
`alpha` node, for the ROS 2 interface compatibility tests.

    param_peer.py [node_name] [profile]   (defaults: param_peer, alpha)

The default profile, `alpha`, declares exactly what `alpha` declares, with
the same descriptors:

    rate     integer, 10
    stepped  integer, 10, range 0..100 step 5, and the same description

and nothing else of its own, so every difference `ros2 param` shows between
the two is a difference between nano-ros and stock rclpy. Prints
`PARAM_PEER_READY` once the parameters are declared, then spins.

The other profiles declare what the C and C++ parameter examples
(`examples/linux/{c,cpp}/parameters`) hold once their local roundtrip is done,
with the same descriptors and the values their own sets left:

    c_parameters    verbose true, publish_rate_hz 10 (range 1..100 step 1),
                    scale_factor 1.0, topic_name "/rosout" (read-only)
    cpp_parameters  verbose false, max_iters 100, ctrl_period 0.05,
                    frame_id "map", mpc_weights [4.0, 5.0, 6.0, 7.0]

Why rclpy and not a stock package: no shipped node declares an integer range
with a step, which is the descriptor `ros2 param describe` and the off-step
refusal are compared on.
"""

import sys

import rclpy
from rcl_interfaces.msg import IntegerRange, ParameterDescriptor
from rclpy.node import Node


def declare_alpha(node):
    node.declare_parameter("rate", 10)
    node.declare_parameter(
        "stepped",
        10,
        ParameterDescriptor(
            description="step-5 lattice; an off-step `ros2 param set` must be refused",
            integer_range=[IntegerRange(from_value=0, to_value=100, step=5)],
        ),
    )


def declare_c_parameters(node):
    node.declare_parameter("verbose", True)
    node.declare_parameter(
        "publish_rate_hz",
        10,
        ParameterDescriptor(integer_range=[IntegerRange(from_value=1, to_value=100, step=1)]),
    )
    node.declare_parameter("scale_factor", 1.0)
    node.declare_parameter("topic_name", "/rosout", ParameterDescriptor(read_only=True))


def declare_cpp_parameters(node):
    node.declare_parameter("verbose", False)
    node.declare_parameter("max_iters", 100)
    node.declare_parameter("ctrl_period", 0.05)
    node.declare_parameter("frame_id", "map")
    node.declare_parameter("mpc_weights", [4.0, 5.0, 6.0, 7.0])


PROFILES = {
    "alpha": declare_alpha,
    "c_parameters": declare_c_parameters,
    "cpp_parameters": declare_cpp_parameters,
}


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "param_peer"
    profile = sys.argv[2] if len(sys.argv) > 2 else "alpha"
    rclpy.init()
    node = Node(name)
    PROFILES[profile](node)
    print("PARAM_PEER_READY", flush=True)
    rclpy.spin(node)


if __name__ == "__main__":
    main()
