"""A stock rclpy publisher for the oversized-relay cell (issue 2978).

Publishes std_msgs/String on a topic, RELIABLE, once a subscriber has matched:
one small sample, one whose data is `big` characters, then `after` small ones.
An XRCE Agent relaying this to a client cannot carry a DATA submessage over
65,535 bytes. The patched Agent drops it with a log line, and the small
samples after it must still arrive.

Usage: relay_peer.py <topic> <big> <after>
"""
import sys
import time

import rclpy
from rclpy.qos import HistoryPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

topic, big, after = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])

rclpy.init()
node = rclpy.create_node("oversized_relay_peer")
qos = QoSProfile(depth=10, reliability=ReliabilityPolicy.RELIABLE,
                 history=HistoryPolicy.KEEP_LAST)
pub = node.create_publisher(String, topic, qos)

deadline = time.time() + 20
while pub.get_subscription_count() == 0 and time.time() < deadline:
    rclpy.spin_once(node, timeout_sec=0.1)
print(f"relay_peer: subscribers={pub.get_subscription_count()}", flush=True)

sizes = [8, big] + [8] * after
for seq, size in enumerate(sizes):
    pub.publish(String(data=("x" * size) if size > 8 else f"small {seq}"))
    print(f"relay_peer: published seq={seq} chars={size}", flush=True)
    time.sleep(0.5)

# Leave time for the last samples to drain before the writer goes away.
time.sleep(2)
node.destroy_node()
rclpy.shutdown()
