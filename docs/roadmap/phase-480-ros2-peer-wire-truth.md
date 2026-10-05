# Phase 480 -- what an image declares is what a stock ROS 2 peer sees

**Status (2026-10-06). IN PROGRESS -- W1-W3 being worked (issues 1686, 1687,
1688, 1691 claimed); W4-W6 queued.** This is the home for wiring and interop
defects: an entity, a QoS profile, a remap, a node or a service that nano-ros
DECLARES, but a stock ROS 2 peer (`ros2` CLI, `rclcpp`, `rmw_zenoh_cpp`,
`rmw_cyclonedds_cpp`, `rmw_fastrtps_cpp` through the XRCE Agent) does not see,
or sees wrongly. New issues of that class should name this phase in
`related:`, and gain a work item here.

**Prior:** issue 1651 (the tier-1 `test-all` had not run in CI since
2026-06-17, which is why W1-W3 surfaced together on 2026-10-05), issue 1269
(one graph node per component on zenoh and Cyclone), issue 1333 (the ros2cli
daemon serves its starter's RMW), issue 1009/1137 (loopback-pinned DDS
interop pairs), phase-455 (live-peer cells), phase-441 (RMW on-target
verification).

## Scope

In: a defect visible from a stock ROS 2 peer, or in the harness that asks one,
and reproducible on a host -- native images, and QEMU guests through user-mode
networking.

Out, with their homes:

- **Defects that need a physical board or a serial link.** Issue 0902 (action
  goals completing 20-90 % on a serial-linked board) stays with its board phase.
- **Embedded images that fail before the wire** (boot, transport bring-up,
  link) -- issues 0997, 1004, 0917, 1281, 1363, 1535. A cell that never reaches
  the wire is a platform defect, not an interop one.
- **Wire-format encoding** (XCDR2, extensibility) -- phase-303, parked.
- **Constrained-link zenoh behaviour** (graph burst, latched reads) --
  phase-473.

## Work items

### W1 -- per-entity configuration reaches the live entity

- **Issue 1687.** A workspace publisher advertises `KEEP_LAST(1)` +
  `TRANSIENT_LOCAL` whatever its code (`KEEP_LAST(10)`) or its plan
  (`qos_overrides.<topic>.publisher.reliability`) declares. C, C++ and mixed QoS
  cells are red, and so is `qos_override_e2e`.
- **Issue 1688.** A Rust workspace entry's launch `<remap>` never reaches the
  wire.

Both are "the model says X, the entity is created with Y"; they take different
paths (`runtime.remaps` vs the QoS profile), so they share this item only if a
diagnosis joins them.

**Acceptance:**

- The four QoS cells and `case_03_rust_remap` pass solo and in the tier-1
  `test-all`.
- `ros2 topic info -v` shows the declared profile.
- A negative control shows each test failing on the pre-fix tree.

### W2 -- request/response over XRCE from C and C++

- **Issue 1686.** Every C/C++ XRCE service and action round-trip fails, and the
  server prints nothing (`test_c_xrce_action_fibonacci`, reqresp cases 09, 17,
  18). XRCE pub/sub passes in the same files.

**Acceptance:** the four tests pass solo against the pinned Agent
(`2.4.3-nros1`), and in `test-all`.

### W3 -- the graph a stock tool reads

- **Issue 1691.** A stock `/fibonacci` server is never seen by `ros2 action
  list` over zenoh.
- **Issue 1342.** The fixed-domain zenoh action cells inherit a foreign ros2cli
  daemon (issue 1333's remedy covers unique domains only). Measured 2026-09-12:
  `--no-daemon` queries see the server and the daemon-backed `ros2 action
  list` does not. Same failure as 1691, so it is one item.
- **Issue 1292.** An XRCE image publishes no `ros_discovery_info`, so `ros2 node
  list` shows none of its nodes. Reasoned from source, not yet measured live --
  measure first.

**Acceptance:**

- `ros2_action_e2e` passes on a host with the peer installed, on a fixed domain
  as well as a unique one.
- `ros2 node list` against an XRCE image lists its components, matching zenoh
  and Cyclone (issue 1269's requirement), or the issue records a measured
  reason it cannot.

### W4 -- Cyclone services a stock peer can call

- **Issue 1293.** On the Rust Cyclone path an EMPTY message has no descriptor
  (`BuildError::EmptySchema`). So the lifecycle services, `std_srvs/Trigger` and
  `std_srvs/Empty` cannot be created. ROS pads an empty struct with
  `uint8 structure_needs_at_least_one_member`, and the C/C++ IDL route already
  does.
- **Issue 1291.** Two clients of one service in one process shared a reply id.
  The fix (`writer_request_id64`) is on main, with `service_request_slots_exhausted`
  as its regression test. What the issue records as NOT covered is interop with
  a stock ROS 2 server, so this item is that run, then the archive.

**Acceptance:**

- A Rust Cyclone image serves the lifecycle services, and `ros2 service call`
  on an empty-request service gets a reply.
- Two in-process clients against a stock `rclcpp` server each receive only
  their own replies.

### W5 -- a declared service fits its buffer

- **Issue 1352.** One flat `SERVICE_BUFFER_SIZE` serves every queryable.
  `set_parameters` for a node's declared parameters outgrows it, and the request
  is dropped with no build-time warning, even though both inputs are DERIVED.

**Acceptance:** the buffer is derived from the largest declared service request
(or the build refuses with the number it needs), and a `ros2 param set` on the
25-parameter case lands.

### W6 -- the interop harness answers reliably

- **Issue 1139.** `nros_rmw_cyclonedds_ros2_pubsub_e2e` fails under the 21-way
  parallel gate and passes solo, on alternating sub-cases. That is contention,
  not a broken direction.
- **Issue 1251.** A Cyclone peer for a QEMU guest cannot be loopback-pinned:
  the isolation profile and the slirp NAT rewrite are mutually exclusive.

**Acceptance:**

- 1139 holds in-gate over N repeated gate runs, with N and the result recorded.
- A QEMU-guest Cyclone pair has an isolation profile that both pins the host
  half and is reachable from the guest, or the harness refuses that pair by
  name.

## Ownership

Claims live on origin (`just claim-list`). As of 2026-10-06:

- 1686, 1687, 1688 and 1691 are claimed and in progress.
- 1291, 1292, 1293, 1342, 1352, 1139 and 1251 are claimed by the same
  coordinator and queued behind them.
