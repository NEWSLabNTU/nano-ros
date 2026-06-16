# Cyclone DDS RMW — Known Limitations

Phase 117 ships `nros-rmw-cyclonedds` as the fourth RMW backend
alongside `rmw-zenoh`, `rmw-xrce`, and `rmw-dds`. The implementation
is **complete enough for nano-ros ↔ nano-ros pub/sub + service
round-trips on POSIX**, but several pieces are explicitly deferred.
This page tracks them so consumers (Autoware safety-island, future
follow-up phases) plan around them rather than rediscovering them at
integration time.

## Pin

- Submodule pinned to **tag `0.10.5`** (`third-party/dds/cyclonedds/`).
- Wire-compatible with `ros-humble-cyclonedds` 0.10.5 +
  `ros-humble-rmw-cyclonedds-cpp` 1.3.4.
- Upgrading the pin requires matching the consumer's ROS 2
  distribution; Cyclone does not commit to wire compat across `0.x`
  minor releases.

## Data plane: 2× CDR round-trip per message

`publish_raw` and `try_recv_raw` deserialize the runtime's CDR bytes
into a typed sample buffer via `dds_stream_read_sample`, then call
`dds_write` (which re-serializes onto the wire). Take is the inverse.
See `src/sertype_min.{hpp,cpp}` for the rationale.

**Why:** Cyclone 0.10.5's public API does not expose
`dds_writer_lookup_serdatatype()` — there is no way to fish a
`ddsi_sertype *` out of a writer/reader, so the zero-copy path
(`ddsi_serdata_from_ser_iov` + `dds_writecdr`) is unreachable
without vendoring private headers.

**Cost:** one extra encode + decode on each side of every message.
Acceptable for control-loop rates (≤1 kHz) on safety MCUs;
unacceptable for high-throughput streams (camera frames, lidar
scans).

**Path forward:** wait for upstream to expose the writer→sertype
lookup (currently being discussed in
[cyclonedds#1342](https://github.com/eclipse-cyclonedds/cyclonedds/issues/1342))
or vendor the small subset of `dds/ddsi/ddsi_serdata_default.h` +
`ddsi_domaingv.h` needed to reach `gv->serpool`.

## Phase 108 status events: NULL slots

The vtable's three event hooks are NULL:

```c
.register_subscriber_event   = NULL,
.register_publisher_event    = NULL,
.assert_publisher_liveliness = NULL,
```

Liveliness changes, deadline misses, and message-lost events are
**not delivered to the runtime** even though Cyclone tracks them
internally via `dds_set_listener`. Apps that rely on
`add_subscriber_event_callback` will silently see no firings on
this backend.

**Path forward:** wire Cyclone's reader/writer listener trampolines
through to `nros_rmw_event_callback_t` in a separate phase. Each
status callback maps cleanly to one event kind; ~150 LOC.

## Service request-id correlation — done (Phase 117.7.B)

Service traffic now wraps every Request and Reply in a
backend-defined envelope:

```idl
struct ServiceEnvelope {
    unsigned long long client_id;
    long long          seq;
    sequence<octet>    payload;
};
```

Each `service_call_raw` stamps a unique random `client_id` (per
client, allocated at create time) plus a monotonic `seq` (per
client, atomic). The server stores `(client_id, client_seq)` in a
32-slot table when it takes a request, returns the slot index as
the runtime-visible `seq`, and echoes the original
`(client_id, seq)` on the matching `service_send_reply`. The
client's `service_call_raw` poll filters incoming replies on
`(client_id, seq) == (mine, my_seq)`. Concurrent calls from
parallel clients no longer interleave.

`service_concurrent` CTest fires 5 calls from each of two parallel
clients against one server and asserts each client receives only
its own replies in order.

**Wire compat — done (Phase 117.12.B).** The interim
`ServiceEnvelope` was replaced by stock `rmw_cyclonedds_cpp`'s
`cdds_request_header_t` (`{uint64_t guid; int64_t seq;}`, 16 bytes,
see upstream `src/serdata.hpp:73-77`). Codegen now injects
`unsigned long long rmw_writer_guid; long long rmw_sequence_number;`
into every `_Request_` / `_Response_` IDL struct, and the
backend's `(build|split)_wire_header` helpers serialise / parse the
matching 16-byte header at the front of each CDR. Bidirectional
interop validated by `nros_rmw_cyclonedds_ros2_srv_e2e` against
`ros2 service call` and `ros2 run demo_nodes_cpp
add_two_ints_server`.

**Caveat — cap:** the server-side slot table is fixed at 32. A
server with more than 32 outstanding requests will report
`NROS_RMW_RET_WOULD_BLOCK` from `try_recv_request` until the
application drains via `send_reply`. Tune by editing
`kRequestSlots` in `src/service.cpp`.

**Caveat — Cyclone same-participant local-delivery race:**
creating two service clients on the same `nros_rmw_session_t`
back-to-back occasionally results in only the second writer
matching the server's reader (Cyclone 0.10.5 local-delivery
shortcut). Stagger client creation by ≥ 100 ms, or move to one
participant per service client.

**Caveat — `service_concurrent` test disabled by default
(Phase 117.X.5).** With the per-client-participant workaround
in place, cross-participant SEDP discovery on POSIX still
consistently drops the last reply on one of the two clients
(Cyclone 0.10.5). The `(writer_guid, seq)` filter logic is
functionally validated by `service_roundtrip` (single client,
single call) and `mangling_test` (descriptor + type-name
correctness). The concurrent harness can be re-enabled with
`-DNROS_RMW_CYCLONEDDS_RUN_SERVICE_CONCURRENT=ON` for local
investigation; closing the gap likely requires explicit
publication-matched-status polling in
`service_client_create` and is tracked separately.

## ROS 2 wire interop — done (Phase 117.12)

POSIX E2E against stock ROS 2 nodes on the same domain is
validated bidirectionally by two CTest harnesses
(`nros_rmw_cyclonedds_ros2_pubsub_e2e`,
`nros_rmw_cyclonedds_ros2_srv_e2e`):

- **Pub/sub:** nano-ros publisher ↔ `ros2 topic echo /chatter`
  (byte-equal `std_msgs/msg/String` payload) and `ros2 topic pub`
  ↔ nano-ros subscriber.
- **Services:** nano-ros server ↔ `ros2 service call /add_two_ints
  example_interfaces/srv/AddTwoInts` and stock `ros2 run
  demo_nodes_cpp add_two_ints_server` ↔ nano-ros client.

The harness picks a multicast-capable ethernet interface
(auto-detect, override via `NROS_RMW_CYCLONEDDS_E2E_IFACE`) and
writes a per-test `CYCLONEDDS_URI` config so SPDP works on hosts
where `lo` is non-multicast. Both harnesses skip cleanly with
`[SKIPPED]` if `/opt/ros/humble/setup.bash`, the `ros2` CLI, or a
suitable interface is missing.

## QoS coverage

`make_dds_qos` honours the full `nros_rmw_qos_t` field set
(reliability, durability, history+depth, deadline, lifespan,
liveliness+lease) **except**:

- `MANUAL_BY_NODE` liveliness — folded to `MANUAL_BY_TOPIC` (Cyclone
  has no node-scoped variant).
- `max_blocking_time` on reliable writers — hard-coded to 100 ms
  to match `rmw_cyclonedds_cpp`. Surfacing it through
  `nros_rmw_qos_t._reserved` is a follow-up.

## Type discovery (XTypes metadata) — Phase 117.X.6 opt-in

By default the codegen helper passes `idlc -t` which **omits the
XTypes type-information section** from the generated descriptor.

**Why.** Cyclone 0.10.5's `idlc` segfaults emitting type-info on
**any** input — verified with the trivial `@final struct Simple {
long x; };` (runs `idlc -l c`, prints `Failed to compile`, output
`.c` is truncated mid-ops-array, no descriptor emitted). The bug
is independent of our IDL shape. Tag `0.10.5` is the latest patch
on the upstream `0.10.*` branch (no `0.10.6`).

**Why we default to `-t`.** Type-info is optional on the wire —
peers fall back to typename matching, which is what nano-ros
publishers / subscribers / services already use end-to-end. Stock
ROS 2 `rclcpp` apps interop fine. Only `ros2 topic info -v` (which
queries `DCPSPublication` / `DCPSSubscription` builtin topics for
XTypes metadata) shows blank type info for nano-ros endpoints.

**Opt-in once the upstream bug is fixed.** Set
`-DNROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO=ON` (cmake cache var) or
`NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO=1` (env). The cmake helper
runs an idlc probe at configure time against a synthetic minimal
IDL — if Cyclone still has the type-info bug the configure errors
out with a pointer to this doc; otherwise the helper drops `-t`
and the regenerated descriptors carry full type-info. Today the
probe fails on the bundled Cyclone 0.10.5 submodule; the option
lights up automatically the day the pin moves past the fixed
release.

## Test rpath / `LD_LIBRARY_PATH`

`packages/dds/nros-rmw-cyclonedds/tests/CMakeLists.txt` sets
`ENVIRONMENT "LD_LIBRARY_PATH=<prefix>/lib:..."` on every CTest
target so the binaries resolve `libddsc.so.0` from `build/install/`
instead of `/opt/ros/humble/lib/x86_64-linux-gnu/libddsc.so.0`. The
ROS 2 environment that `.envrc` sources adds the system path
first. Without the env override:

- `ldd` shows the system `libddsc.so` resolving first.
- The binary launches but crashes inside Cyclone with SIGSEGV
  because `nros_rmw_cyclonedds.a` was compiled against our 0.10.5
  headers and the runtime is the system Cyclone (different build
  flags, possibly different layout).

Downstream consumers (nros-cpp examples linking via
`add_subdirectory(<repo>)` with `NANO_ROS_RMW=cyclonedds`) inherit a
build-tree rpath via `CycloneDDS::ddsc`'s own `INTERFACE_LINK_OPTIONS`,
so the issue is contained to the in-tree CTest harness. Document
this in any external integration guide.

## Heap allocations on the data path

Each `publish_raw` allocates and frees a `desc->m_size`-byte sample
buffer. `try_recv_raw` calls `dds_take` with NULL pre-init, so
Cyclone allocates the typed sample internally and the backend
returns the loan after extracting bytes.

For embedded targets where this allocation traffic matters,
either:
- Re-use a per-publisher / per-subscriber sample buffer (small
  refactor, ~30 LOC).
- Move to the zero-copy fast path once the sertype lookup lands.

The smoke tests don't measure allocation pressure; profile before
deploying on Cortex-R52 with strict heap budgets.

## Boards (Phase 117.10 / 117.11)

`nros-board-fvp-aemv8r-smp` and `nros-board-s32z270dc2-r52` are
**not yet implemented**. Until they land:

- Cyclone backend works on POSIX (Linux, macOS) only.
- Zephyr Cortex-A / Cortex-R targets need:
  - `aarch64-zephyr-elf` toolchain in the Zephyr SDK install
    (Phase 117.B sub-fix to `scripts/zephyr/setup.sh`).
  - ARM FVP `Base_RevC_AEMv8R` or NXP S32Z evaluation board for
    runtime testing (separate downloads, license-gated).
  - Cyclone DDS cross-compiled against the Zephyr toolchain
    (untested; build flags need verification).

See `docs/roadmap/phase-117-cyclonedds-rmw.md` for the per-item
breakdown.

## Runtime type registry sizing (Phase 212.K.7)

Generated msg crates are RMW-agnostic — Cyclone DDS sertypes are
built lazily on first `create_publisher<M>` / `create_subscription<M>`
for a given message type and cached in a bounded `no_std` registry
inside `nros-rmw-cyclonedds`. The cap is a build-time env knob:

- **`NROS_CYCLONEDDS_MAX_TYPES`** — default **32**. Wired through
  the `nros-sizes` build probe (same pattern as
  `EXECUTOR_OPAQUE_U64S`). Each slot costs ~16 bytes static (one
  `u64` type-hash + one `NonNull<ddsi_sertype>`); default footprint
  ~512 bytes.
- Overflow on first-use registration trips a compile-time
  `const _: () = assert!(...)` from the `nros-sizes-build` hook —
  no runtime failure mode.
- Raise the knob for bridge / aggregator nodes that touch many
  distinct message types; lower it on Cortex-M0+ where every
  static byte counts.

The descriptor itself (the `ddsi_sertype` plus Cyclone's internal
type-cache entries) is still allocated from Cyclone's `ddsrt` heap.
On FreeRTOS + ThreadX that heap is `kEmbeddedCycloneConfig`'s fixed
pool (Phase 177.22) — pre-budget it for the worst-case set of
message types the participant will publish or subscribe to.

See section 212.K.7 of
`docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
for the full design + work-item ledger.

## No E2E message-integrity (safety-e2e / CRC)

The `safety-e2e` capability (CRC attach on publish + validate on receive, surfaced via
`ctx.integrity()` / `nros_subscription_try_recv_validated`) is **zenoh-only**. The CRC
machinery lives in the zenoh shim's wire attachment (`nros-rmw-zenoh`); CycloneDDS (and
XRCE) carry no `safety-e2e` feature, so a declared `[safety]` axis no-ops on them. The
`NANO_ROS_SAFETY_E2E=ON` CMake option **warns and is ignored** when `NANO_ROS_RMW` is not
`zenoh`. Adding a CycloneDDS integrity path (a DDS-side CRC + a C surface) is unscoped —
see [issue 0073](../issues/0073-safety-e2e-c-cpp-cmake-path-missing.md).
