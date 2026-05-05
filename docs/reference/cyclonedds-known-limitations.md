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

**Caveat — wire compat:** the envelope pattern is **not** the
upstream `rmw_cyclonedds_cpp` shape (which puts a
`cdds_request_header_t` inside the typed IDL). Service traffic
between nano-ros and stock ROS 2 nodes does **not** interoperate.
Same trade-off as our zenoh backend.

**Caveat — cap:** the server-side slot table is fixed at 32. A
server with more than 32 outstanding requests will report
`NROS_RMW_RET_WOULD_BLOCK` from `try_recv_request` until the
application drains via `send_reply`. Tune by editing
`kRequestSlots` in `src/service.cpp`.

**Caveat — Cyclone same-participant local-delivery race:**
creating two service clients on the same `nros_rmw_session_t`
back-to-back occasionally results in only the second writer
matching the server's reader (Cyclone 0.10.5 local-delivery
shortcut). Stagger client creation by ≥ 100 ms. Documented in
`tests/service_concurrent.cpp`; if it bites a real consumer,
move to one participant per service client.

## ROS 2 wire interop: untested vs stock `rmw_cyclonedds_cpp`

Phase 117.12 (POSIX E2E against a stock ROS 2 publisher /
subscriber on the same domain) is **not yet executed**. The
deserialise/reserialise path produces canonical XCDR1 native-
byte-order CDR — the same shape `rmw_cyclonedds_cpp` emits — so
basic pub/sub *should* interop, but no end-to-end test confirms it
yet.

**Path forward:** a CTest harness that runs a ROS 2 node from
`/opt/ros/humble` against an nros-rmw-cyclonedds peer on the same
domain, asserts byte-exact + structural equality on a `std_msgs/
String` round-trip. Blocked on Phase 117.12.

## QoS coverage

`make_dds_qos` honours the full `nros_rmw_qos_t` field set
(reliability, durability, history+depth, deadline, lifespan,
liveliness+lease) **except**:

- `MANUAL_BY_NODE` liveliness — folded to `MANUAL_BY_TOPIC` (Cyclone
  has no node-scoped variant).
- `max_blocking_time` on reliable writers — hard-coded to 100 ms
  to match `rmw_cyclonedds_cpp`. Surfacing it through
  `nros_rmw_qos_t._reserved` is a follow-up.

## Type discovery (XTypes metadata)

The codegen helper passes `idlc -t` which **omits the XTypes type-
information section** from the generated descriptor. Two reasons:

1. Cyclone 0.10.5's idlc reliably segfaults emitting type-info on
   our build (`-h` exits 1, files truncated, see
   `cyclonedds-interop.md`).
2. Type-info is optional on the wire — peers that need it fall
   back to typename matching, which is what nano-ros does anyway.

**Cost:** `ros2 topic echo --include-hidden-topics` works; full
`ros2 topic info -v` (which queries DCPSPublication / DCPSSubscription
for XTypes metadata) shows blank type info for nano-ros endpoints.

**Path forward:** rebuild Cyclone with type-info fixed, or carry a
patch on the submodule. Tracked separately.

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

Downstream consumers (e.g. nros-cpp examples linking via
`find_package(NanoRos NANO_ROS_RMW=cyclonedds)`) inherit a
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
