# ROS 2 rmw_zenoh Interoperability

This page describes how nros's zenoh backend speaks the `rmw_zenoh_cpp` wire
protocol: key expressions, liveliness tokens, QoS strings, CDR framing and the
per-message attachment. For the user steps (starting the router, running a
talker against `ros2 topic echo`, matching domain ID, QoS and edition), see the
book's [ROS 2 Interop](../../communicate/ros2-interop.html) page; this page does
not repeat them.

## Status: WORKING

nros <-> ROS 2 rmw_zenoh communication is fully operational as of January 2025.

## Architecture

```
┌─────────────────────────┐          ┌─────────────────────────┐
│   ROS 2 Node            │          │   nros Node             │
│   (rmw_zenoh_cpp)       │<-------->│   (zenoh-pico)          │
│                         │  Zenoh   │                         │
│   ros2 topic echo       │  Router  │   native-rs-talker      │
│   /chatter              │ (zenohd) │                         │
└─────────────────────────┘          └─────────────────────────┘
```

Both nodes connect to the same Zenoh router (zenohd). zenoh's router-free peer mode is not
compiled into the nano-ros shim — `NROS_SESSION_MODE=peer` is refused at session open (issue 0682).

## Protocol Requirements

### 1. Data Topic Key Expression Format

For ROS 2 Humble, the data key expression format is:

```
<domain_id>/<topic_name>/<type_name>/<type_hash>
```

**Important:** For Humble, use `TypeHashNotSupported` as the type hash (no `RIHS01_` prefix).

Example:
```
0/chatter/std_msgs::msg::dds_::String_/TypeHashNotSupported
```

**Note:** Newer ROS 2 versions (Iron+) use `RIHS01_<hash>` format with actual type hashes.

Implementation: `TopicInfo::to_key()` in the transport traits module

### 2. Liveliness Token Format

rmw_zenoh uses Zenoh liveliness tokens for discovery. Without these, `ros2 topic list` won't show nros topics.

**Node Liveliness Token:**
```
@ros2_lv/<domain>/<zid>/0/0/NN/%/%/<node_name>
```

**Publisher Liveliness Token:**
```
@ros2_lv/<domain>/<zid>/0/11/MP/%/%/<node_name>/<topic>/<type>/RIHS01_<hash>/<qos>
```

**Subscriber Liveliness Token:**
```
@ros2_lv/<domain>/<zid>/0/11/MS/%/%/<node_name>/<topic>/<type>/RIHS01_<hash>/<qos>
```

Key differences from data keyexprs:
- Liveliness tokens **do** use `RIHS01_` prefix
- Topic names use `%` instead of `/` (e.g., `%chatter` not `/chatter`)
- ZenohId must be in LSB-first hex format

Implementation: `Ros2Liveliness` in the transport/zenoh module

### 3. QoS String Format

The QoS portion of liveliness tokens uses this format:
```
reliability:durability:history,depth:deadline_sec,deadline_nsec:lifespan_sec,lifespan_nsec:liveliness,liveliness_sec,liveliness_nsec
```

Values:
- Reliability: 1=RELIABLE, 2=BEST_EFFORT
- Durability: 1=TRANSIENT_LOCAL, 2=VOLATILE
- History: 1=KEEP_LAST

For BEST_EFFORT/VOLATILE with KEEP_LAST depth 1:
```
2:2:1,1:,:,:,,
```

**Important:** Empty values default to RELIABLE which causes QoS mismatch. Always specify explicit values.


### 3b. TRANSIENT_LOCAL — the `@adv` cache keyexpr

phase-455 W5 / issue 1341. This section is MEASURED, from a
`RUST_LOG=zenoh=debug` router watching a stock `rmw_zenoh_cpp` 0.1.9
TRANSIENT_LOCAL pair on 2026-09-13, not read out of upstream source.

`rmw_zenoh_cpp` 0.1.9 builds its endpoints on zenoh's `ze_advanced_publisher` /
`ze_advanced_subscriber` (confirmed with `nm -D --undefined-only
librmw_zenoh_cpp.so`), so transient-local durability is served by
QUERY-ON-MATCH rather than by anything in the wire format above:

```
Declare queryable  <domain>/<topic>/<type>/<hash>/@adv/pub/<zid>/<eid>/_
Declare subscriber <domain>/<topic>/<type>/<hash>
Declare subscriber <domain>/<topic>/<type>/<hash>/@adv/pub/**
Route query    for <domain>/<topic>/<type>/<hash>/@adv/**
Route query    for <domain>/<topic>/<type>/<hash>/@adv/pub/<zid>/<eid>/_
```

* The PUBLISHER's cache is a queryable at `<topic keyexpr>/@adv/pub/<zid>/<eid>/_`.
  `@adv` is zenoh-ext's own prefix — the key format
  `${remaining:**}/@adv/${entity:*}/${zid:*}/${eid:*}/${meta:**}` is a literal
  string in `libzenohc.so`; `pub` is the entity and the trailing `_` is the
  empty metadata chunk.
* The SUBSCRIBER issues a GLOBAL history query at `<topic keyexpr>/@adv/**` when
  it is created. That intersects any publisher's cache key, and it is what
  serves a late joiner.
* The second query is late-joiner DETECTION: the subscriber also declares a
  liveliness subscriber on `<topic keyexpr>/@adv/pub/**` and queries each
  publisher whose `@adv` liveliness token appears after it.
* The reply carries the TOPIC keyexpr, not the queryable's, and the same
  attachment a live publication would have. The advanced subscriber accepts a
  reply keyexpr differing from the query's, which is what makes that legal.

nano-ros serves the publisher half of this
(`shim/publisher.rs::transient_local`) and declares no `@adv` liveliness token,
so it answers the global query and not the per-publisher one. The subscriber
half — querying a stock transient-local publisher on match — is not
implemented, and `shim/qos.rs::admit` refuses `TRANSIENT_LOCAL` on a
subscription rather than pretending.

### 4. CDR Message Format

Messages use CDR (Common Data Representation) little-endian encoding:
```
[0x00, 0x01, 0x00, 0x00]  // CDR encapsulation header (LE)
[... CDR payload ...]     // Message data
```

Implementation: the serdes crate

### 5. RMW Attachment Format

rmw_zenoh requires metadata attached to each published message. For Humble, the serialization format is:

| Field | Size | Format |
|-------|------|--------|
| sequence_number | 8 bytes | int64, little-endian |
| timestamp | 8 bytes | int64, little-endian (nanoseconds) |
| gid_length | 1 byte | VLE encoded (value: 16) |
| gid | 16 bytes | Random bytes, constant per publisher |

Total: 33 bytes

The serialization uses Zenoh's `ze_serializer` to ensure compatibility with zenoh-cpp's deserializer.

Implementation: `serialize_rmw_attachment()` in `zenoh-pico/src/serializer.rs`

### 6. ZenohId Format

The ZenohId in liveliness tokens must be formatted in LSB-first (little-endian) hex format.

```rust
// Correct: LSB-first
fn to_hex_string(&self) -> String {
    let mut hex = String::new();
    for byte in self.bytes.iter() {  // LSB first
        write!(&mut hex, "{:02x}", byte).ok();
    }
    hex
}
```

## Common Issues and Solutions

These are failures of the protocol encoding itself, seen while developing the
backend.

### Issue: Discovery works but no messages received

**Symptom:** `ros2 topic list` shows the topic, `ros2 topic info` shows correct QoS, but `ros2 topic echo` receives nothing.

**Cause:** Data keyexpr format mismatch.

**Solution:** For Humble, don't use `RIHS01_` prefix in data keyexprs:
- Wrong: `0/chatter/std_msgs::msg::dds_::String_/RIHS01_TypeHashNotSupported`
- Correct: `0/chatter/std_msgs::msg::dds_::String_/TypeHashNotSupported`

### Issue: Topic not visible in ros2 topic list

**Symptom:** nros publishes but ROS 2 doesn't see the topic.

**Cause:** Liveliness token format incorrect.

**Solutions:**
1. Check ZenohId is LSB-first hex format
2. Ensure topic name uses `%` prefix (e.g., `%chatter`)
3. Verify QoS string has explicit values, not defaults

User-level failures (QoS mismatch, `rmw_zenoh_cpp` left in peer mode, domain
ID or edition mismatch) are covered in the book's
[ROS 2 Interop → Troubleshooting](../../communicate/ros2-interop.html#troubleshooting).

## Implementation Status

| Component | Status | Location |
|-----------|--------|----------|
| Data keyexpr format | Done | `TopicInfo::to_key()` |
| Liveliness tokens | Done | `Ros2Liveliness` |
| QoS format | Done | `2:2:1,1:,:,:,,` |
| CDR serialization | Done | serdes crate |
| RMW attachment | Done | `serialize_rmw_attachment()` |
| ZenohId format | Done | `ZenohId::to_hex_string()` |
| Node integration | Done | `ConnectedNode` |

## Version Compatibility

| ROS 2 Version | Type Hash Format | Status |
|---------------|------------------|--------|
| Humble | `TypeHashNotSupported` | Working (default edition) |
| Iron, Jazzy | `RIHS01_<sha256>` | Working — the hash is computed at codegen time by the REP-2011 engine in `packages/cli/rosidl-resolve/src/rihs.rs` (phase-304, RFC-0056) |

The edition is chosen per image (`ros_edition` in `system.toml`); a
Humble-edition image and a Jazzy bus do not discover each other, because the
key expressions differ.

## References

- [rmw_zenoh design docs](https://github.com/ros2/rmw_zenoh/blob/humble/docs/design.md)
- [Zenoh serialization RFC](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md)
- [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico)
