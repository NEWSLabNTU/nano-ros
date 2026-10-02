#!/usr/bin/env python3
"""A STOCK ROS 2 subscriber that prints each sample's publisher gid — issue 1495.

    message_info_peer.py <topic> [timeout_s]

Prints one line per received sample:

    PEER_TAKE_GID <48 hex digits>

i.e. `rmw_message_info_t::publisher_gid.data`, all 24 bytes, exactly as the
RMW wrote it. Exits 0 after the first sample, 3 on timeout.

Why this is not `ros2 topic echo`: rclpy on Humble does not expose the
publisher gid at all — its take hands back source and received timestamps and
nothing else — so no stock CLI can show what a subscriber's RMW read off a
sample. This script owns an ordinary rclpy subscription (so the node, the
subscription and its QoS are what any rclpy user gets) and takes from it with
`rcl_take` directly, which is the same call rclpy makes, with a
`rmw_message_info_t` it can read. Nothing on the data path is ours: on zenoh
the gid is what `rmw_zenoh_cpp` parsed out of the attachment, on Cyclone what
`rmw_cyclonedds_cpp` derived from the sample info.

The executor never spins, so rclpy's own callback never races us for samples.
"""

import ctypes
import sys
import time

import rclpy
from rclpy.node import Node
from std_msgs.msg import String


class RmwGid(ctypes.Structure):
    _fields_ = [("implementation_identifier", ctypes.c_char_p), ("data", ctypes.c_uint8 * 24)]


class RmwMessageInfo(ctypes.Structure):
    # Humble `rmw/types.h`: two int64 timestamps, two uint64 sequence numbers,
    # the gid, the intra-process flag.
    _fields_ = [
        ("source_timestamp", ctypes.c_int64),
        ("received_timestamp", ctypes.c_int64),
        ("publication_sequence_number", ctypes.c_uint64),
        ("reception_sequence_number", ctypes.c_uint64),
        ("publisher_gid", RmwGid),
        ("from_intra_process", ctypes.c_bool),
    ]


class RosString(ctypes.Structure):
    # `rosidl_runtime_c__String` — the C message `std_msgs/msg/String` is.
    _fields_ = [("data", ctypes.c_char_p), ("size", ctypes.c_size_t), ("capacity", ctypes.c_size_t)]


RCL_RET_OK = 0
RCL_RET_SUBSCRIPTION_TAKE_FAILED = 401


def main() -> int:
    topic = sys.argv[1]
    timeout_s = float(sys.argv[2]) if len(sys.argv) > 2 else 30.0
    rclpy.init()
    node = Node("message_info_peer")
    sub = node.create_subscription(String, topic, lambda _msg: None, 10)
    rcl = ctypes.CDLL("librcl.so")
    rcl.rcl_take.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
    rcl.rcl_take.restype = ctypes.c_int
    print("PEER_READY", flush=True)

    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        msg = RosString()
        info = RmwMessageInfo()
        rc = rcl.rcl_take(sub.handle.pointer, ctypes.byref(msg), ctypes.byref(info), None)
        if rc == RCL_RET_OK:
            gid = bytes(info.publisher_gid.data).hex()
            data = msg.data.decode(errors="replace") if msg.data else ""
            print(f"PEER_TAKE_GID {gid}", flush=True)
            print(f"PEER_TAKE_DATA {data}", flush=True)
            return 0
        if rc != RCL_RET_SUBSCRIPTION_TAKE_FAILED:
            print(f"PEER_TAKE_ERROR rc={rc}", flush=True)
            return 2
        time.sleep(0.05)
    print("PEER_TIMEOUT no sample", flush=True)
    return 3


if __name__ == "__main__":
    sys.exit(main())
