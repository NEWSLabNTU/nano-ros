# Phase 480 -- what an image declares is what a stock ROS 2 peer sees

**Status (2026-10-08). ACTIVE -- W1-W7 LANDED; W8 IN FLIGHT, paused (checkpoint below).** All eleven founding
issues and the five W7 issues are resolved (#1735-#1738, #1744-#1748,
#1780-#1784). W8 holds the three issues W7's fixes filed. This is the home for wiring and interop
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

### W7 -- what W1-W6 found

Filed while fixing W1-W6, each measured or reasoned in its issue:

- **Issue 1709** (from 1687): a zenoh transient-local publisher keeps exactly
  one sample, because `TL_RETAIN_DEPTH` is a constant. So a declared
  `KEEP_LAST(N)` with N > 1 is granted 1.
- **Issue 1713** (from 1688): the default `ZPICO_MAX_LIVELINESS` (16) is short
  for a param + lifecycle image (24 tokens). Eight parameter services fail
  `declare failed (Full)` and are invisible to ROS 2.
- **Issue 1722** (from 1352): XRCE's service request buffer is not derived from
  the declared parameter shape.
- **Issue 1723** (from 1139): `timeout N ros2 ...` does not bound a waiting
  ros2 CLI, so an interop cell's peer can outlive its test.

**Acceptance:** each issue's own.

### W8 -- what W7 found

- **Issue 1732** (from 1292): a native image exits without closing its RMW
  session, so an XRCE Agent keeps its participant -- topics, services and now
  nodes -- after the image is gone.
- **Issue 1743** (from 1722): on the cargo road an XRCE image's service-server
  cap ignores the parameter and lifecycle servers, so a param + lifecycle image
  cannot boot. The `native-params-25-rust-xrce` fixture row states
  `NROS_XRCE_MAX_SERVICE_SERVERS=11` until it is fixed.
- **Issue 1741** (from 1723): a threadx-linux image catches SIGTERM and keeps
  running, so `timeout N` does not bound it. A harness defect of the same class
  1723 fixed for the `ros2` CLI.

**Acceptance:** each issue's own.

**Checkpoint (2026-10-08, work paused).** Claims `issue-1732`, `issue-1741`
and `issue-1743` are held until about 2026-10-10 (`just claim-list`).

| issue | state | to resume |
| --- | --- | --- |
| 1741 | branch `fix/1741-image-ends-on-sigterm`, 5 commits, issue archived; files 1750 and 1752. Draft PR #1805 | review (the fix must keep the ThreadX port's own scheduler signals); rebase; tier 2 (the agent's run was cut off); mark ready |
| 1743 | branch `fix/1743-xrce-infra-service-cap`, 1 code commit; `just check fast` green | `just check test-targets` (1.99), `cli-tests`, `rmw-xrce`, tier 2; drop the stated `NROS_XRCE_MAX_SERVICE_SERVERS=11` from row `workspace-features-rust-params-xrce`; archive commit; PR |
| 1732 | not started | every exit path of a native image (clean end, error, SIGTERM/SIGINT) closes its RMW session; check zenoh and Cyclone for the same gap; prove `ros2 node list` empty after exit |

Before resuming a branch, compare it with origin (`git ls-remote`): another
session's PR sweep also rebases and fixes PRs.

## Status of the resolved issues

| item | issue | PR |
| --- | --- | --- |
| W1 | 1687 declared QoS on the wire | #1735 |
| W1 | 1688 typed `[lifecycle]` block sizes the pools | #1736 |
| W2 | 1686 XRCE wait-for-server returns UNSUPPORTED | #1737 |
| W3 | 1691 + 1342 action gate skips the ros2cli daemon | #1738 |
| W3 | 1292 XRCE `ros_discovery_info` (GUID probe + counted keys) | #1782 |
| W4 | 1293 empty-struct padding byte, every RMW and language | #1744 |
| W4 | 1291 two clients vs a stock server | #1745 |
| W5 | 1352 parameter inbox never overflows silently | #1746 |
| W6 | 1139 Cyclone pubsub holds in-gate | #1747 |
| W6 | 1251 tag-isolated slirp profile | #1748 |
| W7 | 1723 every ros2 CLI deadline escalates to SIGKILL | #1780 |
| W7 | 1713 liveliness pool completed from declared facts | #1781 |
| W7 | 1709 + 1740 transient-local history follows the declared depth | #1783 |
| W7 | 1722 XRCE parameter request priced against every buffer | #1784 |
