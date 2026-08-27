---
id: 855
title: "c_port_posix_net hardcodes ports INSIDE the ephemeral range, so any process on the host can red it"
status: resolved
severity: low
area: testing
filed: 2026-08-28
resolved: 2026-08-28
---

## Symptom

`just check` red, one test, no code change near it:

```
FAIL [ 0.008s] nros-platform-cffi::c_port_posix_net udp_loopback_roundtrip
  assertion `left == right` failed
    left: -1
   right: 0
  c_port_posix_net.rs:153
```

`-1` is `nros_platform_udp_listen` failing to bind.

## Cause

The two tests name their ports as literals:

```rust
let port = b"56301\0";   // tcp_loopback_roundtrip
let port = b"56302\0";   // udp_loopback_roundtrip
```

Both are inside this host's ephemeral range:

```
$ cat /proc/sys/net/ipv4/ip_local_port_range
32768	60999
```

So the kernel hands those exact numbers out to any process that asks for
an ephemeral port. Nothing has to be *wrong* for the test to fail — some
unrelated process on the machine has to get unlucky. When this was found,
the holder was an unrelated ROS `component_node` from another agent's
`play_launch` run:

```
$ ss -lunp | grep 56302
UNCONN 127.0.0.1:56302  users:(("component_node",pid=1855814,fd=10))
```

Two properties make this worse than an ordinary flake:

* **It reads as a product failure.** The assertion is on a platform-port
  return code, so the message says the UDP port cannot bind — which is
  true, and says nothing about why.
* **It is not reproducible on demand and not reproducible away either.**
  The window is however long the other process lives. This one had been up
  29 minutes.

This is the port-shaped sibling of the domain-id rule the tree already
follows: `nros_tests::unique_ros_domain_id()` exists precisely because a
baked-in domain collides with whatever else is running.

## Fix

Take the port from the kernel instead of naming one: bind a throwaway
`UdpSocket`/`TcpListener` on port 0, read the assigned port back with
`local_addr()`, drop it, and hand that number to the C port under test.

The kernel will not hand the same ephemeral port to two live sockets, so
the only remaining window is between the probe socket closing and the C
port binding — microseconds, against the previous window of "as long as
any process on the host holds 56302". `SO_REUSEADDR` on the probe is
deliberately NOT set: the point is to be told a port nobody holds.

A fixed port outside the ephemeral range was the other candidate and is
worse: it trades a rare collision with any process for a permanent
collision with anything else that also picks that constant, and the tree
has no port registry to check against.

## Verification

`ss -lunp | grep <port>` while the test runs shows a different port on
each invocation. Both tests in `c_port_posix_net.rs` pass with the
autoware `component_node` still holding 56302.
